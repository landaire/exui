//! XuiEdit (0x9216a298), partial: the text buffer and its display copy,
//! TextLimit, AllowedChars, character input, caret movement by index, the
//! source text answer, line wrapping and the vertical scrolling messages of
//! multi-line edits (top line, scroll to line, line counts, smooth scroll
//! settings). Not implemented: caret placement, horizontal view shifts, the
//! scroll arrows, password characters and scrolling by the up and down keys.

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::mem;

use exui_core::schema::ClassId;
use exui_core::schema::PropDef;

use crate::control::ClassState;
use crate::control::props;
use crate::input::VirtualKey;
use crate::message::Body;
use crate::message::Context;
use crate::message::EditLineInfoKind;
use crate::message::LinesPerSecond;
use crate::message::Message;
use crate::message::NavKey;
use crate::message::SmoothScrollParams;
use crate::message::TimerId;
use crate::message::key_to_nav;
use crate::scene::ElementId;
use crate::scene::PropValue;
use crate::scene::text_line_starts;
use crate::text::TextStyle;
use crate::time::Milliseconds;
use crate::time::Tick;

#[derive(Clone, Debug, Default)]
pub struct EditState {
    pub buffer: Option<Vec<u16>>,
    /// Display copy (`XuiEdit_BuildDisplayText` 0x921580f8): the text from
    /// the first shown character, rebuilt only where the original rebuilds
    /// it. Presenters share it instead of copying the text.
    display: Option<Rc<[u16]>>,
    pub caret: usize,
    pub caret_element: Option<ElementId>,
    pub text_element: Option<ElementId>,
    /// AllowedChars range list (+0x98). Empty allows every character, as
    /// the original's null list does.
    allowed: Vec<CharRange>,
    scroll: Scroll,
}

impl EditState {
    pub fn new() -> Self {
        Self::default()
    }

    /// `XuiEdit_BuildDisplayText` without PasswordChar. Editing can leave
    /// the first shown character past the end, where it copies nothing.
    fn build_display_text(&mut self) {
        let first = self.scroll.first_char;
        self.display = self
            .buffer
            .as_deref()
            .map(|buffer| Rc::from(&buffer[first.min(buffer.len())..]));
    }

    fn allows(&self, character: u16) -> bool {
        self.allowed.is_empty()
            || self
                .allowed
                .iter()
                .any(|range| (range.low..=range.high).contains(&character))
    }
}

/// One node of the AllowedChars range list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CharRange {
    low: u16,
    high: u16,
}

/// Wrapped lines and the vertical scroll position (+0x70 to +0xc4).
#[derive(Clone, Debug)]
struct Scroll {
    /// +0x74: index of the first displayed character.
    first_char: usize,
    /// +0x70.
    first_line: u32,
    /// +0x78 and +0x7c. Kept while the text or the font is missing.
    line_starts: Vec<usize>,
    /// +0x88: LineSpacingAdjust plus the font line height. Kept while the
    /// font is missing.
    line_pitch: u32,
    /// +0xc4: height of `Text`.
    text_height: f32,

    /// +0x9c, +0xa0: pixels.
    position: f32,
    target: f32,
    /// +0xa8: pixels per second with SmoothScroll, else pixels per step.
    velocity: f32,

    /// +0xac to +0xb4.
    start_speed: LinesPerSecond,
    acceleration: f32,
    max_speed: LinesPerSecond,

    /// +0xb8.
    last_step: Tick,
    /// +0xbc.
    timer_active: bool,
}

impl Default for Scroll {
    /// Constructor values (0x9213f0a8).
    fn default() -> Self {
        Self {
            first_char: 0,
            first_line: 0,
            line_starts: Vec::new(),
            line_pitch: 0,
            text_height: 0.0,
            position: 0.0,
            target: 0.0,
            velocity: 0.0,
            start_speed: LinesPerSecond(3.0),
            acceleration: 0.02,
            max_speed: LinesPerSecond(20.0),
            last_step: Tick(0),
            timer_active: false,
        }
    }
}

fn edit<'c>(cx: &'c mut Context<'_, '_>, element: ElementId) -> Option<&'c mut EditState> {
    match cx.class_state(element)? {
        ClassState::Edit(state) => Some(state),
        _ => None,
    }
}

fn flag(cx: &Context<'_, '_>, element: ElementId, def: &'static PropDef) -> bool {
    matches!(
        cx.tree.property(element, &[def], None),
        Some(PropValue::Bool(true))
    )
}

fn read_only(cx: &Context<'_, '_>, element: ElementId) -> bool {
    // Multiline and SmoothScroll force ReadOnly.
    flag(cx, element, props::edit_read_only())
        || flag(cx, element, props::edit_multiline())
        || flag(cx, element, props::edit_smooth_scroll())
}

fn multiline(cx: &Context<'_, '_>, element: ElementId) -> bool {
    flag(cx, element, props::edit_multiline()) || flag(cx, element, props::edit_smooth_scroll())
}

/// `XuiEdit_FindVisualChildren` 0x9215eaa0, caret and text parts only.
fn find_children(cx: &mut Context<'_, '_>, element: ElementId) {
    let visual = cx.control_visual(element);
    let caret_id: Vec<u16> = "Caret".encode_utf16().collect();
    let text_id: Vec<u16> = "Text".encode_utf16().collect();
    let caret = visual.and_then(|visual| cx.tree.child_by_id(visual, &caret_id));
    let text = visual.and_then(|visual| cx.tree.child_by_id(visual, &text_id));
    if let Some(state) = edit(cx, element) {
        state.caret_element = caret;
        state.text_element = text;
    }

    if let Some(text) = text {
        apply_multiline_style(cx, element, text);
    }

    if let Some(caret) = caret {
        let show = !read_only(cx, element) && cx.is_enabled(element);
        cx.set_show(caret, show);
    }
}

/// The Multiline rule for the `Text` TextStyle: wrapping and the multi-line
/// measurer on, or no-wrap and single-line on.
fn apply_multiline_style(cx: &mut Context<'_, '_>, element: ElementId, text: ElementId) {
    const WRAP_BITS: u32 = TextStyle::NO_WRAP | TextStyle::SINGLE_LINE;

    let Some(PropValue::Unsigned(style)) =
        cx.tree
            .property(text, &[props::text_presenter_style()], None)
    else {
        return;
    };
    let style = if multiline(cx, element) {
        (style & !WRAP_BITS) | TextStyle::MULTI_LINE_MEASURE
    } else {
        (style | WRAP_BITS) & !TextStyle::MULTI_LINE_MEASURE
    };

    // The definition is an unsigned property, the only type checked.
    let _ = cx.tree.set_property(
        text,
        &[props::text_presenter_style()],
        None,
        PropValue::Unsigned(style),
    );
}

pub(super) fn on_edit(
    cx: &mut Context<'_, '_>,
    element: ElementId,
    class: ClassId,
    message: &mut Message,
) {
    match &message.body {
        Body::CreateVisual => {
            let mut forwarded = Message::new(Body::CreateVisual);
            cx.send_to_base(element, class, &mut forwarded);
            find_children(cx, element);
            layout(cx, element);
            update_view(cx, element);
            message.handled = true;
        }
        Body::Init { .. } => load_allowed_chars(cx, element),
        Body::InitComplete => {
            if cx.control_visual(element).is_none() {
                cx.send_body(element, Body::CreateResources);
            }
            find_children(cx, element);
            layout(cx, element);
        }
        Body::KeyDown(key) => {
            let key = *key;
            if key.character.is_some() {
                message.handled = true;
                return;
            }
            if edit(cx, element).is_none_or(|state| state.buffer.is_none()) {
                return;
            }
            if key.key == VirtualKey::Clear {
                set_text(cx, element, None);
                message.handled = true;
                return;
            }

            let keyboard = !cx.runtime.keyboard_keys_disabled;
            let moved = match key_to_nav(&key, keyboard) {
                Some(NavKey::Left) => move_caret(cx, element, false),
                Some(NavKey::Right) => move_caret(cx, element, true),
                _ => false,
            };
            if moved {
                message.handled = true;
            }
        }
        Body::Char(data) => {
            let character = data.character;
            message.handled = true;
            match character {
                0x08 => backspace(cx, element),
                0x09 | 0x0d | 0x1b => {}
                character if character < 0x20 => {}
                character => {
                    let Some(state) = edit(cx, element) else {
                        return;
                    };
                    if !state.allows(character) {
                        return;
                    }
                    let caret = state.caret;
                    insert_text(cx, element, caret, &[character]);
                }
            }
        }
        Body::Timer { id } if *id == TimerId::CONTROL => {
            message.handled = true;
            smooth_scroll_step(cx, element, class);
        }
        Body::UpdateState { .. } => {
            let mut forwarded = Message::new(message.body.clone());
            cx.send_to_base(element, class, &mut forwarded);
            if let Some(caret) = edit(cx, element).and_then(|state| state.caret_element) {
                cx.send_body(caret, message.body.clone());
            }
            message.handled = true;
        }
        Body::GetSourceText(query) => {
            let extended = query.extended;
            let read_only = read_only(cx, element);
            let Some(state) = edit(cx, element) else {
                return;
            };

            // Presenters pull the extended answer every frame; it shares
            // the display copy.
            let text = if extended {
                state.display.clone()
            } else {
                state.buffer.as_deref().map(Rc::<[u16]>::from)
            };
            let line_top = state
                .scroll
                .line_pitch
                .wrapping_mul(state.scroll.first_line);
            let y_offset = state.scroll.position - line_top as f32;

            if let Body::GetSourceText(query) = &mut message.body {
                query.text = text;
                if extended {
                    query.y_offset = y_offset;
                    if !read_only {
                        query.style &= !TextStyle::ELLIPSIS;
                    }
                }
            }
            message.handled = true;
        }
        Body::SetSourceText { text, .. } => {
            let text = text.clone();
            // Text over the limit falls through to XuiControl.
            message.handled = set_text(cx, element, text.as_deref());
        }
        Body::EditGetTopLine { current, .. } => {
            let current = *current;
            message.handled = true;
            let line = edit(cx, element).and_then(|state| {
                let scroll = &state.scroll;
                let value = if current {
                    scroll.position
                } else {
                    scroll.target
                };
                // The original traps on a zero pitch.
                top_line(value, scroll.line_pitch)
            });
            if let Body::EditGetTopLine { line: answer, .. } = &mut message.body {
                *answer = line;
            }
        }
        Body::EditScrollToLine { line, animate } => {
            let (line, animate) = (*line, *animate);
            message.handled = true;
            scroll_to_line(cx, element, class, line, animate);
        }
        Body::EditLineInfo { kind, current, .. } => {
            let (kind, current) = (*kind, *current);
            message.handled = true;
            let value = edit(cx, element).and_then(|state| line_info(&state.scroll, kind, current));
            if let Body::EditLineInfo { value: answer, .. } = &mut message.body {
                *answer = value;
            }
        }
        Body::EditGetSmoothScroll { .. } => {
            message.handled = true;
            let enabled = flag(cx, element, props::edit_smooth_scroll());
            let params = edit(cx, element).map(|state| SmoothScrollParams {
                enabled,
                start_speed: state.scroll.start_speed,
                acceleration: state.scroll.acceleration,
                max_speed: state.scroll.max_speed,
            });
            if let Body::EditGetSmoothScroll { params: answer } = &mut message.body {
                *answer = params;
            }
        }
        Body::EditSetSmoothScroll { params } => {
            let params = *params;
            message.handled = true;
            // Otherwise E_INVALIDARG and nothing changes.
            if params.max_speed.0 > 0.0 && params.start_speed.0 > 0.0 && params.acceleration >= 0.0
            {
                set_smooth_scroll(cx, element, params.enabled);
                if let Some(state) = edit(cx, element) {
                    state.scroll.max_speed = params.max_speed;
                    state.scroll.start_speed = params.start_speed;
                    state.scroll.acceleration = params.acceleration;
                }
            }
        }
        Body::SizeChanged => {
            layout(cx, element);
            update_view(cx, element);
        }
        _ => {}
    }
}

/// `fctiwz`: truncating float to i32 conversion; NaN gives `i32::MIN`.
fn truncate(value: f32) -> i32 {
    if value.is_nan() {
        i32::MIN
    } else {
        value as i32
    }
}

/// Line of a scroll position: the position truncated to an integer and
/// divided as an unsigned value by the pitch. `None` for a zero pitch.
fn top_line(position: f32, pitch: u32) -> Option<u32> {
    (truncate(position) as u32).checked_div(pitch)
}

/// Message 0x7ef. `None` where the original traps on a zero pitch.
fn line_info(scroll: &Scroll, kind: EditLineInfoKind, current: bool) -> Option<u32> {
    let line_count = scroll.line_starts.len() as u32;
    let page = truncate(scroll.text_height / scroll.line_pitch as f32);

    match kind {
        EditLineInfoKind::Lines => Some(line_count),
        EditLineInfoKind::ShownLines => {
            let value = if current {
                scroll.position
            } else {
                scroll.target
            };
            let rest = line_count.wrapping_sub(top_line(value, scroll.line_pitch)?);

            // Signed comparison.
            Some(page.min(rest as i32) as u32)
        }
        EditLineInfoKind::PageLines => Some(page as u32),
    }
}

/// `XuiEdit_SetText` 0x92168c30. The scroll position is kept.
fn set_text(cx: &mut Context<'_, '_>, element: ElementId, text: Option<&[u16]>) -> bool {
    let Some(limit) = text_limit(cx, element) else {
        return false;
    };
    if text.is_some_and(|text| text.len() > limit) {
        return false;
    }

    let Some(state) = edit(cx, element) else {
        return false;
    };
    state.buffer = text.map(Into::into);
    state.caret = 0;
    state.scroll.first_line = 0;
    state.scroll.first_char = 0;

    layout(cx, element);
    update_view(cx, element);

    true
}

/// `XuiEdit_Layout` 0x92162a18 without the text measurement and arrows,
/// which only feed the scroll arrows.
fn layout(cx: &mut Context<'_, '_>, element: ElementId) {
    let Some(state) = edit(cx, element) else {
        return;
    };
    let text_element = state.text_element;
    let buffer = state.buffer.take();
    let mut starts = mem::take(&mut state.scroll.line_starts);

    let mut wrapped = Vec::new();
    let metrics = text_element.and_then(|text| {
        let runtime = &mut *cx.runtime;
        text_line_starts(
            cx.tree,
            text,
            buffer.as_deref().unwrap_or(&[]),
            &runtime.fonts,
            &mut runtime.text,
            &mut wrapped,
        )
    });
    if metrics.is_some() && buffer.is_some() {
        starts = wrapped;
    }

    let Some(state) = edit(cx, element) else {
        return;
    };
    state.buffer = buffer;
    state.build_display_text();
    state.scroll.line_starts = starts;
    match metrics {
        Some(metrics) => {
            state.scroll.line_pitch = metrics
                .line_height
                .wrapping_add_signed(metrics.line_spacing_adjust);
            state.scroll.text_height = metrics.box_height;
        }
        None => state.scroll.text_height = 0.0,
    }
}

/// `XuiEdit_UpdateView` 0x921626f0, the first line and first character of a
/// multi-line view.
fn update_view(cx: &mut Context<'_, '_>, element: ElementId) {
    let multiline = multiline(cx, element);
    let Some(state) = edit(cx, element) else {
        return;
    };
    if state.text_element.is_none() {
        return;
    }

    let scroll = &mut state.scroll;
    // The original traps on a zero pitch.
    let Some(first_line) = top_line(scroll.position, scroll.line_pitch) else {
        return;
    };
    scroll.first_line = first_line;

    // The original reads past the line list when a shorter text keeps an
    // old scroll position; nothing is read here.
    if multiline
        && let Some(&start) = scroll.line_starts.get(first_line as usize)
        && start != scroll.first_char
    {
        scroll.first_char = start;
        state.build_display_text();
    }
}

/// `XuiEdit_ScrollToLine` 0x92168538. No clamp to the last page.
fn scroll_to_line(
    cx: &mut Context<'_, '_>,
    element: ElementId,
    class: ClassId,
    line: u32,
    animate: bool,
) {
    let ready = read_only(cx, element) && multiline(cx, element);
    let Some(state) = edit(cx, element) else {
        return;
    };
    if state.buffer.is_none() || !ready {
        return;
    }

    let scroll = &mut state.scroll;
    if line as usize >= scroll.line_starts.len() {
        return;
    }

    let pitch = scroll.line_pitch as f32;
    scroll.target = scroll.line_pitch.wrapping_mul(line) as f32;
    if animate {
        let direction = if scroll.target >= scroll.position {
            1.0
        } else {
            -1.0
        };
        scroll.velocity = direction * scroll.start_speed.0 * pitch;
        start_scroll_timer(cx, element, class);
    } else {
        scroll.position = scroll.target;
    }

    update_view(cx, element);
}

/// `XuiEdit_StartScrollTimer` 0x921581d8: a 16 ms timer on the XuiEdit level.
fn start_scroll_timer(cx: &mut Context<'_, '_>, element: ElementId, class: ClassId) {
    const PERIOD_MS: Milliseconds = Milliseconds(16);

    let now = cx.runtime.timers.now();
    let Some(state) = edit(cx, element) else {
        return;
    };
    if state.scroll.timer_active {
        return;
    }

    state.scroll.last_step = now;
    state.scroll.timer_active = true;
    let handle = cx.level_handle(element, class);
    cx.runtime
        .timers
        .set_for(handle, TimerId::CONTROL, PERIOD_MS);
}

/// `XuiEdit_SmoothScrollStep` 0x92162d58.
fn smooth_scroll_step(cx: &mut Context<'_, '_>, element: ElementId, class: ClassId) {
    const MAX_STEP_MS: f32 = 33.333_332;
    const SECONDS_PER_MS: f32 = 0.001;

    let smooth = flag(cx, element, props::edit_smooth_scroll());
    let now = cx.runtime.timers.now();
    let Some(state) = edit(cx, element) else {
        return;
    };
    let scroll = &mut state.scroll;

    let velocity = scroll.velocity;
    if smooth && velocity != 0.0 {
        // A wrapped tick count loses one more millisecond.
        let elapsed = if now >= scroll.last_step {
            now.elapsed_since(scroll.last_step).0
        } else {
            now.elapsed_since(scroll.last_step).0.wrapping_sub(1)
        };
        let step_ms = (elapsed as f32).min(MAX_STEP_MS);
        scroll.last_step = now;
        scroll.position += step_ms * SECONDS_PER_MS * velocity;
    } else {
        scroll.position += velocity;
    }

    let arrived = (velocity > 0.0 && scroll.position >= scroll.target)
        || (velocity < 0.0 && scroll.position <= scroll.target);
    if arrived {
        scroll.position = scroll.target;
        scroll.velocity = 0.0;
        scroll.timer_active = false;
    }

    if smooth && scroll.velocity != 0.0 {
        let limit = scroll.line_pitch as f32 * scroll.max_speed.0;
        let grown = (scroll.acceleration + 1.0) * scroll.velocity;
        scroll.velocity = if scroll.velocity > 0.0 {
            grown.min(limit)
        } else {
            grown.max(-limit)
        };
    }

    if arrived {
        let handle = cx.level_handle(element, class);
        cx.runtime.timers.kill_for(handle, TimerId::CONTROL);
    }
    if velocity != 0.0 {
        update_view(cx, element);
    }
}

/// `XuiEdit_SetSmoothScroll` 0x921682d8: setting also sets ReadOnly and
/// Multiline.
fn set_smooth_scroll(cx: &mut Context<'_, '_>, element: ElementId, enable: bool) {
    let defs: &[&'static PropDef] = if enable {
        &[
            props::edit_read_only(),
            props::edit_multiline(),
            props::edit_smooth_scroll(),
        ]
    } else {
        &[props::edit_smooth_scroll()]
    };
    for def in defs {
        // Boolean definitions; the type check cannot fail.
        let _ = cx
            .tree
            .set_property(element, &[def], None, PropValue::Bool(enable));
    }

    let (caret, text) = edit(cx, element).map_or((None, None), |state| {
        (state.caret_element, state.text_element)
    });
    if enable {
        if let Some(caret) = caret {
            cx.set_show(caret, false);
        }
        if let Some(text) = text {
            apply_multiline_style(cx, element, text);
        }
    }

    update_view(cx, element);
}

/// `XuiEdit` InsertText 0x92169fa0.
fn insert_text(cx: &mut Context<'_, '_>, element: ElementId, index: usize, text: &[u16]) {
    if read_only(cx, element) || !cx.is_enabled(element) {
        return;
    }

    let Some(limit) = text_limit(cx, element) else {
        return;
    };
    let Some(state) = edit(cx, element) else {
        return;
    };
    let length = state.buffer.as_ref().map_or(0, Vec::len);
    if length + text.len() > limit || index > length {
        return;
    }

    state
        .buffer
        .get_or_insert_with(Vec::new)
        .splice(index..index, text.iter().copied());
    state.build_display_text();
    if index <= state.caret {
        state.caret += text.len();
    }
}

/// `XuiEdit` Backspace 0x9216a1d0.
fn backspace(cx: &mut Context<'_, '_>, element: ElementId) {
    if read_only(cx, element) || !cx.is_enabled(element) {
        return;
    }

    let Some(state) = edit(cx, element) else {
        return;
    };
    let Some(buffer) = &mut state.buffer else {
        return;
    };
    if state.caret == 0 || state.caret > buffer.len() {
        return;
    }

    buffer.remove(state.caret - 1);
    state.caret -= 1;
    state.build_display_text();
}

/// TextLimit (+0x90). `None` only for a freed element.
fn text_limit(cx: &Context<'_, '_>, element: ElementId) -> Option<usize> {
    match cx
        .tree
        .property(element, &[props::edit_text_limit()], None)?
    {
        PropValue::Unsigned(limit) => Some(limit as usize),
        _ => None,
    }
}

/// AllowedChars parser 0x92156de0: literal characters and `a-b` ranges. A
/// `-` as the last character is literal. `None` for a `-` at the start or
/// after another `-`, or a range with its low end above its high end.
fn parse_allowed_chars(text: &[u16]) -> Option<Vec<CharRange>> {
    const DASH: u16 = b'-' as u16;

    // The original reads a terminated string.
    let text = text.split(|unit| *unit == 0).next().unwrap_or_default();
    let mut ranges = Vec::new();
    let mut pending: Option<u16> = None;
    let mut in_range = false;
    for (index, &character) in text.iter().enumerate() {
        if character == DASH && index + 1 != text.len() {
            if in_range || pending.is_none() {
                return None;
            }
            in_range = true;
            continue;
        }

        if in_range {
            let low = pending.take()?;
            if low > character {
                return None;
            }
            ranges.push(CharRange {
                low,
                high: character,
            });
            in_range = false;
            continue;
        }

        if let Some(single) = pending.replace(character) {
            ranges.push(CharRange {
                low: single,
                high: single,
            });
        }
    }
    if let Some(single) = pending {
        ranges.push(CharRange {
            low: single,
            high: single,
        });
    }

    Some(ranges)
}

/// Runs the AllowedChars setter for the value loaded from the XUR, which
/// the loader stores without its setter.
fn load_allowed_chars(cx: &mut Context<'_, '_>, element: ElementId) {
    let Some(text) = cx.tree.string(element, &[props::edit_allowed_chars()]) else {
        return;
    };

    let text = text.to_vec();
    cx.edit_set_allowed_chars(element, Some(&text));
}

impl Context<'_, '_> {
    /// AllowedChars setter 0x92158088. Null keeps the range list; a
    /// malformed string leaves no list and a null string.
    pub(crate) fn edit_set_allowed_chars(&mut self, element: ElementId, text: Option<&[u16]>) {
        let parsed = text.map(parse_allowed_chars);
        let stored: Option<Box<[u16]>> = match &parsed {
            Some(Some(_)) => text.map(Into::into),
            _ => None,
        };

        // A String definition; the type check cannot fail.
        let _ = self.tree.set_property(
            element,
            &[props::edit_allowed_chars()],
            None,
            PropValue::String(stored),
        );

        let Some(parsed) = parsed else {
            return;
        };
        if let Some(state) = edit(self, element) {
            state.allowed = parsed.unwrap_or_default();
        }
    }

    /// TextLimit setter 0x921683b0 after the store: the text is cut at the
    /// limit and the caret moved inside it. The display copy is not rebuilt.
    pub(crate) fn edit_apply_text_limit(&mut self, element: ElementId) {
        let Some(limit) = text_limit(self, element) else {
            return;
        };
        let Some(state) = edit(self, element) else {
            return;
        };

        if let Some(buffer) = &mut state.buffer {
            buffer.truncate(limit);
        }
        if limit < state.caret {
            state.caret = limit;
        }

        update_view(self, element);
    }
}

fn move_caret(cx: &mut Context<'_, '_>, element: ElementId, right: bool) -> bool {
    if read_only(cx, element) || !cx.is_enabled(element) {
        return false;
    }

    let Some(state) = edit(cx, element) else {
        return false;
    };
    let length = state.buffer.as_ref().map_or(0, Vec::len);
    if right && state.caret < length {
        state.caret += 1;
        return true;
    }
    if !right && state.caret > 0 {
        state.caret -= 1;
        return true;
    }

    false
}

#[cfg(test)]
mod tests;

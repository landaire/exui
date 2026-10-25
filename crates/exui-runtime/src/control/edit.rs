//! XuiEdit (0x9216a298): the text buffer and its display copy, TextLimit,
//! AllowedChars, PasswordChar, character input, caret moves and caret
//! placement, horizontal view shifts of single-line edits, line wrapping and
//! vertical scrolling of multi-line edits, the scroll arrows and the edit
//! messages 0x2c to 0x2e and 0x7ea to 0x7f3.

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cmp::Ordering;
use core::iter;
use core::mem;
use core::ptr;

use exui_core::Registry;
use exui_core::schema::ClassId;
use exui_core::schema::PropDef;
use exui_core::schema::xui;
use exui_core::value::Vector3;
use exui_core::xur::ObjectRef;

use crate::control::ClassState;
use crate::control::props;
use crate::input::VirtualKey;
use crate::message::Body;
use crate::message::BuiltinNotify;
use crate::message::Context;
use crate::message::EditLineInfoKind;
use crate::message::LinesPerSecond;
use crate::message::Message;
use crate::message::NavKey;
use crate::message::NotifyCode;
use crate::message::ScrollEndState;
use crate::message::SmoothScrollParams;
use crate::message::TimerId;
use crate::message::key_to_nav;
use crate::scene::EditMeasure;
use crate::scene::ElementId;
use crate::scene::ElementTree;
use crate::scene::PropValue;
use crate::scene::TextBounds;
use crate::scene::edit_text_bounds;
use crate::scene::text_line_starts;
use crate::text::TextStyle;
use crate::time::Milliseconds;
use crate::time::Tick;

#[derive(Clone, Debug, Default)]
pub struct EditState {
    buffer: Option<Vec<u16>>,
    /// Display copy (`XuiEdit_BuildDisplayText` 0x921580f8): the text from
    /// the first shown character, or as many password characters, rebuilt
    /// only where the original rebuilds it. Presenters share it instead of
    /// copying the text.
    display: Option<Rc<[u16]>>,
    /// +0x6c: caret index relative to the first shown character. Index
    /// arithmetic wraps in 32 bits as in the original.
    caret_offset: u32,
    caret_element: Option<ElementId>,
    text_element: Option<ElementId>,
    arrows: Arrows,
    /// AllowedChars range list (+0x98). Empty allows every character, as
    /// the original's null list does.
    allowed: Vec<CharRange>,
    scroll: Scroll,
}

impl EditState {
    pub fn new() -> Self {
        Self::default()
    }

    /// `XuiEdit_BuildDisplayText`. Editing can leave the first shown
    /// character past the end, where it copies nothing.
    fn build_display_text(&mut self, password: Option<u16>) {
        let first = self.scroll.first_char as usize;
        self.display = self.buffer.as_deref().map(|buffer| {
            let shown = buffer.get(first..).unwrap_or_default();
            match password {
                Some(character) => iter::repeat_n(character, shown.len()).collect(),
                None => Rc::from(shown),
            }
        });
    }

    /// Caret index in the text: first shown character plus caret offset.
    fn caret(&self) -> u32 {
        self.scroll.first_char.wrapping_add(self.caret_offset)
    }

    fn text_len(&self) -> u32 {
        self.buffer.as_ref().map_or(0, |buffer| buffer.len() as u32)
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

/// The four scroll arrow parts of the visual.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arrow {
    Left,
    Right,
    Up,
    Down,
}

/// Arrow record (+0x4, +0x14, +0x24, +0x34).
#[derive(Clone, Copy, Debug)]
struct ScrollArrow {
    element: Option<ElementId>,
    current: ScrollEndState,
    requested: ScrollEndState,
    /// Cleared while the arrow shows the scrolling state; a state other
    /// than scrolling waits until the arrow reports the end (notify 5).
    ready: bool,
}

impl Default for ScrollArrow {
    /// Constructor values (0x9213f0a8).
    fn default() -> Self {
        Self {
            element: None,
            current: ScrollEndState::Normal,
            requested: ScrollEndState::Normal,
            ready: true,
        }
    }
}

#[derive(Clone, Debug, Default)]
struct Arrows {
    left: ScrollArrow,
    right: ScrollArrow,
    up: ScrollArrow,
    down: ScrollArrow,
}

impl Arrows {
    fn get_mut(&mut self, arrow: Arrow) -> &mut ScrollArrow {
        match arrow {
            Arrow::Left => &mut self.left,
            Arrow::Right => &mut self.right,
            Arrow::Up => &mut self.up,
            Arrow::Down => &mut self.down,
        }
    }

    /// The arrow whose part is `element`, in the order the original tests
    /// them.
    fn find(&self, element: ElementId) -> Option<Arrow> {
        [
            (Arrow::Left, &self.left),
            (Arrow::Right, &self.right),
            (Arrow::Up, &self.up),
            (Arrow::Down, &self.down),
        ]
        .into_iter()
        .find(|(_, record)| record.element == Some(element))
        .map(|(arrow, _)| arrow)
    }
}

/// Layout values and the scroll position (+0x60 to +0xc4).
#[derive(Clone, Debug)]
struct Scroll {
    /// +0x74: index of the first displayed character.
    first_char: u32,
    /// +0x70.
    first_line: u32,
    /// +0x78 and +0x7c. Kept while the text or the font is missing.
    line_starts: Vec<usize>,
    /// +0x88: LineSpacingAdjust plus the font line height. Kept while the
    /// font is missing.
    line_pitch: u32,
    /// +0x60 and +0x80: TextStyle and LineSpacingAdjust of `Text`.
    style: TextStyle,
    line_spacing_adjust: i32,
    /// +0xc0, +0xc4: size of `Text`.
    text_width: f32,
    text_height: f32,

    /// +0x9c, +0xa0: pixels.
    position: f32,
    target: f32,
    /// +0xa4.
    max_position: f32,
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
            style: TextStyle(0),
            line_spacing_adjust: 0,
            text_width: 0.0,
            text_height: 0.0,
            position: 0.0,
            target: 0.0,
            max_position: 0.0,
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

/// `XuiEdit_FindVisualChildren` 0x9215eaa0. Without a visual the arrow
/// handles are kept.
fn find_children(cx: &mut Context<'_, '_>, element: ElementId) {
    const ARROWS: [(&str, Arrow); 4] = [
        ("ScrollLeft", Arrow::Left),
        ("ScrollRight", Arrow::Right),
        ("ScrollUp", Arrow::Up),
        ("ScrollDown", Arrow::Down),
    ];

    let visual = cx.control_visual(element);
    let child = |cx: &Context<'_, '_>, id: &str| {
        let id: Vec<u16> = id.encode_utf16().collect();
        visual.and_then(|visual| cx.tree.child_by_id(visual, &id))
    };
    let caret = child(cx, "Caret");
    let text = child(cx, "Text");
    let arrows = ARROWS.map(|(id, arrow)| (arrow, child(cx, id)));
    let Some(state) = edit(cx, element) else {
        return;
    };
    state.caret_element = caret;
    state.text_element = text;
    if visual.is_none() {
        return;
    }

    for (arrow, part) in arrows {
        state.arrows.get_mut(arrow).element = part;
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
        Body::InitComplete => {
            if cx.control_visual(element).is_none() {
                cx.send_body(element, Body::CreateResources);
                if let Some(caret) = edit(cx, element).and_then(|state| state.caret_element) {
                    cx.send_body(caret, Body::CreateResources);
                }
            }
            find_children(cx, element);
            layout(cx, element);
        }
        Body::Notify(notify) => {
            let scroll_end: NotifyCode = BuiltinNotify::ScrollEndStateRequest.into();
            if notify.kind.code() != scroll_end {
                return;
            }

            let source = notify.source;
            message.handled = true;
            let Some(state) = edit(cx, element) else {
                return;
            };
            let Some(arrow) = state.arrows.find(source) else {
                return;
            };
            state.arrows.get_mut(arrow).ready = true;
            update_arrow(cx, element, arrow);
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

            // Key down maps keyboard keys whatever the global setting.
            let done = match key_to_nav(&key, true) {
                Some(NavKey::Left) => move_left(cx, element),
                Some(NavKey::Right) => move_right(cx, element),
                Some(NavKey::Up) => scroll_vertical(cx, element, class, false),
                Some(NavKey::Down) => scroll_vertical(cx, element, class, true),
                _ if key.key == VirtualKey::Clear => {
                    set_text(cx, element, None);
                    true
                }
                _ => false,
            };
            if done {
                message.handled = true;
            }
        }
        Body::KeyUp(key) => {
            let key = *key;
            if key.character.is_some() {
                message.handled = true;
                return;
            }
            if edit(cx, element).is_none_or(|state| state.buffer.is_none()) {
                return;
            }

            let keyboard = !cx.runtime.keyboard_keys_disabled;
            let nav = key_to_nav(&key, keyboard);
            let Some(state) = edit(cx, element) else {
                return;
            };
            match nav {
                Some(NavKey::Up) => {
                    message.handled = true;
                    if state.scroll.velocity < 0.0 {
                        snap_target(&mut state.scroll);
                    }
                }
                Some(NavKey::Down) => {
                    message.handled = true;
                    if state.scroll.velocity > 0.0 {
                        snap_target(&mut state.scroll);
                    }
                }
                _ => {}
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
                    insert_char(cx, element, character);
                }
            }
        }
        Body::Timer { id } if *id == TimerId::CONTROL => {
            message.handled = true;
            smooth_scroll_step(cx, element, class);
        }
        Body::EditGetLineStart { line, .. } => {
            let line = *line;
            message.handled = true;
            let multiline = multiline(cx, element);
            let start = edit(cx, element).and_then(|state| {
                let scroll = &state.scroll;
                let start = scroll.line_starts.get(line as usize)?;
                let start = if multiline {
                    *start as u32
                } else {
                    scroll.first_char
                };
                Some(start)
            });
            if let Body::EditGetLineStart { start: answer, .. } = &mut message.body {
                *answer = start;
            }
        }
        Body::EditInsertText { index, text } => {
            let index = *index;
            message.handled = true;
            let Some(text) = text.clone() else {
                return;
            };
            // The original reads a terminated string.
            let text = text.split(|unit| *unit == 0).next().unwrap_or_default();
            insert_text(cx, element, index, text);
        }
        Body::EditDeleteText { index, count } => {
            let (index, count) = (*index, *count);
            message.handled = true;
            delete_text(cx, element, index, count);
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
        Body::GetNavTarget(_) => {
            if let Some(state) = edit(cx, element) {
                snap_target(&mut state.scroll);
            }
        }
        Body::EditGetCaret { .. } => {
            message.handled = true;
            let caret = edit(cx, element).map(|state| state.caret());
            if let Body::EditGetCaret { position } = &mut message.body {
                *position = caret;
            }
        }
        Body::EditSetCaret { position } => {
            let position = *position;
            message.handled = true;
            set_caret_position(cx, element, position);
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

/// Within 0.001 of `value`, compared as the original does in single
/// precision.
fn near(position: f32, value: f32) -> bool {
    const EPSILON: f32 = 0.001;

    position > value - EPSILON && position < value + EPSILON
}

fn more_if(more: bool) -> ScrollEndState {
    if more {
        ScrollEndState::More
    } else {
        ScrollEndState::Normal
    }
}

/// Character count of the measure wrapper: a negative count measures the
/// whole text.
fn char_count(count: u32) -> Option<usize> {
    i32::try_from(count).ok().map(|count| count as usize)
}

/// Key up of up and down (0x25) and message 0x7e6: the target moves to the
/// line boundary in the direction of the scroll, at most to the last page.
fn snap_target(scroll: &mut Scroll) {
    // The original traps on a zero pitch.
    let Some(line) = top_line(scroll.position, scroll.line_pitch) else {
        return;
    };

    if scroll.velocity < 0.0 {
        scroll.target = line.wrapping_mul(scroll.line_pitch) as f32;
    } else if scroll.velocity > 0.0 {
        let next = line.wrapping_add(1).wrapping_mul(scroll.line_pitch) as f32;
        scroll.target = if scroll.max_position < next {
            scroll.max_position
        } else {
            next
        };
    }
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

/// Rebuilds the display copy with the stored PasswordChar.
fn rebuild_display(cx: &mut Context<'_, '_>, element: ElementId) {
    let password = cx
        .tree
        .string(element, &[props::edit_password_char()])
        .map(|text| text.first().copied().unwrap_or(0));
    if let Some(state) = edit(cx, element) {
        state.build_display_text(password);
    }
}

/// The measure wrapper over the first `count` code units of the display
/// copy in a `width` by `height` rect. `None` without `Text` or font.
fn measure_display(
    cx: &mut Context<'_, '_>,
    element: ElementId,
    count: Option<usize>,
    width: f32,
    height: f32,
) -> Option<TextBounds> {
    let state = edit(cx, element)?;
    let text_element = state.text_element?;
    let display = state.display.clone();
    let measure = EditMeasure {
        text: display.as_deref().unwrap_or_default(),
        count,
        style: state.scroll.style,
        line_spacing_adjust: state.scroll.line_spacing_adjust,
        width,
        height,
    };

    let runtime = &mut *cx.runtime;
    edit_text_bounds(
        cx.tree,
        text_element,
        &measure,
        &runtime.fonts,
        &mut runtime.text,
    )
}

/// `XuiTextPresenterGetFont` on `Text` succeeds.
fn has_font(cx: &mut Context<'_, '_>, element: ElementId) -> bool {
    measure_display(cx, element, Some(0), 0.0, 0.0).is_some()
}

fn show_arrow(cx: &mut Context<'_, '_>, element: ElementId, arrow: Arrow, show: bool) {
    let part = edit(cx, element).and_then(|state| state.arrows.get_mut(arrow).element);
    if let Some(part) = part {
        cx.set_show(part, show);
    }
}

/// `XuiEdit_UpdateScrollArrow` 0x9215e970.
fn update_arrow(cx: &mut Context<'_, '_>, element: ElementId, arrow: Arrow) {
    let Some(state) = edit(cx, element) else {
        return;
    };
    let record = state.arrows.get_mut(arrow);
    let Some(part) = record.element else {
        return;
    };
    if record.current == record.requested {
        return;
    }

    match record.requested {
        ScrollEndState::Scrolling => {
            record.current = ScrollEndState::Scrolling;
            record.ready = false;
        }
        requested => {
            if !record.ready {
                return;
            }
            record.current = requested;
        }
    }

    let state = record.current;
    cx.send_body(part, Body::ScrollEndSetState { state });
}

fn request_arrow(
    cx: &mut Context<'_, '_>,
    element: ElementId,
    arrow: Arrow,
    requested: ScrollEndState,
) {
    if let Some(state) = edit(cx, element) {
        state.arrows.get_mut(arrow).requested = requested;
    }

    update_arrow(cx, element, arrow);
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
    state.caret_offset = 0;
    state.scroll.first_line = 0;
    state.scroll.first_char = 0;

    layout(cx, element);
    update_view(cx, element);

    true
}

/// `XuiEdit_Layout` 0x92162a18.
fn layout(cx: &mut Context<'_, '_>, element: ElementId) {
    let multiline = multiline(cx, element);
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
    let has_text = buffer.is_some();
    if metrics.is_some() && has_text {
        starts = wrapped;
    }

    let Some(state) = edit(cx, element) else {
        return;
    };
    state.buffer = buffer;
    let scroll = &mut state.scroll;
    scroll.line_starts = starts;
    match metrics {
        Some(metrics) => {
            scroll.style = metrics.style;
            scroll.line_spacing_adjust = metrics.line_spacing_adjust;
            scroll.line_pitch = metrics
                .line_height
                .wrapping_add_signed(metrics.line_spacing_adjust);
            scroll.text_width = metrics.box_width;
            scroll.text_height = metrics.box_height;
        }
        None => {
            scroll.style = TextStyle(0);
            scroll.line_spacing_adjust = 0;
            scroll.text_width = 0.0;
            scroll.text_height = 0.0;
        }
    }
    scroll.max_position = max_position(scroll);
    let (width, height) = (scroll.text_width, scroll.text_height);
    rebuild_display(cx, element);

    let text_height = if has_text && metrics.is_some() {
        measure_display(cx, element, None, width, height).map_or(0.0, |bounds| bounds.bottom)
    } else {
        0.0
    };
    if multiline {
        let show = text_height > height;
        show_arrow(cx, element, Arrow::Up, show);
        show_arrow(cx, element, Arrow::Down, show);
        show_arrow(cx, element, Arrow::Left, false);
        show_arrow(cx, element, Arrow::Right, false);
    }

    update_view(cx, element);

    let Some(state) = edit(cx, element) else {
        return;
    };
    let (position, max) = (state.scroll.position, state.scroll.max_position);
    if multiline && max > 0.0 {
        state.arrows.down.ready = true;
        request_arrow(cx, element, Arrow::Down, more_if(!near(position, max)));
        if let Some(state) = edit(cx, element) {
            state.arrows.up.ready = true;
        }
        request_arrow(cx, element, Arrow::Up, more_if(!near(position, 0.0)));
    }
}

/// Maximum scroll position: the lines past the whole lines of the first
/// page, never below 0.
fn max_position(scroll: &Scroll) -> f32 {
    if scroll.line_pitch == 0 {
        return 0.0;
    }

    let pitch = scroll.line_pitch as f32;
    let page = libm::floorf(scroll.text_height / pitch);
    let lines = scroll.line_starts.len() as f32;

    // `fmsubs`: one rounding for lines * pitch - pitch * page.
    let value = libm::fmaf(lines, pitch, -(pitch * page));
    if value >= 0.0 { value } else { 0.0 }
}

/// `XuiEdit_UpdateView` 0x921626f0, without the highlight run copy, for
/// which the binary has no writer.
fn update_view(cx: &mut Context<'_, '_>, element: ElementId) {
    let read_only = read_only(cx, element);
    let multiline = multiline(cx, element);
    let Some(state) = edit(cx, element) else {
        return;
    };
    let caret = state.caret_element;
    let caret_offset = state.caret_offset;
    if state.text_element.is_none() || !has_font(cx, element) {
        return;
    }

    // The caret moves to the end of the text before it; the position of
    // `Text` is not added.
    if let Some(caret) = caret
        && !read_only
        && cx.is_enabled(element)
        && let Some(bounds) = measure_display(cx, element, char_count(caret_offset), 0.0, 0.0)
        && let Some(position) = cx.tree.get(caret).map(|caret| caret.position())
    {
        let position = Vector3::new(bounds.right, position.y, position.z);
        let _ = cx.tree.set_property(
            caret,
            &[props::position()],
            None,
            PropValue::Vector(position),
        );
    }

    let Some(state) = edit(cx, element) else {
        return;
    };
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
        && start as u32 != scroll.first_char
    {
        scroll.first_char = start as u32;
        rebuild_display(cx, element);
    }

    if multiline {
        return;
    }

    let Some(state) = edit(cx, element) else {
        return;
    };
    let (width, height) = (state.scroll.text_width, state.scroll.text_height);
    let first_char = state.scroll.first_char;
    let overflow = measure_display(cx, element, None, width, height)
        .is_some_and(|bounds| bounds.right > width);
    let visible = overflow || first_char > 0;
    show_arrow(cx, element, Arrow::Left, visible);
    request_arrow(cx, element, Arrow::Left, more_if(first_char != 0));
    show_arrow(cx, element, Arrow::Right, visible);
    request_arrow(cx, element, Arrow::Right, more_if(overflow));
    show_arrow(cx, element, Arrow::Up, false);
    show_arrow(cx, element, Arrow::Down, false);
}

/// `XuiEdit_ShiftViewLeft` 0x921683f8 and `XuiEdit_ShiftViewRight`
/// 0x92168498: the view moves by 5 characters. Every caller passes
/// animate 1, which shows the arrow of the direction scrolling.
fn shift_view(cx: &mut Context<'_, '_>, element: ElementId, right: bool) {
    const STEP: u32 = 5;

    if !cx.is_enabled(element) {
        return;
    }

    let read_only = read_only(cx, element);
    let Some(state) = edit(cx, element) else {
        return;
    };
    let (first_char, caret_offset) = if right {
        (
            state.scroll.first_char.wrapping_add(STEP),
            state.caret_offset.wrapping_sub(STEP),
        )
    } else {
        (
            state.scroll.first_char.wrapping_sub(STEP),
            state.caret_offset.wrapping_add(STEP),
        )
    };
    state.scroll.first_char = first_char;
    state.caret_offset = if read_only { 0 } else { caret_offset };
    rebuild_display(cx, element);
    update_view(cx, element);

    let arrow = if right { Arrow::Right } else { Arrow::Left };
    request_arrow(cx, element, arrow, ScrollEndState::Scrolling);
}

/// `XuiEdit_MoveLeft` 0x92168a00. False where the original fails.
fn move_left(cx: &mut Context<'_, '_>, element: ElementId) -> bool {
    if edit(cx, element).is_none_or(|state| state.buffer.is_none()) || !cx.is_enabled(element) {
        return false;
    }

    let read_only = read_only(cx, element);
    let multiline = multiline(cx, element);
    let Some(state) = edit(cx, element) else {
        return false;
    };
    let (caret_offset, first_char) = (state.caret_offset, state.scroll.first_char);
    if caret_offset == 0 && first_char == 0 {
        return false;
    }

    if read_only {
        // A read-only single-line edit moves its view only.
        if multiline || first_char == 0 {
            return false;
        }
        shift_view(cx, element, false);
    } else {
        if caret_offset == 1 && first_char > 0 {
            shift_view(cx, element, false);
        }
        if let Some(state) = edit(cx, element) {
            state.caret_offset = state.caret_offset.wrapping_sub(1);
        }
    }

    update_view(cx, element);

    true
}

/// `XuiEdit_MoveRight` 0x92168ae0. False where the original fails.
fn move_right(cx: &mut Context<'_, '_>, element: ElementId) -> bool {
    if edit(cx, element).is_none_or(|state| state.buffer.is_none()) || !cx.is_enabled(element) {
        return false;
    }

    let read_only = read_only(cx, element);
    let multiline = multiline(cx, element);
    let Some(state) = edit(cx, element) else {
        return false;
    };
    if state.caret() >= state.text_len() {
        return false;
    }

    let shown = state
        .display
        .as_ref()
        .map_or(0, |display| display.len() as u32);
    let (width, height) = (state.scroll.text_width, state.scroll.text_height);
    if !has_font(cx, element) {
        return false;
    }

    if read_only {
        // A read-only single-line edit moves its view while text is cut off.
        if multiline {
            return false;
        }
        let overflow = measure_display(cx, element, char_count(shown), width, height)
            .is_some_and(|bounds| bounds.right > width);
        if !overflow {
            return false;
        }
        shift_view(cx, element, true);
    } else {
        let Some(state) = edit(cx, element) else {
            return false;
        };
        state.caret_offset = state.caret_offset.wrapping_add(1);

        // The character after the caret must fit as well.
        let count = shown.min(state.caret_offset.wrapping_add(1));
        let overflow = measure_display(cx, element, char_count(count), width, height)
            .is_some_and(|bounds| bounds.right > width);
        if overflow {
            shift_view(cx, element, true);
        }
    }

    update_view(cx, element);

    true
}

/// `XuiEdit_SetCaretPosition` 0x92169518: caret moves one character at a
/// time. False where the original fails.
fn set_caret_position(cx: &mut Context<'_, '_>, element: ElementId, position: u32) -> bool {
    if read_only(cx, element) || !cx.is_enabled(element) {
        return false;
    }

    let Some(state) = edit(cx, element) else {
        return false;
    };
    if state.buffer.is_none() || state.text_element.is_none() || position > state.text_len() {
        return false;
    }

    loop {
        let Some(caret) = edit(cx, element).map(|state| state.caret()) else {
            return false;
        };
        let moved = if position < caret {
            move_left(cx, element)
        } else if position > caret {
            move_right(cx, element)
        } else {
            return true;
        };
        if !moved {
            return false;
        }
    }
}

/// `XuiEdit_InsertText` 0x92169fa0. The original does not bound `index`;
/// an index past the end moves memory past the terminator. This port
/// ignores such a request.
fn insert_text(cx: &mut Context<'_, '_>, element: ElementId, index: u32, text: &[u16]) {
    if read_only(cx, element) || !cx.is_enabled(element) {
        return;
    }

    let Some(limit) = text_limit(cx, element) else {
        return;
    };
    let Some(state) = edit(cx, element) else {
        return;
    };
    let length = state.text_len() as usize;
    if length + text.len() > limit || index as usize > length {
        return;
    }

    state
        .buffer
        .get_or_insert_with(Vec::new)
        .splice(index as usize..index as usize, text.iter().copied());
    rebuild_display(cx, element);

    let Some(caret) = edit(cx, element).map(|state| state.caret()) else {
        return;
    };
    if index <= caret {
        set_caret_position(cx, element, caret.wrapping_add(text.len() as u32));
    }

    update_view(cx, element);
}

/// `XuiEdit_DeleteText` 0x9216a100. A caret after `index` moves back by
/// `count`; when that passes below 0 the move fails and the caret stays.
/// A range whose end wraps past 2^32, which moves memory past the
/// terminator in the original, is ignored.
fn delete_text(cx: &mut Context<'_, '_>, element: ElementId, index: u32, count: u32) {
    if read_only(cx, element) || !cx.is_enabled(element) {
        return;
    }

    let Some(state) = edit(cx, element) else {
        return;
    };
    let end = index.wrapping_add(count);
    let Some(buffer) = &mut state.buffer else {
        return;
    };
    if end > buffer.len() as u32 || end < index {
        return;
    }

    buffer.drain(index as usize..end as usize);
    rebuild_display(cx, element);

    let Some(caret) = edit(cx, element).map(|state| state.caret()) else {
        return;
    };
    if caret > index {
        set_caret_position(cx, element, caret.wrapping_sub(count));
    }

    update_view(cx, element);
}

/// `XuiEdit_Backspace` 0x9216a1d0.
fn backspace(cx: &mut Context<'_, '_>, element: ElementId) {
    let Some(caret) = edit(cx, element).map(|state| state.caret()) else {
        return;
    };
    if caret != 0 && cx.is_enabled(element) {
        delete_text(cx, element, caret - 1, 1);
    }
}

/// `XuiEdit_InsertChar` 0x9216a240.
fn insert_char(cx: &mut Context<'_, '_>, element: ElementId, character: u16) {
    if !cx.is_enabled(element) {
        return;
    }

    let Some(caret) = edit(cx, element).map(|state| state.caret()) else {
        return;
    };
    insert_text(cx, element, caret, &[character]);
}

/// `XuiEdit_ScrollUp` 0x92168638 and `XuiEdit_ScrollDown` 0x92168820 with
/// animate 1, the only value their caller (key down) passes. False where
/// the original fails, which leaves the key to navigation.
fn scroll_vertical(
    cx: &mut Context<'_, '_>,
    element: ElementId,
    class: ClassId,
    down: bool,
) -> bool {
    let ready = read_only(cx, element) && multiline(cx, element);
    if edit(cx, element).is_none_or(|state| state.buffer.is_none())
        || !ready
        || !cx.is_enabled(element)
    {
        return false;
    }

    let smooth = flag(cx, element, props::edit_smooth_scroll());
    let Some(state) = edit(cx, element) else {
        return false;
    };
    let scroll = &mut state.scroll;
    let end = if down { scroll.max_position } else { 0.0 };
    if near(scroll.position, end) {
        return false;
    }

    let pitch = scroll.line_pitch as f32;
    if smooth {
        // The speed starts over only when the scroll does not go this way.
        scroll.target = end;
        let start = pitch * scroll.start_speed.0;
        if down && scroll.velocity.partial_cmp(&0.0) != Some(Ordering::Greater) {
            scroll.velocity = start;
        } else if !down && scroll.velocity.partial_cmp(&0.0) != Some(Ordering::Less) {
            scroll.velocity = -start;
        }
    } else if down {
        let next = scroll.target + pitch;
        scroll.target = if scroll.max_position < next {
            scroll.max_position
        } else {
            next
        };
        scroll.velocity = pitch;
    } else {
        let previous = scroll.target - pitch;
        scroll.target = if previous >= 0.0 { previous } else { 0.0 };
        scroll.velocity = -pitch;
    }
    start_scroll_timer(cx, element, class);

    let (own, other) = if down {
        (Arrow::Down, Arrow::Up)
    } else {
        (Arrow::Up, Arrow::Down)
    };
    request_arrow(cx, element, own, ScrollEndState::Scrolling);
    request_arrow(cx, element, other, ScrollEndState::More);
    update_view(cx, element);

    true
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
        let elapsed = elapsed as f32;
        let step_ms = if elapsed > MAX_STEP_MS {
            MAX_STEP_MS
        } else {
            elapsed
        };
        scroll.last_step = now;
        // `fmadds`: one rounding for the step and the sum.
        scroll.position = libm::fmaf(step_ms * SECONDS_PER_MS, velocity, scroll.position);
    } else {
        scroll.position += velocity;
    }

    let arrived = if velocity > 0.0 {
        Some((
            scroll.position.partial_cmp(&scroll.target) != Some(Ordering::Less),
            Arrow::Down,
        ))
    } else if velocity < 0.0 {
        Some((
            scroll.position.partial_cmp(&scroll.target) != Some(Ordering::Greater),
            Arrow::Up,
        ))
    } else {
        None
    };
    if let Some((true, arrow)) = arrived {
        scroll.position = scroll.target;
        scroll.velocity = 0.0;
        scroll.timer_active = false;
        let end = if arrow == Arrow::Down {
            scroll.max_position
        } else {
            0.0
        };
        let requested = more_if(!near(scroll.position, end));

        let handle = cx.level_handle(element, class);
        cx.runtime.timers.kill_for(handle, TimerId::CONTROL);
        request_arrow(cx, element, arrow, requested);
    }
    if arrived.is_some() {
        update_view(cx, element);
    }

    let Some(state) = edit(cx, element) else {
        return;
    };
    let scroll = &mut state.scroll;
    if smooth && scroll.velocity != 0.0 {
        let limit = scroll.line_pitch as f32 * scroll.max_speed.0;
        let grown = (scroll.acceleration + 1.0) * scroll.velocity;
        scroll.velocity = if scroll.velocity > 0.0 {
            if limit >= grown { grown } else { limit }
        } else if -limit <= grown {
            grown
        } else {
            -limit
        };
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

/// Loaded properties of an XuiEdit whose setters do more than store the
/// value, in serialized order (class levels root first, then definition
/// order): Text, TextLimit, AllowedChars, PasswordChar. Empty for other
/// classes. `XuiApplyLoadedProperty` 0x9214f7f8 sets every loaded entry
/// through `XuiObjectSetPropertyById`, which calls the definition's
/// setter (+0x24) when it has one.
pub(crate) fn loaded_setters(
    registry: &Registry,
    tree: &ElementTree,
    element: ElementId,
    object: ObjectRef<'_, '_>,
) -> Vec<&'static PropDef> {
    let mut level = tree.class(element).and_then(|class| registry.class(class));
    let is_edit = loop {
        let Some(current) = level else {
            break false;
        };
        if ptr::eq(current.def(), &xui::XUI_EDIT) {
            break true;
        }
        level = current.base();
    };
    if !is_edit {
        return Vec::new();
    }

    let replayed = [
        props::text(),
        props::edit_text_limit(),
        props::edit_allowed_chars(),
        props::edit_password_char(),
    ];
    object
        .properties()
        .iter()
        .map(|property| property.def())
        .filter(|def| replayed.iter().any(|candidate| ptr::eq(*candidate, *def)))
        .collect()
}

impl Context<'_, '_> {
    /// Runs the setters of `loaded` (from [`loaded_setters`]) in order on
    /// the values the loader stored, starting from the constructor values.
    /// The original runs them at creation, before the children exist; no
    /// XuiEdit setter reads a child then.
    pub(crate) fn edit_apply_loaded(&mut self, element: ElementId, loaded: &[&'static PropDef]) {
        let values: Vec<(&'static PropDef, PropValue)> = loaded
            .iter()
            .filter_map(|def| Some((*def, self.tree.property(element, &[def], None)?)))
            .collect();
        for (def, _) in &values {
            if let Some(default) = def.default {
                // The loaded value had the same type; the store cannot fail.
                let _ =
                    self.tree
                        .set_property(element, &[def], None, PropValue::from_default(default));
            }
        }

        for (def, value) in values {
            // Each definition belongs to the element's class levels and the
            // value has its type.
            let _ = self.set_property(element, &[def], None, value);
        }
    }

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

    /// PasswordChar setter 0x92168340: only the first character is kept.
    /// The display copy changes at its next rebuild.
    pub(crate) fn edit_set_password_char(&mut self, element: ElementId, text: Option<&[u16]>) {
        let stored: Option<Box<[u16]>> =
            text.map(|text| Box::new([text.first().copied().unwrap_or(0)]) as Box<[u16]>);

        // A String definition; the type check cannot fail.
        let _ = self.tree.set_property(
            element,
            &[props::edit_password_char()],
            None,
            PropValue::String(stored),
        );

        update_view(self, element);
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
        let limit = limit as u32;
        if limit < state.caret() {
            state.caret_offset = limit.wrapping_sub(state.scroll.first_char);
        }

        update_view(self, element);
    }
}

#[cfg(test)]
mod tests;

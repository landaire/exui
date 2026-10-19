//! XuiSlider (0x92162358) and XuiProgressBar (0x921620a0).

use alloc::format;
use alloc::rc::Rc;
use alloc::vec::Vec;

use exui_core::schema::ClassId;
use exui_core::schema::PropDef;

use crate::control::ClassState;
use crate::control::props;
use crate::message::Body;
use crate::message::Context;
use crate::message::Message;
use crate::message::NavKey;
use crate::message::NotifyKind;
use crate::message::ScrollEndState;
use crate::message::TimerId;
use crate::message::key_to_nav;
use crate::scene::ElementFlags;
use crate::scene::ElementId;
use crate::scene::PropValue;
use crate::time::Milliseconds;
use crate::timeline::Frame;
use crate::timeline::PlayEnd;
use crate::timeline::PlayRange;

/// DataAssociation that sliders and progress bars answer with their value
/// text.
const VALUE_TEXT_ASSOCIATION: u32 = 1;

/// XuiSlider parts and acceleration state.
#[derive(Clone, Debug, Default)]
pub struct SliderState {
    pub body: Option<ElementId>,
    pub scroll_min: Option<ElementId>,
    pub scroll_max: Option<ElementId>,
    pub accel_amount: i32,
    pub accel_sign: i32,
    pub accelerating: bool,
    /// Decimal value text answered for DataAssociation 1.
    pub text: Option<Rc<[u16]>>,
}

impl SliderState {
    pub fn new() -> Self {
        Self::default()
    }
}

/// XuiProgressBar body part and value text.
#[derive(Clone, Debug, Default)]
pub struct ProgressState {
    pub body: Option<ElementId>,
    pub text: Option<Rc<[u16]>>,
}

impl ProgressState {
    pub fn new() -> Self {
        Self::default()
    }
}

fn integer(cx: &Context<'_, '_>, element: ElementId, def: &'static PropDef) -> i32 {
    match cx.tree.property(element, &[def], None) {
        Some(PropValue::Integer(value)) => value,
        Some(PropValue::Unsigned(value)) => value as i32,
        _ => 0,
    }
}

fn decimal(value: i32) -> Rc<[u16]> {
    format!("{value}")
        .encode_utf16()
        .collect::<Vec<u16>>()
        .into()
}

fn slider<'c>(cx: &'c mut Context<'_, '_>, element: ElementId) -> Option<&'c mut SliderState> {
    match cx.class_state(element)? {
        ClassState::Slider(state) => Some(state),
        _ => None,
    }
}

fn progress<'c>(cx: &'c mut Context<'_, '_>, element: ElementId) -> Option<&'c mut ProgressState> {
    match cx.class_state(element)? {
        ClassState::Progress(state) => Some(state),
        _ => None,
    }
}

/// Sets flag 0x20 on a subtree (`XuiElementSetFlag20Recursive`).
fn keep_design_layout(cx: &mut Context<'_, '_>, root: ElementId) {
    let mut stack = Vec::new();
    stack.push(root);
    while let Some(node) = stack.pop() {
        if let Some(mut flags) = cx.tree.flags(node) {
            flags.set(ElementFlags::KEEP_DESIGN_LAYOUT, true);
            let _ = cx.tree.set_flags(node, flags);
        }
        stack.extend(cx.tree.children(node));
    }
}

fn visual_part(cx: &Context<'_, '_>, element: ElementId, id: &str) -> Option<ElementId> {
    let visual = cx.control_visual(element)?;
    let id: Vec<u16> = id.encode_utf16().collect();
    cx.tree.child_by_id(visual, &id)
}

pub(super) fn on_slider(
    cx: &mut Context<'_, '_>,
    element: ElementId,
    class: ClassId,
    message: &mut Message,
) {
    match &message.body {
        Body::CreateVisual => {
            if let Some(state) = slider(cx, element) {
                state.body = None;
                state.scroll_min = None;
                state.scroll_max = None;
            }
            let mut forwarded = Message::new(Body::CreateVisual);
            cx.send_to_base(element, class, &mut forwarded);
            message.handled = true;

            let body = visual_part(cx, element, "SliderBody");
            let scroll_min = visual_part(cx, element, "ScrollMin");
            let scroll_max = visual_part(cx, element, "ScrollMax");
            if let Some(state) = slider(cx, element) {
                state.body = body;
                state.scroll_min = scroll_min;
                state.scroll_max = scroll_max;
            }
            if let Some(body) = body {
                keep_design_layout(cx, body);
            }
            slider_update_visual(cx, element, true, false, false);
            slider_update_scroll_ends(cx, element);
        }
        Body::InitComplete => {
            let value = integer(cx, element, props::slider_value());
            cx.slider_set_value(element, value);
        }
        Body::KillFocus { .. } => {
            if slider(cx, element).is_some_and(|state| state.accelerating) {
                stop_accel(cx, element);
            }
        }
        Body::Notify(notify) => {
            if matches!(notify.kind, NotifyKind::ScrollEndStateRequest) {
                let source = notify.source;
                let parts = slider(cx, element).map(|state| (state.scroll_min, state.scroll_max));
                if let Some((min, max)) = parts
                    && (Some(source) == min || Some(source) == max)
                {
                    message.handled = true;
                    slider_update_scroll_ends(cx, element);
                }
            }
        }
        Body::KeyDown(key) => {
            let key = *key;
            if !cx.is_enabled(element) {
                return;
            }
            let keyboard = !cx.runtime.keyboard_keys_disabled;
            let nav = key_to_nav(&key, keyboard);
            let vertical = matches!(
                cx.tree.property(element, &[props::slider_vertical()], None),
                Some(PropValue::Bool(true))
            );
            let (decrement, increment) = if vertical {
                (nav == Some(NavKey::Down), nav == Some(NavKey::Up))
            } else {
                (nav == Some(NavKey::Left), nav == Some(NavKey::Right))
            };

            let step = integer(cx, element, props::slider_step());
            let value = integer(cx, element, props::slider_value());
            let min = integer(cx, element, props::slider_min());
            let max = integer(cx, element, props::slider_max());

            if decrement && ((step > 0 && value != min) || (step < 0 && value != max)) {
                if !start_accel(cx, element, -1) {
                    cx.slider_set_value(element, value.wrapping_sub(step));
                }
                message.handled = true;
            } else if increment && ((step > 0 && value != max) || (step < 0 && value != min)) {
                if !start_accel(cx, element, 1) {
                    cx.slider_set_value(element, value.wrapping_add(step));
                }
                message.handled = true;
            }
        }
        Body::KeyUp(key) => {
            let key = *key;
            if !slider(cx, element).is_some_and(|state| state.accelerating) {
                return;
            }
            let keyboard = !cx.runtime.keyboard_keys_disabled;
            let vertical = matches!(
                cx.tree.property(element, &[props::slider_vertical()], None),
                Some(PropValue::Bool(true))
            );
            let on_axis = match key_to_nav(&key, keyboard) {
                Some(NavKey::Left | NavKey::Right) => !vertical,
                Some(NavKey::Up | NavKey::Down) => vertical,
                _ => false,
            };
            if on_axis {
                message.handled = true;
                stop_accel(cx, element);
            }
        }
        Body::Timer { id } if *id == TimerId::CONTROL => {
            message.handled = true;
            let inc = integer(cx, element, props::slider_accel_inc());
            let Some(state) = slider(cx, element) else {
                return;
            };
            state.accel_amount = state.accel_amount.wrapping_add(inc);
            let delta = state.accel_sign.wrapping_mul(state.accel_amount);
            let value = integer(cx, element, props::slider_value());
            cx.slider_set_value(element, value.wrapping_add(delta));
        }
        Body::UpdateState { explicit, focused } => {
            let (explicit, focused) = (*explicit, *focused);
            let mut forwarded = Message::new(message.body.clone());
            cx.send_to_base(element, class, &mut forwarded);
            message.handled = true;
            slider_update_visual(cx, element, true, explicit, focused);
            slider_update_scroll_ends(cx, element);
        }
        Body::GetSourceText(query)
            if query.item == 0 && query.association == VALUE_TEXT_ASSOCIATION =>
        {
            let text = slider(cx, element).and_then(|state| state.text.clone());
            if let Body::GetSourceText(query) = &mut message.body {
                query.text = text;
            }
            message.handled = true;
        }
        _ => {}
    }
}

/// `StartAccel` 0x92156fa8. Returns true when the key is consumed without
/// a step.
fn start_accel(cx: &mut Context<'_, '_>, element: ElementId, sign: i32) -> bool {
    let period = match cx
        .tree
        .property(element, &[props::slider_accel_time()], None)
    {
        Some(PropValue::Unsigned(time)) if time != 0 => Milliseconds(time),
        _ => return false,
    };

    let step = integer(cx, element, props::slider_step());
    let Some(state) = slider(cx, element) else {
        return false;
    };
    if state.accelerating {
        return true;
    }

    state.accel_sign = sign;
    state.accel_amount = step;
    state.accelerating = true;
    cx.runtime.timers.set(element, TimerId::CONTROL, period);

    false
}

/// `StopAccel` 0x92157018.
fn stop_accel(cx: &mut Context<'_, '_>, element: ElementId) {
    let time = match cx
        .tree
        .property(element, &[props::slider_accel_time()], None)
    {
        Some(PropValue::Unsigned(time)) => time,
        _ => 0,
    };
    if time != 0 {
        cx.runtime.timers.kill(element, TimerId::CONTROL);
    }
    if let Some(state) = slider(cx, element) {
        state.accelerating = false;
    }
}

fn slider_update_scroll_ends(cx: &mut Context<'_, '_>, element: ElementId) {
    let value = integer(cx, element, props::slider_value());
    let min = integer(cx, element, props::slider_min());
    let max = integer(cx, element, props::slider_max());
    let Some((scroll_min, scroll_max)) =
        slider(cx, element).map(|state| (state.scroll_min, state.scroll_max))
    else {
        return;
    };

    let state = |more: bool| {
        if more {
            ScrollEndState::More
        } else {
            ScrollEndState::Normal
        }
    };
    if let Some(scroll_min) = scroll_min {
        cx.send_body(
            scroll_min,
            Body::ScrollEndSetState {
                state: state(value > min),
            },
        );
    }
    if let Some(scroll_max) = scroll_max {
        cx.send_body(
            scroll_max,
            Body::ScrollEndSetState {
                state: state(value < max),
            },
        );
    }
}

/// `XuiSlider` UpdateVisual 0x9215e770: the value seeks the body's state
/// segment.
fn slider_update_visual(
    cx: &mut Context<'_, '_>,
    element: ElementId,
    play_thumb: bool,
    explicit: bool,
    focused: bool,
) {
    let Some(body) = slider(cx, element).and_then(|state| state.body) else {
        return;
    };

    let focused = if explicit {
        focused
    } else {
        cx.has_focus(element)
    };
    let enabled = cx.is_enabled(element);
    let value = integer(cx, element, props::slider_value());
    let min = integer(cx, element, props::slider_min());
    let max = integer(cx, element, props::slider_max());

    if cx.runtime.timelines.get(body).is_some()
        && let Some((start, end)) = cx.state_frame_range(body, enabled, focused)
    {
        let (start, end) = (start as i32, end as i32);
        let mut frame = end;
        if max > min {
            frame = value
                .wrapping_sub(min)
                .wrapping_mul(end.wrapping_sub(start))
                .wrapping_div(max.wrapping_sub(min))
                .wrapping_add(start);
        }
        let frame = Frame(frame);
        cx.play_timeline(
            body,
            PlayRange {
                start: frame,
                initial: frame,
                end: PlayEnd::from_frame(frame),
                looping: true,
            },
            false,
        );
    }

    if !play_thumb {
        return;
    }

    let thumb_id: Vec<u16> = "SliderThumb".encode_utf16().collect();
    let Some(thumb) = cx.tree.child_by_id(body, &thumb_id) else {
        return;
    };
    if let Some((start, end)) = cx.state_frame_range(thumb, enabled, focused) {
        let start = Frame(start as i32);
        cx.play_timeline(
            thumb,
            PlayRange {
                start,
                initial: start,
                end: PlayEnd::from_frame(Frame(end as i32)),
                looping: true,
            },
            true,
        );
    }
}

impl Context<'_, '_> {
    /// `XuiSlider` SetValue 0x921621c0.
    pub fn slider_set_value(&mut self, element: ElementId, value: i32) {
        let min = integer(self, element, props::slider_min());
        let max = integer(self, element, props::slider_max());
        // Max wins when Min > Max.
        let mut value = value;
        if value < min {
            value = min;
        }
        if value > max {
            value = max;
        }

        let old = integer(self, element, props::slider_value());
        let (scroll_min, scroll_max) = slider(self, element)
            .map_or((None, None), |state| (state.scroll_min, state.scroll_max));
        if value < old
            && let Some(scroll_min) = scroll_min
        {
            if old == max
                && let Some(scroll_max) = scroll_max
            {
                self.send_body(
                    scroll_max,
                    Body::ScrollEndSetState {
                        state: ScrollEndState::Normal,
                    },
                );
            }
            self.send_body(
                scroll_min,
                Body::ScrollEndSetState {
                    state: ScrollEndState::Scrolling,
                },
            );
        }
        if value > old
            && let Some(scroll_max) = scroll_max
        {
            if old == min
                && let Some(scroll_min) = scroll_min
            {
                self.send_body(
                    scroll_min,
                    Body::ScrollEndSetState {
                        state: ScrollEndState::Normal,
                    },
                );
            }
            self.send_body(
                scroll_max,
                Body::ScrollEndSetState {
                    state: ScrollEndState::Scrolling,
                },
            );
        }

        if value != old {
            // A handler may replace the new value; it is not clamped again.
            let message = self.bubble_notify(
                Some(element),
                element,
                NotifyKind::ValueChanging { old, new: value },
            );
            if let Body::Notify(notify) = message.body
                && let NotifyKind::ValueChanging { new, .. } = notify.kind
            {
                value = new;
            }
        }

        let changed = value != integer(self, element, props::slider_value());
        let _ = self.tree.set_property(
            element,
            &[props::slider_value()],
            None,
            PropValue::Integer(value),
        );
        if let Some(state) = slider(self, element) {
            state.text = Some(decimal(value));
        }
        slider_update_visual(self, element, false, false, false);

        if changed {
            self.bubble_notify(Some(element), element, NotifyKind::ValueChanged { value });
        }
    }

    /// `XuiProgressBar` SetValue 0x92162038. No notifications.
    pub fn progress_set_value(&mut self, element: ElementId, value: i32) {
        let min = integer(self, element, props::progress_min());
        let max = integer(self, element, props::progress_max());
        let value = value.max(min).min(max);

        let _ = self.tree.set_property(
            element,
            &[props::progress_value()],
            None,
            PropValue::Integer(value),
        );
        if let Some(state) = progress(self, element) {
            state.text = Some(decimal(value));
        }
        progress_update_visual(self, element);
    }
}

/// `XuiProgressBar` UpdateVisual 0x9215e5f8: the value seeks the body
/// timeline between frame 0 and its length.
fn progress_update_visual(cx: &mut Context<'_, '_>, element: ElementId) {
    let Some(body) = progress(cx, element).and_then(|state| state.body) else {
        return;
    };
    let Some(length) = cx
        .runtime
        .timelines
        .get(body)
        .map(|timeline| timeline.length() as i32)
    else {
        return;
    };

    let value = integer(cx, element, props::progress_value());
    let min = integer(cx, element, props::progress_min());
    let max = integer(cx, element, props::progress_max());
    let mut frame = length;
    if max > min {
        frame = value
            .wrapping_sub(min)
            .wrapping_mul(length)
            .wrapping_div(max.wrapping_sub(min));
    }

    let frame = Frame(frame);
    cx.play_timeline(
        body,
        PlayRange {
            start: frame,
            initial: frame,
            end: PlayEnd::from_frame(frame),
            looping: true,
        },
        true,
    );
}

pub(super) fn on_progress(
    cx: &mut Context<'_, '_>,
    element: ElementId,
    class: ClassId,
    message: &mut Message,
) {
    match &mut message.body {
        Body::CreateVisual => {
            if let Some(state) = progress(cx, element) {
                state.body = None;
            }
            let mut forwarded = Message::new(Body::CreateVisual);
            cx.send_to_base(element, class, &mut forwarded);
            message.handled = true;

            let body = visual_part(cx, element, "ProgressBody");
            if let Some(state) = progress(cx, element) {
                state.body = body;
            }
            if let Some(body) = body {
                keep_design_layout(cx, body);
            }
            progress_update_visual(cx, element);
        }
        Body::QueryFocusable { focusable } => {
            *focusable = false;
            message.handled = true;
        }
        Body::InitComplete => {
            let value = integer(cx, element, props::progress_value());
            cx.progress_set_value(element, value);
        }
        Body::UpdateState { .. } => {
            message.handled = true;
            progress_update_visual(cx, element);
        }
        Body::GetSourceText(query)
            if query.item == 0 && query.association == VALUE_TEXT_ASSOCIATION =>
        {
            let text = progress(cx, element).and_then(|state| state.text.clone());
            if let Body::GetSourceText(query) = &mut message.body {
                query.text = text;
            }
            message.handled = true;
        }
        _ => {}
    }
}

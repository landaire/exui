//! XuiControl level (0x9215aa68): focus, navigation, press cycle, visual
//! creation and text and image binding.

use alloc::rc::Rc;
use alloc::vec::Vec;

use exui_core::schema::ClassId;

use crate::control::props;
use crate::message::Body;
use crate::message::Context;
use crate::message::FocusUser;
use crate::message::KeyData;
use crate::message::KeyFlags;
use crate::message::Message;
use crate::message::NavDirection;
use crate::message::NavKey;
use crate::message::NavQuery;
use crate::message::NotifyKind;
use crate::message::ResourceMask;
use crate::message::TabStopKind;
use crate::message::key_to_nav;
use crate::scene::ElementId;
use crate::scene::PropValue;
use crate::timeline::PlayEnd;
use crate::timeline::PlayRange;

/// XuiControl instance fields that are not properties.
#[derive(Clone, Debug)]
pub struct ControlState {
    /// Flag 0x4.
    pub visual_created: bool,
    /// Flag 0x8.
    pub pressed: bool,
    /// Child or item data index (+0xc).
    pub index: Option<u32>,
    /// User of the press in progress (+0x38).
    pub press_user: FocusUser,
    /// Text version counter (+0x3c); never 0 after the first change.
    pub text_version: u32,
    /// Copy of Text handed out by 0x7df and 0x7fb, kept while Text is
    /// unchanged. The original hands out its field pointer; presenters ask
    /// every frame.
    text_share: Option<Rc<[u16]>>,
}

impl ControlState {
    pub fn new() -> Self {
        Self {
            visual_created: false,
            pressed: false,
            index: None,
            press_user: FocusUser::Shared,
            text_version: 0,
            text_share: None,
        }
    }
}

impl Default for ControlState {
    fn default() -> Self {
        Self::new()
    }
}

pub(super) fn on_message(
    cx: &mut Context<'_, '_>,
    element: ElementId,
    class: ClassId,
    message: &mut Message,
) {
    match &mut message.body {
        Body::Render | Body::CreateResources => {
            if !cx
                .control_state(element)
                .is_some_and(|state| state.visual_created)
            {
                cx.send_body(element, Body::CreateVisual);
            }
        }
        Body::CreateVisual => {
            message.handled = true;
            if cx.runtime.load_depth == 0 && cx.create_control_visual(element).is_ok() {
                cx.update_state(element);
            }
        }
        Body::Destroy => {
            cx.destroy_control_visual(element);
        }
        Body::ReleaseResources { mask } => {
            if mask.contains(ResourceMask::CONTROL_VISUALS) {
                cx.destroy_control_visual(element);
            }
        }
        Body::QueryFocusable { focusable } => {
            message.handled = true;
            let shown = cx.is_shown(element);
            *focusable = shown && !cx.is_focus_disabled(element, true);
        }
        Body::SetFocus { previous, .. } => {
            let previous = *previous;
            message.handled = true;
            cx.update_state_focus(element, true);
            cx.notify_parent(element, NotifyKind::SetFocus { previous });
        }
        Body::KillFocus { new, .. } => {
            let new = *new;
            message.handled = true;
            cx.update_state_focus(element, false);
            cx.notify_parent(element, NotifyKind::KillFocus { new });
        }
        Body::TimelineEnd { .. } => on_timeline_end(cx, element),
        Body::SetShow { .. } => {
            cx.send_to_base(element, class, message);
            message.handled = true;

            // 0x9214cac8 tests the slots for the control itself only, so
            // hiding an ancestor of the focused element keeps focus.
            if !cx.is_shown(element)
                && let Some(user) = cx.focus_user(element)
            {
                let parent = cx.tree.parent(element);
                let _ = cx.set_focus(parent, user);
            }
        }
        Body::KeyDown(key) => {
            let key = *key;
            on_key_down(cx, element, &key, message);
        }
        Body::GetEnabled { enabled } => {
            *enabled = matches!(
                cx.tree.property(element, &[props::enabled()], None),
                Some(PropValue::Bool(true))
            );
            message.handled = true;
        }
        Body::SetEnabled { enabled } => {
            let enabled = *enabled;
            let _ =
                cx.tree
                    .set_property(element, &[props::enabled()], None, PropValue::Bool(enabled));
            message.handled = true;
            cx.update_state(element);
        }
        Body::UpdateState { explicit, focused } => {
            let (explicit, focused) = (*explicit, *focused);
            message.handled = true;
            cx.play_focus_state_frames(element, explicit, focused);
        }
        Body::GetChildByIndex { index, child } => {
            let wanted = *index;
            let children: Vec<ElementId> = cx.tree.children(element).collect();
            *child = children.into_iter().find(|candidate| {
                cx.runtime
                    .states
                    .get(candidate)
                    .and_then(|state| state.control.as_ref())
                    .is_some_and(|state| state.index == Some(wanted))
            });
            message.handled = true;
        }
        Body::GetSourceText(query) => {
            if query.item == 0 && query.association == 0 {
                query.text = shared_text(cx, element);
                query.point_size = match cx.tree.property(element, &[props::point_size()], None) {
                    Some(PropValue::Float(size)) if size != -1.0 => Some(size),
                    _ => None,
                };
                message.handled = true;
            }
        }
        Body::SetSourceText {
            item: 0,
            association: 0,
            text,
            ..
        } => {
            let new = text.clone();
            message.handled = set_text(cx, element, new);
        }
        Body::GetSourceImage(query) => {
            if query.item == 0 && query.association == 0 {
                query.path = cx
                    .tree
                    .string(element, &[props::image_path()])
                    .map(Rc::from);
                message.handled = true;
            }
        }
        Body::SetSourceImage {
            item: 0,
            association: 0,
            path,
            ..
        } => {
            let value = PropValue::String(path.as_deref().map(Into::into));
            message.handled = cx
                .tree
                .set_property(element, &[props::image_path()], None, value)
                .is_ok();
        }
        Body::GetTextVersion {
            version,
            item: 0,
            association: 0,
            text,
        } => {
            *version = cx
                .control_state(element)
                .map_or(0, |state| state.text_version);
            *text = shared_text(cx, element);
            message.handled = true;
        }
        Body::Press { .. } => on_press(cx, element, message),
        Body::GetNavTarget(query) => {
            if query.source != element {
                return;
            }

            if let Some(parent) = cx.tree.parent(element) {
                cx.send(parent, message);
            }

            let Body::GetNavTarget(query) = &mut message.body else {
                return;
            };
            if !message.handled {
                let direction = query.direction;
                let source = query.source;
                let target = cx.find_explicit_nav(element, direction);
                query.target = target;
                if let Some(target) = target {
                    message.handled = true;
                    query.target_is_source = target == source;
                }
            }
        }
        Body::GetPressed { pressed } => {
            *pressed = cx.control_state(element).is_some_and(|state| state.pressed);
            message.handled = true;
        }
        Body::SetPressed { pressed } => {
            let pressed = *pressed;
            if let Some(state) = cx.control_state(element) {
                state.pressed = pressed;
            }
            message.handled = true;
        }
        _ => {}
    }
}

/// Text of the control for its source text answers.
fn shared_text(cx: &mut Context<'_, '_>, element: ElementId) -> Option<Rc<[u16]>> {
    let Some(current) = cx.tree.string(element, &[props::text()]) else {
        if let Some(state) = cx.control_state(element) {
            state.text_share = None;
        }
        return None;
    };

    let shared = cx
        .runtime
        .states
        .get(&element)
        .and_then(|state| state.control.as_ref())
        .and_then(|control| control.text_share.as_ref());
    if let Some(shared) = shared
        && **shared == *current
    {
        return Some(Rc::clone(shared));
    }

    let text: Rc<[u16]> = Rc::from(current);
    if let Some(state) = cx.control_state(element) {
        state.text_share = Some(Rc::clone(&text));
    }

    Some(text)
}

/// XuiControl 0x7e0 for the control itself. Returns whether it was handled.
fn set_text(cx: &mut Context<'_, '_>, element: ElementId, new: Option<Rc<[u16]>>) -> bool {
    let current = cx.tree.string(element, &[props::text()]);
    if current == new.as_deref() {
        return true;
    }

    if let Some(state) = cx.control_state(element) {
        state.text_version = state.text_version.wrapping_add(1);
        if state.text_version == 0 {
            state.text_version = 1;
        }
    }

    // An empty string is stored as null.
    let stored = new
        .filter(|text| !text.is_empty())
        .map(|text| text.as_ref().into());
    cx.tree
        .set_property(element, &[props::text()], None, PropValue::String(stored))
        .is_ok()
}

/// XuiControl key down: explicit navigation and the tab order.
fn on_key_down(cx: &mut Context<'_, '_>, element: ElementId, key: &KeyData, message: &mut Message) {
    if !cx.has_focus(element) {
        return;
    }

    let keyboard = !cx.runtime.keyboard_keys_disabled;
    let Some(nav) = key_to_nav(key, keyboard) else {
        return;
    };

    let shift = key.flags.contains(KeyFlags::SHIFT);
    let direction = match nav {
        NavKey::Left => NavDirection::Left,
        NavKey::Right => NavDirection::Right,
        NavKey::Up => NavDirection::Up,
        NavKey::Down => NavDirection::Down,
        NavKey::Tab if shift => NavDirection::TabBackward,
        NavKey::Tab => NavDirection::TabForward,
        NavKey::PageUp | NavKey::PageDown => return,
    };

    let user = cx.focus_user(element).unwrap_or(FocusUser::Shared);

    let query = cx.send_body(
        element,
        Body::GetNavTarget(NavQuery {
            direction,
            source: element,
            target: None,
            target_is_source: false,
        }),
    );
    if query.handled {
        message.handled = true;
        if let Body::GetNavTarget(query) = query.body
            && !query.target_is_source
        {
            let _ = cx.set_focus(query.target, user);
        }
        return;
    }

    if nav != NavKey::Tab {
        return;
    }

    message.handled = true;
    let kind = if shift {
        TabStopKind::Previous
    } else {
        TabStopKind::Next
    };
    let stop = cx.send_body(
        element,
        Body::FindTabStop(NavQuery {
            direction: kind,
            source: element,
            target: None,
            target_is_source: false,
        }),
    );
    if stop.handled
        && let Body::FindTabStop(query) = stop.body
        && !query.target_is_source
    {
        let _ = cx.set_focus(query.target, user);
    }
}

/// `XuiControl_OnPress` 0x9215a7e0.
fn on_press(cx: &mut Context<'_, '_>, element: ElementId, message: &mut Message) {
    message.handled = true;
    let Body::Press {
        user, from_input, ..
    } = message.body
    else {
        return;
    };

    if !cx.is_enabled(element) {
        let has_frames = cx.control_visual(element).is_some_and(|visual| {
            cx.find_named_frame(visual, "PressDisable").is_some()
                && cx.find_named_frame(visual, "EndPressDisable").is_some()
        });
        if has_frames {
            set_accepted(message);
            if from_input {
                cx.runtime.focus.set_pressed(user, Some(element));
            }
            cx.set_pressed(element, true);
            if let Some(state) = cx.control_state(element) {
                state.press_user = user;
            }
            cx.update_state(element);
        }
        return;
    }

    if cx.is_pressed(element) {
        cx.notify_pressing(element, user, true);
        return;
    }

    if cx.notify_pressing(element, user, false) {
        return;
    }

    set_accepted(message);
    if from_input {
        cx.runtime.focus.set_pressed(user, Some(element));
    }
    cx.set_pressed(element, true);

    // Toggles checkboxes; classes without a value ignore it.
    let value = cx.send_body(element, Body::GetValue { value: 0 });
    let current = match value.body {
        Body::GetValue { value } => value,
        _ => 0,
    };
    cx.send_body(
        element,
        Body::SetValue {
            value: u32::from(current == 0),
        },
    );

    if cx.control_visual(element).is_some() {
        if let Some(state) = cx.control_state(element) {
            state.press_user = user;
        }
        cx.update_state(element);
    } else {
        cx.notify_press(element, user);
        cx.set_pressed(element, false);
        cx.runtime.focus.set_pressed(user, None);
    }
}

fn set_accepted(message: &mut Message) {
    if let Body::Press { accepted, .. } = &mut message.body {
        *accepted = true;
    }
}

/// `XuiControl_OnTimelineEnd` 0x9215a980: any timeline end that reaches a
/// pressed control completes the press.
fn on_timeline_end(cx: &mut Context<'_, '_>, element: ElementId) {
    if !cx.is_pressed(element) {
        return;
    }

    cx.set_pressed(element, false);
    let user = cx
        .control_state(element)
        .map_or(FocusUser::Shared, |state| state.press_user);
    cx.runtime.focus.set_pressed(user, None);

    if cx.is_enabled(element) {
        cx.notify_press(element, user);
    }

    cx.update_state(element);

    // Seek the state animation just started to its end frame.
    let Some(visual) = cx.control_visual(element) else {
        return;
    };
    let Some(timeline) = cx.runtime.timelines.get_mut(visual) else {
        return;
    };
    let save = timeline
        .saved_end()
        .is_some_and(|end| end != PlayEnd::Length);
    let range = PlayRange {
        start: timeline.start(),
        initial: timeline.end().frame(),
        end: timeline.end(),
        looping: timeline.is_looping(),
    };
    timeline.play(range, save);
}

impl Context<'_, '_> {
    /// `XuiControlIsPressed`: message 0x7f7.
    pub fn is_pressed(&mut self, control: ElementId) -> bool {
        let message = self.send_body(control, Body::GetPressed { pressed: false });
        matches!(message.body, Body::GetPressed { pressed: true })
    }

    /// `XuiControlSetPressed`: message 0x7f8.
    pub fn set_pressed(&mut self, control: ElementId, pressed: bool) {
        self.send_body(control, Body::SetPressed { pressed });
    }

    /// `XuiControlNotifyPress` 0x9215a758: notify 1 bubbled from the parent.
    pub fn notify_press(&mut self, control: ElementId, user: FocusUser) {
        self.notify_parent(control, NotifyKind::Press { user });
    }

    /// `XuiControlNotifyPressing` 0x9215a6e0: returns the cancel answer.
    pub fn notify_pressing(&mut self, control: ElementId, user: FocusUser, repeat: bool) -> bool {
        let message = self.notify_parent(
            control,
            NotifyKind::Pressing {
                repeat,
                cancel: false,
                user,
            },
        );

        matches!(
            message.body,
            Body::Notify(crate::message::Notify {
                kind: NotifyKind::Pressing { cancel: true, .. },
                ..
            })
        )
    }

    /// `XuiControlGetIndex`.
    pub fn control_index(&mut self, control: ElementId) -> Option<u32> {
        self.control_state(control)?.index
    }

    /// `XuiControlSetIndex`.
    pub fn set_control_index(&mut self, control: ElementId, index: Option<u32>) {
        if let Some(state) = self.control_state(control) {
            state.index = index;
        }
    }

    /// `XuiControlGetText`: 0x7df {0, 0, 0}.
    pub fn control_text(&mut self, control: ElementId) -> Option<Rc<[u16]>> {
        let message = self.send_body(
            control,
            Body::GetSourceText(crate::message::SourceTextQuery {
                item: 0,
                association: 0,
                item_data: false,
                text: None,
                point_size: None,
                extended: false,
                y_offset: 0.0,
                style: 0,
            }),
        );

        match message.body {
            Body::GetSourceText(query) => query.text,
            _ => None,
        }
    }

    /// `XuiControlSetText`: 0x7e0 {0, 0, 0, text}.
    pub fn set_control_text(&mut self, control: ElementId, text: Option<&[u16]>) {
        self.send_body(
            control,
            Body::SetSourceText {
                item: 0,
                association: 0,
                item_data: false,
                text: text.map(Rc::from),
            },
        );
    }

    /// `XuiControlGetValue`: message 0x7d0.
    pub fn control_value(&mut self, control: ElementId) -> u32 {
        let message = self.send_body(control, Body::GetValue { value: 0 });
        match message.body {
            Body::GetValue { value } => value,
            _ => 0,
        }
    }

    /// `XuiControlSetValue`: message 0x7d1.
    pub fn set_control_value(&mut self, control: ElementId, value: u32) {
        self.send_body(control, Body::SetValue { value });
    }

    /// `XuiControlGetCurItem`: message 0x7d7.
    pub fn cur_item(&mut self, control: ElementId) -> (Option<u32>, Option<ElementId>) {
        let message = self.send_body(
            control,
            Body::GetCurItem {
                index: None,
                item: None,
            },
        );
        match message.body {
            Body::GetCurItem { index, item } => (index, item),
            _ => (None, None),
        }
    }
}

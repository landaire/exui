//! XuiButton, XuiNavButton, XuiCheckbox, XuiRadioButton, XuiRadioGroup,
//! XuiLabel, XuiCaret and XuiGroup levels.

use alloc::rc::Rc;
use alloc::vec::Vec;

use exui_core::schema::PropDef;

use crate::control::Builtin;
use crate::control::ClassState;
use crate::control::StateFrames;
use crate::control::props;
use crate::input::Key;
use crate::input::VirtualKey;
use crate::message::Body;
use crate::message::Context;
use crate::message::FocusUser;
use crate::message::KeyData;
use crate::message::KeyFlags;
use crate::message::Message;
use crate::message::NavData;
use crate::message::NavQuery;
use crate::message::NotifyKind;
use crate::message::TabStopKind;
use crate::scene::ElementId;
use crate::scene::PropValue;

/// Button `PressKey` property: a key code, or one of two values matching a
/// group of keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PressKey {
    /// A, Start, and keyboard Enter and Space.
    Accept,
    /// B, Back and keyboard Escape.
    Cancel,
    /// Exactly this key code.
    Code(u32),
}

impl PressKey {
    fn from_raw(raw: u32) -> Self {
        const ACCEPT: u32 = 0x5840;
        const CANCEL: u32 = 0x5841;

        match raw {
            ACCEPT => Self::Accept,
            CANCEL => Self::Cancel,
            code => Self::Code(code),
        }
    }

    /// Matching of `XuiButton_OnKeyDown` 0x9215b6c8.
    fn matches(self, key: Key, keyboard: bool) -> bool {
        let virtual_key = key.virtual_key();
        match self {
            Self::Accept => {
                matches!(virtual_key, Some(VirtualKey::PadA | VirtualKey::PadStart))
                    || (keyboard
                        && matches!(virtual_key, Some(VirtualKey::Enter | VirtualKey::Space)))
            }
            Self::Cancel => {
                matches!(virtual_key, Some(VirtualKey::PadB | VirtualKey::PadBack))
                    || (keyboard && virtual_key == Some(VirtualKey::Escape))
            }
            Self::Code(code) => u32::from(key.raw()) == code,
        }
    }
}

fn unsigned(cx: &Context<'_, '_>, element: ElementId, def: &'static PropDef) -> u32 {
    match cx.tree.property(element, &[def], None) {
        Some(PropValue::Unsigned(value)) => value,
        _ => 0,
    }
}

/// Key down of buttons and checkboxes.
fn press_key_down(
    cx: &mut Context<'_, '_>,
    element: ElementId,
    key: &KeyData,
    press_key: PressKey,
    message: &mut Message,
) {
    let user = FocusUser::User(key.user);

    // Another press of this user is in progress.
    if cx.runtime.focus.pressed(user).is_some() {
        message.handled = true;
        return;
    }

    let keyboard = !cx.runtime.keyboard_keys_disabled;
    let matched = press_key.matches(key.key, keyboard);

    if cx.is_pressed(element) {
        // Every key is swallowed while pressed.
        message.handled = true;
        if matched {
            cx.notify_pressing(element, user, true);
        }
        return;
    }

    if !matched {
        return;
    }

    message.handled = true;
    if !key.flags.contains(KeyFlags::REPEAT) {
        cx.send_body(
            element,
            Body::Press {
                accepted: false,
                user,
                from_input: true,
            },
        );
    }
}

pub(super) fn on_button(cx: &mut Context<'_, '_>, element: ElementId, message: &mut Message) {
    match &message.body {
        Body::KeyDown(key) => {
            let key = *key;
            let press_key = PressKey::from_raw(unsigned(cx, element, props::button_press_key()));
            press_key_down(cx, element, &key, press_key, message);
        }
        Body::UpdateState { explicit, focused } => {
            let (explicit, focused) = (*explicit, *focused);
            message.handled = true;
            button_state_frames(cx, element, explicit, focused);
        }
        _ => {}
    }
}

/// `XuiButton_PlayStateFrames` 0x9215b7c8.
fn button_state_frames(
    cx: &mut Context<'_, '_>,
    element: ElementId,
    explicit: bool,
    focused: bool,
) {
    if cx.control_visual(element).is_none() {
        return;
    }

    if !cx.is_pressed(element) {
        cx.play_focus_state_frames(element, explicit, focused);
        return;
    }

    let focused = if explicit {
        focused
    } else {
        cx.has_focus(element)
    };
    let frames = if !cx.is_enabled(element) {
        StateFrames::PRESS_DISABLE
    } else if !focused {
        StateFrames::NORMAL_PRESS
    } else {
        StateFrames::PRESS
    };
    let _ = cx.play_state_frames(element, frames);
}

pub(super) fn on_nav_button(cx: &mut Context<'_, '_>, element: ElementId, message: &mut Message) {
    match &mut message.body {
        Body::GetNavData { data, .. } => {
            *data = nav_button_data(cx, element);
            message.handled = true;
        }
        Body::SetNavData { data, .. } => {
            let data = data.clone();
            let path = PropValue::String(data.path.as_deref().map(Into::into));
            let _ = cx
                .tree
                .set_property(element, &[props::press_path()], None, path);
            let _ = cx.tree.set_property(
                element,
                &[props::stay_visible()],
                None,
                PropValue::Bool(data.stay_visible),
            );
            let _ = cx.tree.set_property(
                element,
                &[props::src_trans_index()],
                None,
                PropValue::Unsigned(data.src_trans_index),
            );
            let _ = cx.tree.set_property(
                element,
                &[props::dest_trans_index()],
                None,
                PropValue::Unsigned(data.dest_trans_index),
            );
            message.handled = true;
        }
        _ => {}
    }
}

pub(crate) fn nav_button_data(cx: &Context<'_, '_>, element: ElementId) -> NavData {
    NavData {
        path: cx
            .tree
            .string(element, &[props::press_path()])
            .map(Rc::from),
        stay_visible: matches!(
            cx.tree.property(element, &[props::stay_visible()], None),
            Some(PropValue::Bool(true))
        ),
        src_trans_index: unsigned(cx, element, props::src_trans_index()),
        dest_trans_index: unsigned(cx, element, props::dest_trans_index()),
    }
}

fn is_checked(cx: &mut Context<'_, '_>, element: ElementId) -> bool {
    matches!(
        cx.class_state(element),
        Some(ClassState::Checkbox { checked: true })
    )
}

fn set_checked(cx: &mut Context<'_, '_>, element: ElementId, value: bool) {
    if let Some(ClassState::Checkbox { checked }) = cx.class_state(element) {
        *checked = value;
    }
}

/// `XuiCheckbox_OnMessage` 0x92161940, shared by XuiRadioButton.
pub(super) fn on_checkbox(cx: &mut Context<'_, '_>, element: ElementId, message: &mut Message) {
    match &mut message.body {
        Body::KeyDown(key) => {
            let key = *key;
            let def = if cx.is_builtin_a(element, Builtin::RadioButton) {
                props::radio_press_key()
            } else {
                props::checkbox_press_key()
            };
            let press_key = PressKey::from_raw(unsigned(cx, element, def));
            press_key_down(cx, element, &key, press_key, message);
        }
        Body::GetValue { value } => {
            *value = 0;
            let checked = is_checked(cx, element);
            if let Body::GetValue { value } = &mut message.body {
                *value = u32::from(checked);
            }
            message.handled = true;
        }
        Body::SetValue { value } => {
            let new = *value != 0;
            message.handled = true;
            if cx.is_pressed(element) {
                set_checked(cx, element, new);
            } else if is_checked(cx, element) != new {
                set_checked(cx, element, new);
                cx.update_state(element);
            }
        }
        Body::UpdateState { explicit, focused } => {
            let (explicit, focused) = (*explicit, *focused);
            message.handled = true;
            checkbox_state_frames(cx, element, explicit, focused);
        }
        _ => {}
    }
}

/// `XuiCheckbox_PlayStateFrames` 0x9215b950.
fn checkbox_state_frames(
    cx: &mut Context<'_, '_>,
    element: ElementId,
    explicit: bool,
    focused: bool,
) {
    if !is_checked(cx, element) {
        cx.play_focus_state_frames(element, explicit, focused);
        return;
    }

    if cx.control_visual(element).is_none() {
        return;
    }

    let focused = if explicit {
        focused
    } else {
        cx.has_focus(element)
    };
    let enabled = cx.is_enabled(element);
    let frames = match (focused, enabled) {
        (false, true) => StateFrames::NORMAL_CHECK,
        (false, false) => StateFrames::NORMAL_CHECK_DISABLE,
        (true, true) if explicit && cx.is_initial_focus_active() => StateFrames::INIT_FOCUS_CHECK,
        (true, false) if explicit && cx.is_initial_focus_active() => {
            StateFrames::INIT_FOCUS_CHECK_DISABLE
        }
        (true, true) => StateFrames::FOCUS_CHECK,
        (true, false) => StateFrames::FOCUS_CHECK_DISABLE,
    };
    let _ = cx.play_state_frames(element, frames);
}

fn radio_selected(cx: &mut Context<'_, '_>, group: ElementId) -> Option<ElementId> {
    match cx.class_state(group)? {
        ClassState::RadioGroup { selected, .. } => *selected,
        _ => None,
    }
}

fn is_checkable(cx: &Context<'_, '_>, element: ElementId) -> bool {
    cx.is_builtin_a(element, Builtin::Checkbox) || cx.is_builtin_a(element, Builtin::RadioButton)
}

/// `XuiRadioGroup_OnMessage` 0x9215bb78.
pub(super) fn on_radio_group(cx: &mut Context<'_, '_>, element: ElementId, message: &mut Message) {
    match &mut message.body {
        Body::ChildAdded { child } => {
            let child = *child;
            if is_checkable(cx, child) {
                let index = match cx.class_state(element) {
                    Some(ClassState::RadioGroup { next_index, .. }) => {
                        let index = *next_index;
                        *next_index += 1;
                        Some(index)
                    }
                    _ => None,
                };
                cx.set_control_index(child, index);
            }
        }
        Body::ChildRemoved { child } => {
            let child = *child;
            if is_checkable(cx, child) {
                let mut next = 0;
                let children: Vec<ElementId> = cx.tree.children(element).collect();
                for sibling in children {
                    if is_checkable(cx, sibling) {
                        cx.set_control_index(sibling, Some(next));
                        next += 1;
                    }
                }
                if let Some(ClassState::RadioGroup { next_index, .. }) = cx.class_state(element) {
                    *next_index = next;
                }
                cx.set_control_index(child, None);
            }
        }
        Body::SetFocus { previous, user } => {
            let (previous, user) = (*previous, *user);
            message.handled = true;
            cx.notify_parent(element, NotifyKind::SetFocus { previous });

            match radio_selected(cx, element) {
                Some(selected) => {
                    let _ = cx.set_focus(Some(selected), user);
                }
                None => {
                    let initial = cx.is_initial_focus_active();
                    let children: Vec<ElementId> = cx.tree.children(element).collect();
                    for child in children {
                        if cx.init_focus(child, user, initial).is_ok() {
                            break;
                        }
                    }
                }
            }
        }
        Body::Notify(notify) => {
            let source = notify.source;
            match notify.kind.clone() {
                NotifyKind::Press { user } => {
                    if cx.tree.parent(source) != Some(element) {
                        return;
                    }
                    message.handled = true;
                    if radio_selected(cx, element) == Some(source)
                        && let Some(ClassState::RadioGroup { selected, .. }) =
                            cx.class_state(element)
                    {
                        *selected = None;
                    }
                    radio_select(cx, element, Some(source));
                    cx.notify_press(element, user);
                }
                NotifyKind::SetFocus { previous } => {
                    let outside =
                        previous.is_none_or(|previous| !cx.is_descendant(previous, element));
                    if cx.is_descendant(source, element) && outside {
                        cx.update_state_focus(element, true);
                    }
                }
                NotifyKind::KillFocus { new }
                    if new.is_none_or(|new| !cx.is_descendant(new, element)) =>
                {
                    cx.update_state_focus(element, false);
                }
                _ => {}
            }
        }
        Body::GetSelected { selected } => {
            *selected = None;
            let current = radio_selected(cx, element);
            if let Body::GetSelected { selected } = &mut message.body {
                *selected = current;
            }
            message.handled = true;
        }
        Body::SetSelected { selected } => {
            let target = *selected;
            message.handled = true;
            radio_select(cx, element, target);
        }
        Body::GetCurItem { .. } => {
            let selected = radio_selected(cx, element);
            let index = selected.and_then(|selected| cx.control_index(selected));
            message.body = Body::GetCurItem {
                index,
                item: selected,
            };
            message.handled = true;
        }
        Body::SetCurItem { index } => {
            let wanted = *index;
            message.handled = true;
            let children: Vec<ElementId> = cx.tree.children(element).collect();
            let child = children
                .into_iter()
                .find(|child| wanted.is_some() && cx.control_index(*child) == wanted);
            radio_select(cx, element, child);
        }
        _ => {}
    }
}

/// `XuiRadioGroup_Select` 0x9215bac8.
fn radio_select(cx: &mut Context<'_, '_>, group: ElementId, target: Option<ElementId>) {
    if let Some(target) = target
        && !cx.is_descendant(target, group)
    {
        return;
    }

    let old = radio_selected(cx, group);
    if old == target {
        return;
    }

    if let Some(old) = old {
        cx.set_control_value(old, 0);
    }
    if let Some(target) = target {
        cx.set_control_value(target, 1);
    }
    if let Some(ClassState::RadioGroup { selected, .. }) = cx.class_state(group) {
        *selected = target;
    }

    let new_index = target.and_then(|target| cx.control_index(target));
    let old_index = old.and_then(|old| cx.control_index(old));
    cx.bubble_notify(
        Some(group),
        group,
        NotifyKind::SelectionChanged {
            new: new_index,
            old: old_index,
        },
    );
}

/// XuiLabel and XuiCaret: never focusable.
pub(super) fn on_unfocusable(message: &mut Message) {
    if let Body::QueryFocusable { focusable } = &mut message.body {
        *focusable = false;
        message.handled = true;
    }
}

/// XuiGroup 0x9215cc68: forwards focus to its first tab stop.
pub(super) fn on_group(cx: &mut Context<'_, '_>, element: ElementId, message: &mut Message) {
    match &message.body {
        Body::QueryFocusable { .. } => {
            let first = first_tab_stop(cx, element);
            message.body = Body::QueryFocusable {
                focusable: first.is_some_and(|first| first != element),
            };
            message.handled = true;
        }
        Body::SetFocus { previous, user } => {
            let (previous, user) = (*previous, *user);
            message.handled = true;
            cx.notify_parent(element, NotifyKind::SetFocus { previous });
            if let Some(first) = first_tab_stop(cx, element) {
                let _ = cx.set_focus(Some(first), user);
            }
        }
        _ => {}
    }
}

fn first_tab_stop(cx: &mut Context<'_, '_>, element: ElementId) -> Option<ElementId> {
    let message = cx.send_body(
        element,
        Body::FindTabStop(NavQuery {
            direction: TabStopKind::First,
            source: element,
            target: None,
            target_is_source: false,
        }),
    );

    match message.body {
        Body::FindTabStop(query) if message.handled => query.target,
        _ => None,
    }
}

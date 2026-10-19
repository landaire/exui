//! XuiScene level (0x92169720): press routing, focus memory, unfocused
//! input and the transition messages.

use alloc::rc::Rc;
use alloc::vec::Vec;

use crate::control::Builtin;
use crate::control::props;
use crate::message::Body;
use crate::message::Context;
use crate::message::FocusUser;
use crate::message::Message;
use crate::message::NotifyKind;
use crate::message::TimerId;
use crate::message::TransitionAction;
use crate::message::TransitionKind;
use crate::nav::NavUser;
use crate::scene::ElementId;
use crate::scene::PropValue;

/// XuiScene instance fields that are not properties.
#[derive(Clone, Debug)]
pub struct SceneState {
    /// Last focused descendant (+0x8).
    pub last_focus: Option<ElementId>,
    /// Unfocused-input controls in registration order (+0xc).
    pub unfocused_input: Vec<ElementId>,
    /// Skin prefix (+0x1c).
    pub skin_prefix: Option<Rc<[u16]>>,
    /// Back scene (+0x20).
    pub back: Option<ElementId>,
    /// Transition partner (+0x24).
    pub partner: Option<ElementId>,
    /// Flag bit 0: own named-frame transition running.
    pub own_transition: bool,
    /// Flag bit 1: XuiTransition wrapper running.
    pub wrapper_transition: bool,
    /// Flag bit 2, set by 0x1f.
    pub transition_control: bool,
    /// Flag bit 5, set on destroy.
    pub destroying: bool,
    /// Pending transition kind and action (+0x40, +0x44).
    pub pending: Option<(TransitionKind, TransitionAction)>,
    /// Source and destination transition indexes (+0x48, +0x4c).
    pub src_index: u32,
    pub dest_index: u32,
    /// Next child index (+0x50).
    pub next_child_index: u32,
}

impl SceneState {
    pub fn new() -> Self {
        Self {
            last_focus: None,
            unfocused_input: Vec::new(),
            skin_prefix: None,
            back: None,
            partner: None,
            own_transition: false,
            wrapper_transition: false,
            transition_control: false,
            destroying: false,
            pending: None,
            src_index: 0,
            dest_index: 0,
            next_child_index: 0,
        }
    }

    /// Flags & 3: a transition of either kind is running.
    pub fn in_transition(&self) -> bool {
        self.own_transition || self.wrapper_transition
    }
}

impl Default for SceneState {
    fn default() -> Self {
        Self::new()
    }
}

pub(super) fn on_message(cx: &mut Context<'_, '_>, element: ElementId, message: &mut Message) {
    match &mut message.body {
        Body::ChildAdded { child } => {
            let child = *child;
            child_added(cx, element, child);
        }
        Body::ChildRemoved { child } => {
            let child = *child;
            renumber_children(cx, element);
            if let Some(scene) = cx.scene_state(element) {
                scene.unfocused_input.retain(|control| *control != child);
            }
        }
        Body::Destroy => {
            if let Some(scene) = cx.scene_state(element) {
                scene.destroying = true;
            }
        }
        Body::SetFocus { previous, user } => {
            let (previous, user) = (*previous, *user);
            message.handled = true;
            scene_focus(cx, element, previous, user);
        }
        Body::Notify(notify) => {
            let source = notify.source;
            let kind = notify.kind.clone();
            on_notify(cx, element, source, kind, message);
        }
        Body::TimelineEnd { owner } => {
            if *owner != element {
                return;
            }
            let Some(scene) = cx.scene_state(element) else {
                return;
            };
            if !scene.own_transition {
                return;
            }
            scene.own_transition = false;
            let wrapper = scene.wrapper_transition;
            let pending = scene.pending;
            if !wrapper && let Some((kind, action)) = pending {
                cx.on_transition_end(element, kind, action);
            }
        }
        Body::GetBackScene { .. } => {
            let back = cx.scene_state(element).and_then(|scene| scene.back);
            message.body = Body::GetBackScene { scene: back };
            message.handled = true;
        }
        Body::SetBackScene { new, .. } => {
            let new = *new;
            let old = cx
                .scene_state(element)
                .and_then(|scene| core::mem::replace(&mut scene.back, new));
            message.body = Body::SetBackScene { new, old };
            message.handled = true;
        }
        Body::StartTransition { kind, .. } => {
            let kind = *kind;
            cx.on_start_transition(element, kind, message);
        }
        Body::GetSkinPrefix { .. } => {
            // Answered only when set, so the query keeps bubbling.
            let stored = cx
                .scene_state(element)
                .and_then(|scene| scene.skin_prefix.clone());
            if let Some(stored) = stored {
                message.body = Body::GetSkinPrefix {
                    prefix: Some(stored),
                };
                message.handled = true;
            }
        }
        Body::SetSkinPrefix { prefix } => {
            let prefix = prefix.clone().filter(|prefix| !prefix.is_empty());
            if let Some(scene) = cx.scene_state(element) {
                scene.skin_prefix = prefix;
            }
            message.handled = true;
        }
        Body::KeyDown(_) | Body::KeyUp(_) => forward_unfocused_input(cx, element, message),
        Body::Timer { id } => {
            if *id != TimerId::CONTROL {
                return;
            }
            message.handled = true;
            cx.runtime.timers.kill(element, TimerId::CONTROL);
            if !cx.accepts_input(element) {
                cx.set_input_enabled(element, true);
            }
        }
        Body::InterruptTransition => {
            message.handled = true;
            cx.on_interrupt_transition(element);
        }
        Body::GetCurItem { .. } => {
            let last = cx.scene_state(element).and_then(|scene| scene.last_focus);
            let index = last.and_then(|last| cx.control_index(last));
            message.body = Body::GetCurItem { index, item: last };
            message.handled = true;
        }
        Body::SetCurItem { index } => {
            let wanted = *index;
            message.handled = true;
            set_cur_item(cx, element, wanted);
        }
        _ => {}
    }
}

/// 0x4: index assignment and unfocused-input registration.
fn child_added(cx: &mut Context<'_, '_>, scene: ElementId, child: ElementId) {
    if cx.tree.next_sibling(child).is_some() {
        renumber_children(cx, scene);
    } else if cx.is_builtin_a(child, Builtin::Control) {
        let index = cx.scene_state(scene).map(|state| {
            let index = state.next_child_index;
            state.next_child_index += 1;
            index
        });
        cx.set_control_index(child, index);
    }

    let unfocused = cx.is_builtin_a(child, Builtin::Control)
        && matches!(
            cx.tree.property(child, &[props::unfocused_input()], None),
            Some(PropValue::Bool(true))
        );
    if unfocused && let Some(state) = cx.scene_state(scene) {
        state.unfocused_input.push(child);
    }
}

/// `XuiControlRenumberChildren` 0x92157b30.
fn renumber_children(cx: &mut Context<'_, '_>, scene: ElementId) {
    let children: Vec<ElementId> = cx.tree.children(scene).collect();
    let mut next = 0;
    for child in children {
        if cx.is_builtin_a(child, Builtin::Control) {
            cx.set_control_index(child, Some(next));
            next += 1;
        }
    }

    if let Some(state) = cx.scene_state(scene) {
        state.next_child_index = next;
    }
}

/// 0xc: restore the remembered focus, else DefaultFocus, else the first
/// child that takes initial focus.
fn scene_focus(
    cx: &mut Context<'_, '_>,
    scene: ElementId,
    previous: Option<ElementId>,
    user: FocusUser,
) {
    cx.notify_parent(scene, NotifyKind::SetFocus { previous });

    let remembered = cx
        .scene_state(scene)
        .and_then(|state| state.last_focus)
        .filter(|last| cx.tree.contains(*last) && *last != scene && cx.is_descendant(*last, scene));

    let restored = remembered.is_some_and(|last| cx.set_focus(Some(last), user).is_ok());
    if !restored {
        let default_focus: Option<Vec<u16>> = cx
            .tree
            .string(scene, &[props::default_focus()])
            .map(Into::into);
        match default_focus {
            Some(path) => {
                // A DefaultFocus that is missing or not focusable does not
                // fall back to the children.
                if let Some(target) = cx.find_by_path(scene, &path) {
                    let _ = cx.set_focus(Some(target), user);
                }
            }
            None => {
                let initial = cx.is_initial_focus_active();
                let children: Vec<ElementId> = cx.tree.children(scene).collect();
                for child in children {
                    if cx.init_focus(child, user, initial).is_ok() {
                        break;
                    }
                }
            }
        }
    }

    if cx.runtime.focus.slot(user) == Some(scene) {
        cx.update_state_focus(scene, true);
    }
}

fn on_notify(
    cx: &mut Context<'_, '_>,
    scene: ElementId,
    source: ElementId,
    kind: NotifyKind,
    message: &mut Message,
) {
    match kind {
        NotifyKind::Press { .. } => press_routing(cx, scene, source, message),
        NotifyKind::SetFocus { previous } => {
            if source == scene || !cx.is_descendant(source, scene) {
                return;
            }

            let old = cx.scene_state(scene).and_then(|state| state.last_focus);
            if let Some(state) = cx.scene_state(scene) {
                state.last_focus = Some(source);
            }

            if previous.is_none_or(|previous| !cx.is_descendant(previous, scene)) {
                cx.update_state_focus(scene, true);
            }

            if old != Some(source) {
                let new = cx.control_index(source);
                let old = old.and_then(|old| cx.control_index(old));
                cx.bubble_notify(
                    Some(scene),
                    scene,
                    NotifyKind::SelectionChanged { new, old },
                );
            }
        }
        NotifyKind::KillFocus { new }
            if source != scene
                && cx.is_descendant(source, scene)
                && new.is_none_or(|new| !cx.is_descendant(new, scene)) =>
        {
            cx.update_state_focus(scene, false);
        }
        _ => {}
    }
}

/// Notify 1: navigation for nav buttons, back buttons and lists with
/// ItemsNavPath.
fn press_routing(
    cx: &mut Context<'_, '_>,
    scene: ElementId,
    source: ElementId,
    message: &mut Message,
) {
    let ignore = matches!(
        cx.tree.property(scene, &[props::ignore_presses()], None),
        Some(PropValue::Bool(true))
    );
    if ignore {
        return;
    }

    if cx.is_builtin_a(source, Builtin::BackButton) {
        message.handled = true;
        let back = cx.back_scene(scene);
        if let Some(back) = back {
            let _ = cx.navigate_back(scene, back, NavUser::FromSource);
        }
        return;
    }

    let data = if cx.is_builtin_a(source, Builtin::NavButton) {
        message.handled = true;
        Some(super::button::nav_button_data(cx, source))
    } else if cx.is_builtin_a(source, Builtin::List) {
        let (item, _) = cx.cur_item(source);
        let reply = cx.send_body(
            source,
            Body::GetNavData {
                item,
                data: crate::message::NavData::default(),
            },
        );
        match reply.body {
            Body::GetNavData { data, .. } if data.path.is_some() => {
                message.handled = true;
                Some(data)
            }
            _ => None,
        }
    } else {
        None
    };

    let Some(data) = data else {
        return;
    };
    let Some(path) = data.path.clone() else {
        return;
    };

    let base = cx.tree.base_path(scene);
    let Ok(next) = cx.create_scene(base, &path) else {
        return;
    };

    cx.set_trans_index(scene, TransIndexSide::Source, data.src_trans_index);
    cx.set_trans_index(next, TransIndexSide::Destination, data.dest_trans_index);
    let _ = cx.navigate_forward(scene, data.stay_visible, next, NavUser::FromSource);
}

/// Scene 0x24/0x25: offers an unhandled key to the unfocused-input
/// controls in order.
fn forward_unfocused_input(cx: &mut Context<'_, '_>, scene: ElementId, message: &mut Message) {
    let controls = cx
        .scene_state(scene)
        .map(|state| state.unfocused_input.clone())
        .unwrap_or_default();

    for control in controls {
        let mut forwarded = Message::new(message.body.clone());
        cx.send(control, &mut forwarded);
        if forwarded.handled {
            message.handled = true;
            return;
        }
    }
}

/// 0x7d8: focus the child with the index for the focused user.
fn set_cur_item(cx: &mut Context<'_, '_>, scene: ElementId, index: Option<u32>) {
    let children: Vec<ElementId> = cx.tree.children(scene).collect();
    let Some(child) = children
        .into_iter()
        .find(|child| index.is_some() && cx.control_index(*child) == index)
    else {
        return;
    };

    let user = cx
        .focus_owner(scene)
        .and_then(|owner| cx.focus_user(owner))
        .unwrap_or(FocusUser::Shared);
    let old = cx.scene_state(scene).and_then(|state| state.last_focus);
    let _ = cx.set_focus(Some(child), user);

    let old = old.and_then(|old| cx.control_index(old));
    cx.bubble_notify(
        Some(scene),
        scene,
        NotifyKind::SelectionChanged { new: index, old },
    );
}

/// Which transition index of a scene `XuiSceneSetTransIndex` writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransIndexSide {
    Source,
    Destination,
}

impl Context<'_, '_> {
    /// `XuiSceneSetTransIndex`.
    pub fn set_trans_index(&mut self, scene: ElementId, side: TransIndexSide, index: u32) {
        if let Some(state) = self.scene_state(scene) {
            match side {
                TransIndexSide::Source => state.src_index = index,
                TransIndexSide::Destination => state.dest_index = index,
            }
        }
    }

    /// `XuiSceneGetBackScene`: message 0x1b.
    pub fn back_scene(&mut self, scene: ElementId) -> Option<ElementId> {
        let message = self.send_body(scene, Body::GetBackScene { scene: None });
        match message.body {
            Body::GetBackScene { scene } => scene,
            _ => None,
        }
    }

    /// `XuiSceneSetBackScene`: message 0x1c; returns the previous back
    /// scene.
    pub fn set_back_scene(
        &mut self,
        scene: ElementId,
        back: Option<ElementId>,
    ) -> Option<ElementId> {
        let message = self.send_body(
            scene,
            Body::SetBackScene {
                new: back,
                old: None,
            },
        );
        match message.body {
            Body::SetBackScene { old, .. } => old,
            _ => None,
        }
    }
}

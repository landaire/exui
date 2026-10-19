//! Focus slots, initial focus, explicit navigation paths, tab order and
//! keystroke routing (doc/xui-controls.md "Input", "Focus", "Navigation").

#[cfg(test)]
mod tests;

use alloc::vec::Vec;

use crate::control::Builtin;
use crate::error::FocusError;
use crate::input::KeyKind;
use crate::input::Keystroke;
use crate::input::UserIndex;
use crate::message::Body;
use crate::message::CharData;
use crate::message::Context;
use crate::message::FocusUser;
use crate::message::KeyData;
use crate::message::KeyFlags;
use crate::message::Message;
use crate::message::NavDirection;
use crate::message::NavQuery;
use crate::message::TabStopKind;
use crate::scene::ElementFlags;
use crate::scene::ElementId;

/// Per-user focus slots, the shared slot and the matching pressed control
/// slots.
#[derive(Clone, Debug, Default)]
pub struct FocusSlots {
    users: [Option<ElementId>; UserIndex::COUNT as usize],
    shared: Option<ElementId>,
    single_focus: bool,
    pressed_users: [Option<ElementId>; UserIndex::COUNT as usize],
    pressed_shared: Option<ElementId>,
}

impl FocusSlots {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn slot(&self, user: FocusUser) -> Option<ElementId> {
        match user {
            FocusUser::User(index) => self.users[usize::from(index.get())],
            FocusUser::Shared => self.shared,
        }
    }

    fn slot_mut(&mut self, user: FocusUser) -> &mut Option<ElementId> {
        match user {
            FocusUser::User(index) => &mut self.users[usize::from(index.get())],
            FocusUser::Shared => &mut self.shared,
        }
    }

    /// `XuiGetPressSlot`.
    pub fn pressed(&self, user: FocusUser) -> Option<ElementId> {
        match user {
            FocusUser::User(index) => self.pressed_users[usize::from(index.get())],
            FocusUser::Shared => self.pressed_shared,
        }
    }

    pub(crate) fn set_pressed(&mut self, user: FocusUser, element: Option<ElementId>) {
        match user {
            FocusUser::User(index) => self.pressed_users[usize::from(index.get())] = element,
            FocusUser::Shared => self.pressed_shared = element,
        }
    }

    pub fn is_single_focus(&self) -> bool {
        self.single_focus
    }

    /// Users whose slot holds `element`, shared slot first.
    fn users_of(&self, element: ElementId) -> impl Iterator<Item = FocusUser> + '_ {
        all_users().filter(move |user| self.slot(*user) == Some(element))
    }

    pub(crate) fn forget(&mut self, element: ElementId) {
        for slot in self
            .users
            .iter_mut()
            .chain(core::iter::once(&mut self.shared))
            .chain(self.pressed_users.iter_mut())
            .chain(core::iter::once(&mut self.pressed_shared))
        {
            if *slot == Some(element) {
                *slot = None;
            }
        }
    }
}

/// Shared slot first, then users 0 to 3.
fn all_users() -> impl Iterator<Item = FocusUser> {
    core::iter::once(FocusUser::Shared)
        .chain((0..UserIndex::COUNT).filter_map(|index| UserIndex::new(index).map(FocusUser::User)))
}

fn per_users() -> impl Iterator<Item = FocusUser> {
    (0..UserIndex::COUNT).filter_map(|index| UserIndex::new(index).map(FocusUser::User))
}

/// Result of `XuiProcessKeystroke`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyResult {
    Handled,
    Unhandled,
}

impl Context<'_, '_> {
    /// `XuiElementSetFocus` 0x921515a0. `None` clears the slot.
    pub fn set_focus(
        &mut self,
        element: Option<ElementId>,
        user: FocusUser,
    ) -> Result<(), FocusError> {
        if let Some(element) = element {
            if let FocusUser::User(own) = user
                && per_users().any(|other| {
                    other != FocusUser::User(own) && self.runtime.focus.slot(other) == Some(element)
                })
            {
                return Err(FocusError::InvalidUser);
            }

            match user {
                FocusUser::User(_) => {
                    self.runtime.focus.single_focus = false;
                    self.runtime.focus.pressed_shared = None;
                    if self.runtime.focus.shared.is_some() {
                        // Clearing a slot cannot fail.
                        let _ = self.set_focus(None, FocusUser::Shared);
                    }
                }
                FocusUser::Shared => {
                    self.runtime.focus.single_focus = true;
                    for other in per_users() {
                        self.runtime.focus.set_pressed(other, None);
                        if self.runtime.focus.slot(other).is_some() {
                            let _ = self.set_focus(None, other);
                        }
                    }
                }
            }

            let query = self.send_body(element, Body::QueryFocusable { focusable: false });
            if !matches!(query.body, Body::QueryFocusable { focusable: true }) {
                return Err(FocusError::NotFocusable);
            }
        }

        // Kill focus is sent even when the old and new elements are equal.
        let old = self.runtime.focus.slot(user);
        if let Some(old) = old {
            self.send_body(old, Body::KillFocus { new: element, user });
        }

        *self.runtime.focus.slot_mut(user) = element;

        if let Some(element) = element {
            let message = self.send_body(
                element,
                Body::SetFocus {
                    previous: old,
                    user,
                },
            );
            if !message.handled {
                *self.runtime.focus.slot_mut(user) = None;
                return Err(FocusError::Rejected);
            }
        }

        Ok(())
    }

    /// `XuiElementInitFocus` 0x92152ca0: focuses the element, or searches
    /// the children of elements that do not answer the focus query.
    pub fn init_focus(
        &mut self,
        element: ElementId,
        user: FocusUser,
        initial: bool,
    ) -> Result<(), FocusError> {
        if initial {
            self.runtime.init_focus_depth += 1;
        }

        let result = self.init_focus_search(element, user);

        if initial {
            self.runtime.init_focus_depth -= 1;
        }

        result
    }

    fn init_focus_search(&mut self, root: ElementId, user: FocusUser) -> Result<(), FocusError> {
        let mut stack = Vec::new();
        stack.push(root);

        let mut root_result = Err(FocusError::Rejected);
        while let Some(element) = stack.pop() {
            let query = self.send_body(element, Body::QueryFocusable { focusable: false });
            let focusable = matches!(query.body, Body::QueryFocusable { focusable: true });

            if focusable {
                let result = self.set_focus(Some(element), user);
                if result.is_ok() {
                    return Ok(());
                }
                if element == root {
                    root_result = result;
                }
                continue;
            }

            // An element that answered "not focusable" hides its children.
            if query.handled {
                continue;
            }

            let mut children: Vec<ElementId> = self.tree.children(element).collect();
            children.reverse();
            stack.extend(children);
        }

        root_result
    }

    /// `XuiIsInitialFocusActive`.
    pub fn is_initial_focus_active(&self) -> bool {
        self.runtime.init_focus_depth > 0
    }

    /// `XuiElementIsDescendant`: `element` is `ancestor` or inside it.
    pub fn is_descendant(&self, element: ElementId, ancestor: ElementId) -> bool {
        let mut current = Some(element);
        while let Some(node) = current {
            if node == ancestor {
                return true;
            }
            current = self.tree.parent(node);
        }

        false
    }

    /// `XuiElementGetFocusOwner`: the focused element that is `element` or
    /// inside it, shared slot first.
    pub fn focus_owner(&self, element: ElementId) -> Option<ElementId> {
        all_users()
            .filter_map(|user| self.runtime.focus.slot(user))
            .find(|focused| self.is_descendant(*focused, element))
    }

    /// `XuiElementGetFocusUser`: the user whose slot holds the element.
    pub fn focus_user(&self, element: ElementId) -> Option<FocusUser> {
        self.runtime.focus.users_of(element).next()
    }

    /// `XuiElementHasFocus`.
    pub fn has_focus(&self, element: ElementId) -> bool {
        self.focus_owner(element).is_some()
    }

    /// `XuiElementIsShown`.
    pub fn is_shown(&self, element: ElementId) -> bool {
        self.tree
            .flags(element)
            .is_some_and(|flags| flags.contains(ElementFlags::SHOW))
    }

    /// `XuiElementAcceptsInput`.
    pub fn accepts_input(&self, element: ElementId) -> bool {
        self.tree
            .flags(element)
            .is_some_and(|flags| !flags.contains(ElementFlags::INPUT_BLOCKED))
    }

    /// `XuiElementSetInputEnabled` 0x9214d388: sets or clears the input
    /// block on the whole subtree.
    pub fn set_input_enabled(&mut self, element: ElementId, enabled: bool) {
        let mut stack = Vec::new();
        stack.push(element);
        while let Some(node) = stack.pop() {
            if let Some(mut flags) = self.tree.flags(node) {
                flags.set(ElementFlags::INPUT_BLOCKED, !enabled);
                let _ = self.tree.set_flags(node, flags);
            }
            stack.extend(self.tree.children(node));
        }
    }

    /// `XuiElementIsFocusDisabled`: DisableFocusRecursion on the element,
    /// or with `recursive` on any ancestor.
    pub fn is_focus_disabled(&mut self, element: ElementId, recursive: bool) -> bool {
        let mut current = Some(element);
        while let Some(node) = current {
            let message = self.send_body(node, Body::GetFocusDisabled { disabled: false });
            if matches!(message.body, Body::GetFocusDisabled { disabled: true }) {
                return true;
            }
            if !recursive {
                return false;
            }
            current = self.tree.parent(node);
        }

        false
    }

    /// `XuiElementFindByPath` 0x92172390: `\` separated ids relative to
    /// `start`; `..` moves to the parent.
    pub fn find_by_path(&self, start: ElementId, path: &[u16]) -> Option<ElementId> {
        const PARENT: [u16; 2] = [b'.' as u16, b'.' as u16];

        let mut current = start;
        for component in path.split(|unit| *unit == u16::from(b'\\')) {
            current = if component == PARENT {
                self.tree.parent(current)?
            } else {
                self.tree.child_by_id(current, component)?
            };
        }

        Some(current)
    }

    /// `XuiElementIsFocusableControl` 0x921517c0.
    pub fn is_focusable_control(&mut self, element: ElementId) -> bool {
        if !self.is_builtin_a(element, Builtin::Control) {
            return false;
        }

        let query = self.send_body(element, Body::QueryFocusable { focusable: false });
        matches!(query.body, Body::QueryFocusable { focusable: true })
    }

    /// `XuiControlFindExplicitNav` 0x9214fdb8 with checkEnabled 0 and
    /// checkShown 1. Disabled controls stay valid targets.
    pub fn find_explicit_nav(
        &mut self,
        origin: ElementId,
        direction: NavDirection,
    ) -> Option<ElementId> {
        let mut visited: Vec<ElementId> = Vec::new();
        let mut current = origin;

        loop {
            if !self.is_builtin_a(current, Builtin::Control) || visited.contains(&current) {
                return None;
            }
            visited.push(current);

            let def = crate::control::props::nav(direction);
            let path: Vec<u16> = self.tree.string(current, &[def])?.into();
            if path.is_empty() {
                return None;
            }
            let parent = self.tree.parent(current)?;
            let target = self.find_by_path(parent, &path)?;

            if target == origin {
                return Some(target);
            }

            let skip = !self.is_shown(target) || self.is_focus_disabled(target, false);
            if !skip {
                return Some(target);
            }

            // Continue along the skipped target's own Nav property.
            current = target;
        }
    }

    /// `XuiElementFindTabStop` (0x92151c70 and helpers).
    pub fn find_tab_stop(&mut self, element: ElementId, kind: TabStopKind) -> Option<ElementId> {
        match kind {
            TabStopKind::First => self.find_in_children(element, false),
            TabStopKind::Last => self.find_in_children(element, true),
            TabStopKind::Next => self.find_sibling_stop(element, false),
            TabStopKind::Previous => self.find_sibling_stop(element, true),
        }
    }

    /// Offers a candidate: its own first or last tab stop, else itself when
    /// it is a focusable control.
    fn tab_candidate(&mut self, candidate: ElementId, reverse: bool) -> Option<ElementId> {
        if !self.is_shown(candidate) || self.is_focus_disabled(candidate, false) {
            return None;
        }

        let kind = if reverse {
            TabStopKind::Last
        } else {
            TabStopKind::First
        };
        let message = self.send_body(
            candidate,
            Body::FindTabStop(NavQuery {
                direction: kind,
                source: candidate,
                target: None,
                target_is_source: false,
            }),
        );
        if message.handled
            && let Body::FindTabStop(query) = message.body
            && !query.target_is_source
            && query.target.is_some()
        {
            return query.target;
        }

        if self.is_focusable_control(candidate) {
            return Some(candidate);
        }

        None
    }

    fn find_in_children(&mut self, element: ElementId, reverse: bool) -> Option<ElementId> {
        let mut children: Vec<ElementId> = self.tree.children(element).collect();
        if reverse {
            children.reverse();
        }

        children
            .into_iter()
            .find_map(|child| self.tab_candidate(child, reverse))
    }

    fn find_sibling_stop(&mut self, element: ElementId, reverse: bool) -> Option<ElementId> {
        let mut current = element;
        loop {
            let mut sibling = self.sibling(current, reverse);
            while let Some(candidate) = sibling {
                if let Some(found) = self.tab_candidate(candidate, reverse) {
                    return Some(found);
                }
                sibling = self.sibling(candidate, reverse);
            }

            match self.tree.parent(current) {
                Some(parent) => current = parent,
                // At the root, wrap around to the first or last stop.
                None => return self.find_in_children(current, reverse),
            }
        }
    }

    fn sibling(&self, element: ElementId, reverse: bool) -> Option<ElementId> {
        if reverse {
            self.tree.prev_sibling(element)
        } else {
            self.tree.next_sibling(element)
        }
    }

    /// `XuiProcessKeystroke` 0x92150120: one keystroke to the focused
    /// element of its user (else the shared slot), bubbled.
    pub fn process_keystroke(&mut self, keystroke: &Keystroke) -> KeyResult {
        let target = self
            .runtime
            .focus
            .slot(FocusUser::User(keystroke.user))
            .or(self.runtime.focus.shared);

        // A key with no target or a blocked target counts as handled.
        let Some(target) = target else {
            return KeyResult::Handled;
        };
        if !self.accepts_input(target) {
            return KeyResult::Handled;
        }

        let (up, flags) = match keystroke.kind {
            KeyKind::Down => (false, keystroke.flags),
            KeyKind::Repeat => (false, keystroke.flags.union(KeyFlags::REPEAT)),
            KeyKind::Up => (true, keystroke.flags),
        };

        let mut handled = false;
        if let Some(key) = keystroke.key {
            let data = KeyData {
                key,
                character: keystroke.character,
                flags,
                user: keystroke.user,
            };
            let body = if up {
                Body::KeyUp(data)
            } else {
                Body::KeyDown(data)
            };
            let mut message = Message::new(body);
            self.bubble(target, &mut message);
            handled = message.handled;
        }

        if !up && let Some(character) = keystroke.character {
            let mut message = Message::new(Body::Char(CharData {
                character,
                flags,
                user: keystroke.user,
            }));
            self.bubble(target, &mut message);
            handled |= message.handled;
        }

        if handled {
            KeyResult::Handled
        } else {
            KeyResult::Unhandled
        }
    }
}

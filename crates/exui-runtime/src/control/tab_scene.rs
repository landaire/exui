//! XuiTabScene level (0x92160100) and tab frame selection
//! (doc/xui-timelines.md section 8, doc/xui-controls.md "XuiTabScene").

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::control::ClassState;
use crate::control::props;
use crate::input::VirtualKey;
use crate::message::Body;
use crate::message::Context;
use crate::message::FocusUser;
use crate::message::KeyData;
use crate::message::Message;
use crate::message::NavKey;
use crate::message::NotifyKind;
use crate::message::key_to_nav;
use crate::scene::ElementId;
use crate::scene::PropValue;
use crate::timeline::Frame;
use crate::timeline::PlayEnd;
use crate::timeline::PlayRange;

/// XuiTabScene instance fields that are not properties.
#[derive(Clone, Copy, Debug, Default)]
pub struct TabSceneState {
    /// Flag 0x4, set through `XuiTabSceneEnableTabbing`.
    pub tabbing_disabled: bool,
    /// Flag 0x8: a tab transition is playing.
    pub animating: bool,
    /// Current tab, 0-based.
    pub current: Option<u32>,
    pub current_tab: Option<ElementId>,
}

impl TabSceneState {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Direction a key steps a tab scene in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabStep {
    Next,
    Previous,
}

fn tab_state<'c>(cx: &'c mut Context<'_, '_>, element: ElementId) -> Option<&'c mut TabSceneState> {
    match cx.class_state(element)? {
        ClassState::TabScene(state) => Some(state),
        _ => None,
    }
}

fn flag(
    cx: &Context<'_, '_>,
    element: ElementId,
    def: &'static exui_core::schema::PropDef,
) -> bool {
    matches!(
        cx.tree.property(element, &[def], None),
        Some(PropValue::Bool(true))
    )
}

fn unsigned(
    cx: &Context<'_, '_>,
    element: ElementId,
    def: &'static exui_core::schema::PropDef,
) -> u32 {
    match cx.tree.property(element, &[def], None) {
        Some(PropValue::Unsigned(value)) => value,
        _ => 0,
    }
}

fn tab_id(index: u32) -> Vec<u16> {
    format!("Tab{}", index + 1).encode_utf16().collect()
}

pub(super) fn on_message(cx: &mut Context<'_, '_>, element: ElementId, message: &mut Message) {
    match &message.body {
        Body::TimelineEnd { owner } => {
            if *owner == element
                && let Some(state) = tab_state(cx, element)
            {
                state.animating = false;
            }
        }
        Body::InitComplete => {
            let query = cx.send_body(element, Body::TabSceneInitQuery);
            if !query.handled && !flag(cx, element, props::no_auto_hide()) {
                hide_other_tabs(cx, element);
            }

            let count = unsigned(cx, element, props::tab_count());
            let current = tab_state(cx, element).and_then(|state| state.current);
            if current.is_none_or(|current| current >= count) {
                let default_tab = unsigned(cx, element, props::default_tab());
                let index = if (1..=count).contains(&default_tab) {
                    default_tab - 1
                } else {
                    0
                };
                cx.goto_tab(element, index, false);
            }
        }
        Body::KeyDown(key) => {
            let key = *key;
            on_key_down(cx, element, &key, message);
        }
        Body::SetFocus { previous, user } => {
            let (previous, user) = (*previous, *user);
            let Some(tab) = tab_state(cx, element).and_then(|state| state.current_tab) else {
                return;
            };
            message.handled = true;
            cx.notify_parent(element, NotifyKind::SetFocus { previous });
            let _ = cx.set_focus(Some(tab), user);
        }
        // Tab scenes never play their own transition frames.
        Body::StartTransition { .. } => message.handled = true,
        _ => {}
    }
}

/// `XuiTabScene_HideOtherTabs` 0x9215ffe0.
fn hide_other_tabs(cx: &mut Context<'_, '_>, element: ElementId) {
    let count = unsigned(cx, element, props::tab_count());
    let current = tab_state(cx, element).and_then(|state| state.current);

    for index in 0..count {
        if Some(index) == current {
            continue;
        }
        if let Some(tab) = cx.tree.child_by_id(element, &tab_id(index)) {
            cx.set_show(tab, false);
        }
    }
}

fn on_key_down(cx: &mut Context<'_, '_>, element: ElementId, key: &KeyData, message: &mut Message) {
    let Some(step) = cx.tab_step(element, key) else {
        return;
    };
    let next = step == TabStep::Next;

    let count = unsigned(cx, element, props::tab_count());
    let wrap = flag(cx, element, props::tab_wrap());
    let current = tab_state(cx, element).and_then(|state| state.current);
    let possible = if next {
        current.map_or(0, |current| current + 1) < count || wrap
    } else {
        current != Some(0) || wrap
    };
    if !possible {
        return;
    }

    message.handled = true;
    if !cx.tab_switch_allowed(element) {
        return;
    }

    if next {
        cx.next_tab(element);
    } else {
        cx.prev_tab(element);
    }
}

fn frame_of(cx: &Context<'_, '_>, element: ElementId, name: &str) -> Option<u32> {
    cx.find_named_frame(element, name)
}

impl Context<'_, '_> {
    /// The step `key` asks of tab scene `element`, as XuiTabScene reads
    /// its keys: right and RB, or down and RT with VerticalTabs, step to
    /// the next tab; left and LB, or up and LT, to the previous one.
    pub fn tab_step(&self, element: ElementId, key: &KeyData) -> Option<TabStep> {
        let vertical = flag(self, element, props::vertical_tabs());
        let keyboard = !self.runtime.keyboard_keys_disabled;
        let nav = key_to_nav(key, keyboard);

        let step = if vertical {
            if nav == Some(NavKey::Down) || key.key == VirtualKey::PadRightTrigger {
                TabStep::Next
            } else if nav == Some(NavKey::Up) || key.key == VirtualKey::PadLeftTrigger {
                TabStep::Previous
            } else {
                return None;
            }
        } else if nav == Some(NavKey::Right) || key.key == VirtualKey::PadRightShoulder {
            TabStep::Next
        } else if nav == Some(NavKey::Left) || key.key == VirtualKey::PadLeftShoulder {
            TabStep::Previous
        } else {
            return None;
        };

        Some(step)
    }

    /// `XuiTabScene_Next` 0x9215fcd0.
    pub fn next_tab(&mut self, element: ElementId) {
        let count = unsigned(self, element, props::tab_count());
        if count < 2 {
            return;
        }

        let wrap = flag(self, element, props::tab_wrap());
        let current = tab_state(self, element).and_then(|state| state.current);
        // The original keeps an unset index as -1, so the step from it
        // lands on index 0.
        let shown = current.map_or(0, |current| current + 1);

        let name: String = if shown < count {
            format!("{}To{}", shown, shown + 1)
        } else if wrap && shown == count {
            format!("{count}To1")
        } else {
            return;
        };

        let start = frame_of(self, element, &name);
        let end = frame_of(self, element, &format!("{name}End"));
        let new = if shown == count { 0 } else { shown };
        if let Some(state) = tab_state(self, element) {
            state.current = Some(new);
        }

        self.activate_tab(element, start, end, true);
    }

    /// `XuiTabScene_Prev` 0x9215fde8.
    pub fn prev_tab(&mut self, element: ElementId) {
        let count = unsigned(self, element, props::tab_count());
        if count < 2 {
            return;
        }

        let wrap = flag(self, element, props::tab_wrap());
        let current = tab_state(self, element).and_then(|state| state.current);

        // The original keeps an unset index as -1 and formats it with %u,
        // so the step from it names frames `0To4294967295` and stores -2.
        let (name, new) = match current {
            Some(0) if wrap => (format!("1To{count}"), count - 1),
            Some(0) => return,
            Some(current) => (format!("{}To{}", current + 1, current), current - 1),
            None => (format!("0To{}", u32::MAX), u32::MAX - 1),
        };

        let start = frame_of(self, element, &name);
        let end = frame_of(self, element, &format!("{name}End"));
        if let Some(state) = tab_state(self, element) {
            state.current = Some(new);
        }

        self.activate_tab(element, start, end, true);
    }

    /// `XuiTabScene_GotoTab` 0x9215fef0: snaps to the end pose of the
    /// transition into tab `index`.
    pub fn goto_tab(&mut self, element: ElementId, index: u32, focus: bool) -> bool {
        let count = unsigned(self, element, props::tab_count());
        if index >= count {
            return false;
        }

        let current = tab_state(self, element).and_then(|state| state.current);
        if current == Some(index) {
            return true;
        }

        let frame = if count > 1 {
            let name = if index == 0 {
                String::from("2To1End")
            } else {
                format!("{}To{}End", index, index + 1)
            };
            frame_of(self, element, &name)
        } else {
            None
        };

        let old = tab_state(self, element).and_then(|state| state.current_tab);
        if let Some(state) = tab_state(self, element) {
            state.current = Some(index);
        }
        self.activate_tab(element, frame, frame, focus);

        if let Some(old) = old
            && !flag(self, element, props::no_auto_hide())
        {
            self.set_show(old, false);
        }

        true
    }

    /// `XuiTabScene_Activate` 0x9215fb88. A missing end frame plays to the
    /// timeline length, which never raises 0xf, so the animating flag
    /// stays set.
    fn activate_tab(
        &mut self,
        element: ElementId,
        start: Option<u32>,
        end: Option<u32>,
        focus: bool,
    ) {
        if let Some(start) = start {
            if let Some(state) = tab_state(self, element) {
                state.animating = true;
            }
            let start = Frame(start as i32);
            let end = end.map_or(PlayEnd::Length, |end| {
                PlayEnd::from_frame(Frame(end as i32))
            });
            self.play_timeline(
                element,
                PlayRange {
                    start,
                    initial: start,
                    end,
                    looping: true,
                },
                false,
            );
        }

        let no_auto_hide = flag(self, element, props::no_auto_hide());
        let old = tab_state(self, element).and_then(|state| state.current_tab);
        if let Some(old) = old {
            if !no_auto_hide {
                self.set_show(old, false);
            }
            self.send_body(old, Body::TabDeactivated);
        }

        let current = tab_state(self, element).and_then(|state| state.current);
        let new = current.and_then(|current| self.tree.child_by_id(element, &tab_id(current)));
        if let Some(state) = tab_state(self, element) {
            state.current_tab = new;
        }

        let Some(new) = new else {
            return;
        };

        self.set_show(new, true);
        if focus {
            let user = self
                .focus_owner(element)
                .and_then(|owner| self.focus_user(owner))
                .unwrap_or(FocusUser::Shared);
            let _ = self.init_focus(new, user, true);
        }
        self.send_body(new, Body::TabActivated);
    }

    /// Whether a key may switch tabs of tab scene `element` now: tabbing is
    /// enabled and no tab transition plays, unless UserInterrupt is set.
    pub fn tab_switch_allowed(&mut self, element: ElementId) -> bool {
        let user_interrupt = flag(self, element, props::user_interrupt());
        let Some(state) = tab_state(self, element) else {
            return false;
        };

        !state.tabbing_disabled && (!state.animating || user_interrupt)
    }

    /// Makes tab `index` (0-based) current and runs `XuiTabScene_Activate`
    /// 0x9215fb88 with frames `start` to `end`, focusing the new tab.
    /// Returns false when the index is past TabCount.
    pub fn activate_tab_index(
        &mut self,
        element: ElementId,
        index: u32,
        start: Option<u32>,
        end: Option<u32>,
    ) -> bool {
        if index >= unsigned(self, element, props::tab_count()) {
            return false;
        }
        let Some(state) = tab_state(self, element) else {
            return false;
        };

        state.current = Some(index);
        self.activate_tab(element, start, end, true);

        true
    }

    /// `XuiTabSceneEnableTabbing` 0x9214ec80.
    pub fn enable_tabbing(&mut self, element: ElementId, enabled: bool) {
        if let Some(state) = tab_state(self, element) {
            state.tabbing_disabled = !enabled;
        }
    }

    /// Current tab index, 0-based.
    pub fn current_tab(&mut self, element: ElementId) -> Option<u32> {
        tab_state(self, element)?.current
    }
}

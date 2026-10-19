//! Control visual state frames and the fallback table
//! (xui-controls.md "Control visual state").

use crate::control::Builtin;
use crate::error::NoStateFrames;
use crate::message::Body;
use crate::message::Context;
use crate::scene::ElementId;
use crate::timeline::Frame;
use crate::timeline::PlayEnd;
use crate::timeline::PlayRange;

/// A start and end named frame pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateFrames {
    pub start: &'static str,
    pub end: &'static str,
}

const fn pair(start: &'static str, end: &'static str) -> StateFrames {
    StateFrames { start, end }
}

impl StateFrames {
    pub const NORMAL: Self = pair("Normal", "EndNormal");
    pub const FOCUS: Self = pair("Focus", "EndFocus");
    pub const KILL_FOCUS_TO_NORMAL: Self = pair("KillFocus", "EndNormal");
    pub const NORMAL_DISABLE: Self = pair("NormalDisable", "EndNormalDisable");
    pub const FOCUS_DISABLE: Self = pair("FocusDisable", "EndFocusDisable");
    pub const PRESS: Self = pair("Press", "EndPress");
    pub const NORMAL_PRESS: Self = pair("NormalPress", "EndNormalPress");
    pub const NORMAL_CHECK: Self = pair("NormalCheck", "EndNormalCheck");
    pub const FOCUS_CHECK: Self = pair("FocusCheck", "EndFocusCheck");
    pub const NORMAL_CHECK_DISABLE: Self = pair("NormalCheckDisable", "EndNormalCheckDisable");
    pub const FOCUS_CHECK_DISABLE: Self = pair("FocusCheckDisable", "EndFocusCheckDisable");
    pub const NORMAL_SEL: Self = pair("NormalSel", "EndNormalSel");
    pub const NORMAL_SEL_CHECK: Self = pair("NormalSelCheck", "EndNormalSelCheck");
    pub const NORMAL_SEL_DISABLE: Self = pair("NormalSelDisable", "EndNormalSelDisable");
    pub const NORMAL_SEL_CHECK_DISABLE: Self =
        pair("NormalSelCheckDisable", "EndNormalSelCheckDisable");
    pub const SCROLL_MORE: Self = pair("ScrollMore", "EndScrollMore");
    pub const SCROLLING: Self = pair("Scrolling", "EndScrolling");
    pub const SCROLL_MORE_DISABLE: Self = pair("ScrollMoreDisable", "EndScrollMoreDisable");
    pub const INIT_FOCUS: Self = pair("InitFocus", "EndInitFocus");
    pub const INIT_FOCUS_DISABLE: Self = pair("InitFocusDisable", "EndInitFocusDisable");
    pub const INIT_FOCUS_CHECK: Self = pair("InitFocusCheck", "EndInitFocusCheck");
    pub const INIT_FOCUS_CHECK_DISABLE: Self =
        pair("InitFocusCheckDisable", "EndInitFocusCheckDisable");
    pub const PRESS_DISABLE: Self = pair("PressDisable", "EndPressDisable");
    pub const KILL_FOCUS: Self = pair("KillFocus", "EndKillFocus");
}

/// Fallback table at 0x9200b810: each entry and the entry tried next.
const FALLBACK: [(StateFrames, Option<StateFrames>); 24] = [
    (StateFrames::NORMAL, None),
    (StateFrames::FOCUS, Some(StateFrames::NORMAL)),
    (StateFrames::KILL_FOCUS_TO_NORMAL, Some(StateFrames::NORMAL)),
    (StateFrames::NORMAL_DISABLE, Some(StateFrames::NORMAL)),
    (StateFrames::FOCUS_DISABLE, Some(StateFrames::FOCUS)),
    (StateFrames::PRESS, Some(StateFrames::FOCUS)),
    (StateFrames::NORMAL_PRESS, Some(StateFrames::PRESS)),
    (StateFrames::NORMAL_CHECK, Some(StateFrames::NORMAL)),
    (StateFrames::FOCUS_CHECK, Some(StateFrames::NORMAL_CHECK)),
    (
        StateFrames::NORMAL_CHECK_DISABLE,
        Some(StateFrames::NORMAL_CHECK),
    ),
    (
        StateFrames::FOCUS_CHECK_DISABLE,
        Some(StateFrames::FOCUS_CHECK),
    ),
    (StateFrames::NORMAL_SEL, Some(StateFrames::NORMAL)),
    (StateFrames::NORMAL_SEL_CHECK, Some(StateFrames::NORMAL_SEL)),
    (
        StateFrames::NORMAL_SEL_DISABLE,
        Some(StateFrames::NORMAL_SEL),
    ),
    (
        StateFrames::NORMAL_SEL_CHECK_DISABLE,
        Some(StateFrames::NORMAL_SEL_DISABLE),
    ),
    (StateFrames::SCROLL_MORE, Some(StateFrames::NORMAL)),
    (StateFrames::SCROLLING, Some(StateFrames::SCROLL_MORE)),
    (
        StateFrames::SCROLL_MORE_DISABLE,
        Some(StateFrames::SCROLL_MORE),
    ),
    (StateFrames::INIT_FOCUS, Some(StateFrames::FOCUS)),
    (
        StateFrames::INIT_FOCUS_DISABLE,
        Some(StateFrames::FOCUS_DISABLE),
    ),
    (
        StateFrames::INIT_FOCUS_CHECK,
        Some(StateFrames::FOCUS_CHECK),
    ),
    (
        StateFrames::INIT_FOCUS_CHECK_DISABLE,
        Some(StateFrames::FOCUS_CHECK_DISABLE),
    ),
    (StateFrames::PRESS_DISABLE, Some(StateFrames::NORMAL_PRESS)),
    (
        StateFrames::KILL_FOCUS,
        Some(StateFrames::KILL_FOCUS_TO_NORMAL),
    ),
];

fn fallback(frames: StateFrames) -> Option<StateFrames> {
    FALLBACK
        .iter()
        .find(|(entry, _)| *entry == frames)
        .and_then(|(_, fallback)| *fallback)
}

fn wide_eq(units: &[u16], text: &str) -> bool {
    units.iter().copied().eq(text.encode_utf16())
}

impl Context<'_, '_> {
    /// `XuiElementFindNamedFrame`.
    pub fn find_named_frame(&self, element: ElementId, name: &str) -> Option<u32> {
        let timeline = self.runtime.timelines.get(element)?;
        timeline
            .data()
            .named_frames
            .iter()
            .find(|frame| wide_eq(&frame.name, name))
            .map(|frame| frame.frame)
    }

    /// Plays `start` to `end` when both named frames exist (0x921502f0):
    /// `XuiElementPlayTimeline(element, s, s, e, 1, 1)`.
    pub fn play_named_frames(&mut self, element: ElementId, frames: StateFrames) -> bool {
        let Some(start) = self.find_named_frame(element, frames.start) else {
            return false;
        };
        let Some(end) = self.find_named_frame(element, frames.end) else {
            return false;
        };

        let start = Frame(start as i32);
        self.play_timeline(
            element,
            PlayRange {
                start,
                initial: start,
                end: PlayEnd::from_frame(Frame(end as i32)),
                looping: true,
            },
            true,
        );

        true
    }

    /// `XuiControlGetVisual` 0x92150240: the first child when it is a
    /// XuiVisual.
    pub fn control_visual(&self, control: ElementId) -> Option<ElementId> {
        let first = self.tree.first_child(control)?;
        self.is_builtin_a(first, Builtin::Visual).then_some(first)
    }

    /// `XuiControlPlayVisualFrames(control, start, initial, end, 1)`
    /// 0x921503C8: plays the visual from `start` to `end`, beginning at
    /// `initial`, when the three frames exist in that order.
    pub fn play_visual_frames(
        &mut self,
        control: ElementId,
        start: &str,
        initial: &str,
        end: &str,
    ) -> bool {
        let Some(visual) = self.control_visual(control) else {
            return false;
        };
        let (Some(start), Some(initial), Some(end)) = (
            self.find_named_frame(visual, start),
            self.find_named_frame(visual, initial),
            self.find_named_frame(visual, end),
        ) else {
            return false;
        };
        if initial < start || initial > end {
            return false;
        }

        let range = PlayRange {
            start: Frame(start as i32),
            initial: Frame(initial as i32),
            end: PlayEnd::from_frame(Frame(end as i32)),
            looping: true,
        };
        self.play_timeline(visual, range, true);

        true
    }

    /// `XuiControlPlayStateFrames` 0x92150448 without alternates: the pair,
    /// else the fallback chain of the matching table entry.
    pub fn play_state_frames(
        &mut self,
        control: ElementId,
        frames: StateFrames,
    ) -> Result<(), NoStateFrames> {
        let Some(visual) = self.control_visual(control) else {
            return Err(NoStateFrames);
        };

        if self.play_named_frames(visual, frames) {
            return Ok(());
        }

        let mut next = fallback(frames);
        while let Some(entry) = next {
            if self.play_named_frames(visual, entry) {
                return Ok(());
            }
            next = fallback(entry);
        }

        Err(NoStateFrames)
    }

    /// `XuiControl_PlayFocusStateFrames` 0x921579b0.
    pub fn play_focus_state_frames(&mut self, control: ElementId, explicit: bool, focused: bool) {
        if self.control_visual(control).is_none() {
            return;
        }

        let focused = if explicit {
            focused
        } else {
            self.has_focus(control)
        };
        let enabled = self.is_enabled(control);

        let frames = if !focused {
            if !enabled {
                StateFrames::NORMAL_DISABLE
            } else if !explicit {
                StateFrames::NORMAL
            } else {
                StateFrames::KILL_FOCUS
            }
        } else if explicit && self.is_initial_focus_active() {
            if enabled {
                StateFrames::INIT_FOCUS
            } else {
                StateFrames::INIT_FOCUS_DISABLE
            }
        } else if enabled {
            StateFrames::FOCUS
        } else {
            StateFrames::FOCUS_DISABLE
        };

        // A visual without any usable frames keeps its current pose.
        let _ = self.play_state_frames(control, frames);
    }

    /// `XuiControlUpdateState` 0x92158f20: 0x7d6 {0, 0}. Not sent while
    /// loading.
    pub fn update_state(&mut self, control: ElementId) {
        if self.runtime.load_depth > 0 {
            return;
        }

        self.send_body(
            control,
            Body::UpdateState {
                explicit: false,
                focused: false,
            },
        );
    }

    /// `XuiControlUpdateStateFocus` 0x92158f60: 0x7d6 {1, focused}.
    pub fn update_state_focus(&mut self, control: ElementId, focused: bool) {
        if self.runtime.load_depth > 0 {
            return;
        }

        self.send_body(
            control,
            Body::UpdateState {
                explicit: true,
                focused,
            },
        );
    }

    /// `XuiControlIsEnabled`.
    pub fn is_enabled(&mut self, control: ElementId) -> bool {
        let message = self.send_body(control, Body::GetEnabled { enabled: false });
        matches!(message.body, Body::GetEnabled { enabled: true })
    }

    /// `XuiElementGetStateFrameRange` 0x92157f60, used by sliders.
    pub fn state_frame_range(
        &self,
        element: ElementId,
        enabled: bool,
        focused: bool,
    ) -> Option<(u32, u32)> {
        let pick = |frames: StateFrames| -> Option<(u32, u32)> {
            Some((
                self.find_named_frame(element, frames.start)?,
                self.find_named_frame(element, frames.end)?,
            ))
        };

        let (primary, disabled) = if focused {
            (StateFrames::FOCUS, StateFrames::FOCUS_DISABLE)
        } else {
            (StateFrames::NORMAL, StateFrames::NORMAL_DISABLE)
        };

        let chosen = if !enabled && self.find_named_frame(element, disabled.start).is_some() {
            disabled
        } else {
            primary
        };

        pick(chosen).or_else(|| pick(StateFrames::NORMAL))
    }
}

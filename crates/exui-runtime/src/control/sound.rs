//! XuiSound (0x9216fc30) and XuiSoundXAudio (0x9214c2f8). Voices live in
//! the platform [`SoundSink`](crate::message::SoundSink); the runtime only
//! emits requests.

use alloc::rc::Rc;

use crate::control::Builtin;
use crate::control::ClassState;
use crate::control::props;
use crate::message::Body;
use crate::message::Context;
use crate::message::Message;
use crate::message::ResourceMask;
use crate::message::SoundEvent;
use crate::message::TimerId;
use crate::scene::ElementFlags;
use crate::scene::ElementId;
use crate::scene::PropValue;
use crate::time::Milliseconds;

/// Bits of the State property.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SoundStateFlags(u32);

impl SoundStateFlags {
    const NONE: Self = Self(0);
    const PLAY: Self = Self(0x2);
    const STOP: Self = Self(0x4);
    const WAIT_DONE: Self = Self(0x8);

    fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// Timer period armed by XuiSoundXAudio after starting a voice.
const XAUDIO_POLL_MS: Milliseconds = Milliseconds(10);

/// XuiSound flags and XuiSoundXAudio voice bookkeeping.
#[derive(Clone, Copy, Debug, Default)]
pub struct SoundState {
    /// Bit 0: state apply pending.
    pub pending: bool,
    /// Bit 1: inside a XuiVisual.
    pub in_visual: bool,
    /// Bit 5.
    pub playing: bool,
    /// XuiSoundXAudio has a voice.
    pub has_voice: bool,
    /// XuiSoundXAudio mute (+0x10).
    pub mute: bool,
}

impl SoundState {
    pub fn new() -> Self {
        Self::default()
    }
}

fn sound_state<'c>(cx: &'c mut Context<'_, '_>, element: ElementId) -> Option<&'c mut SoundState> {
    match cx.class_state(element)? {
        ClassState::Sound(state) => Some(state),
        _ => None,
    }
}

fn state_flags(cx: &Context<'_, '_>, element: ElementId) -> SoundStateFlags {
    match cx.tree.property(element, &[props::sound_state()], None) {
        Some(PropValue::Unsigned(bits)) => SoundStateFlags(bits),
        // The State property defaults to 0.
        _ => SoundStateFlags::NONE,
    }
}

pub(super) fn on_sound(cx: &mut Context<'_, '_>, element: ElementId, message: &mut Message) {
    match &mut message.body {
        Body::Init { .. } => {
            message.handled = true;
            let in_visual = inside_visual(cx, element);
            if let Some(state) = sound_state(cx, element) {
                state.in_visual = in_visual;
            }
        }
        Body::InitComplete => message.handled = true,
        Body::Timer { id } if *id == TimerId::CONTROL => {
            message.handled = true;
            on_timer(cx, element);
        }
        Body::SoundGetVolume { db } => {
            *db = match cx.tree.property(element, &[props::sound_volume()], None) {
                Some(PropValue::Float(volume)) => volume,
                _ => -12.0,
            };
            message.handled = true;
        }
        Body::SoundSetVolume { db } => {
            let db = *db;
            message.handled = true;
            if (-96.0..=6.0).contains(&db) {
                let _ = cx.tree.set_property(
                    element,
                    &[props::sound_volume()],
                    None,
                    PropValue::Float(db),
                );
            }
        }
        _ => {}
    }
}

/// XuiSound 0x5: any ancestor whose class is XuiVisual.
fn inside_visual(cx: &Context<'_, '_>, element: ElementId) -> bool {
    let mut current = cx.tree.parent(element);
    while let Some(node) = current {
        let is_visual = cx
            .tree
            .class(node)
            .and_then(|class| cx.registry.class(class))
            .is_some_and(|class| class.name() == "XuiVisual");
        if is_visual {
            return true;
        }
        current = cx.tree.parent(node);
    }

    false
}

/// XuiSound timer state machine.
fn on_timer(cx: &mut Context<'_, '_>, element: ElementId) {
    let flags = state_flags(cx, element);
    let pending = sound_state(cx, element).is_some_and(|state| state.pending);

    if pending {
        if flags.contains(SoundStateFlags::WAIT_DONE) && !cx.sound_done(element) {
            return;
        }

        if flags.contains(SoundStateFlags::PLAY) {
            let started = cx.send_body(element, Body::SoundPlay);
            if let Some(state) = sound_state(cx, element) {
                if started.handled {
                    state.playing = true;
                }
                state.pending = false;
            }
        } else if flags.contains(SoundStateFlags::STOP) {
            cx.send_body(element, Body::SoundStop);
            if let Some(state) = sound_state(cx, element) {
                state.playing = false;
                state.pending = false;
            }
        } else if let Some(state) = sound_state(cx, element) {
            state.pending = false;
        }
    }

    let looping = matches!(
        cx.tree.property(element, &[props::sound_loop()], None),
        Some(PropValue::Bool(true))
    );
    let Some(state) = sound_state(cx, element).copied() else {
        return;
    };

    if looping && state.playing {
        if cx.sound_done(element) {
            cx.send_body(element, Body::SoundPlay);
        }
    } else if !state.pending {
        cx.runtime.timers.kill(element, TimerId::CONTROL);
    }
}

pub(super) fn on_xaudio(cx: &mut Context<'_, '_>, element: ElementId, message: &mut Message) {
    match &mut message.body {
        Body::Render => message.handled = true,
        // Not handled: XuiSound sees the tick next.
        Body::Timer { id } if *id == TimerId::CONTROL => {
            if cx.voice_done(element) {
                release_voice(cx, element);
            }
        }
        Body::ReleaseResources { mask } => {
            if mask.contains(ResourceMask::SOUNDS) {
                release_voice(cx, element);
            }
        }
        Body::SoundPlay => {
            message.handled = true;
            let autoplay = cx
                .tree
                .flags(element)
                .is_some_and(|flags| !flags.contains(ElementFlags::AUTOPLAY_DISABLED));
            if autoplay {
                start_voice(cx, element);
            }
        }
        Body::SoundStop => {
            message.handled = true;
            stop_voice(cx, element);
        }
        Body::SoundQueryDone { done } => {
            if cx_voice_done(cx, element) {
                *done = true;
            }
        }
        Body::SoundSetVolume { db } => {
            let db = *db;
            apply_volume(cx, element, Some(db));
        }
        Body::SoundGetMute { mute } => {
            *mute = false;
            let current = sound_state(cx, element).is_some_and(|state| state.mute);
            message.body = Body::SoundGetMute { mute: current };
            message.handled = true;
        }
        Body::SoundSetMute { mute } => {
            let mute = *mute;
            message.handled = true;
            if let Some(state) = sound_state(cx, element) {
                state.mute = mute;
            }
            apply_volume(cx, element, None);
        }
        Body::Destroy => release_voice(cx, element),
        _ => {}
    }
}

fn cx_voice_done(cx: &mut Context<'_, '_>, element: ElementId) -> bool {
    cx.voice_done(element)
}

/// `StartVoice` 0x9214c050.
fn start_voice(cx: &mut Context<'_, '_>, element: ElementId) {
    let Some(file) = cx
        .tree
        .string(element, &[props::sound_file()])
        .map(Rc::<[u16]>::from)
    else {
        return;
    };

    if sound_state(cx, element).is_some_and(|state| state.has_voice) {
        stop_voice(cx, element);
    }

    // Sounds inside a visual resolve without the element's base path.
    let in_visual = sound_state(cx, element).is_some_and(|state| state.in_visual);
    let base = if in_visual {
        None
    } else {
        cx.tree.base_path(element)
    };

    let amplitude = amplitude(cx, element, None);
    let Some(sink) = &mut cx.runtime.sound else {
        return;
    };
    sink.event(SoundEvent::Start {
        element,
        file,
        base,
        amplitude: amplitude.unwrap_or(0.0),
    });

    cx.runtime
        .timers
        .set(element, TimerId::CONTROL, XAUDIO_POLL_MS);
    if let Some(state) = sound_state(cx, element) {
        state.has_voice = true;
        state.playing = true;
    }
}

/// `StopVoiceDeferred` 0x9214bde8.
fn stop_voice(cx: &mut Context<'_, '_>, element: ElementId) {
    let had_voice = sound_state(cx, element).is_some_and(|state| state.has_voice);
    if had_voice && let Some(sink) = &mut cx.runtime.sound {
        sink.event(SoundEvent::Stop { element });
    }

    if let Some(state) = sound_state(cx, element) {
        state.has_voice = false;
        state.playing = false;
    }
    cx.runtime.timers.kill(element, TimerId::CONTROL);
}

/// `ReleaseVoice` 0x9214bfb8.
fn release_voice(cx: &mut Context<'_, '_>, element: ElementId) {
    cx.runtime.timers.kill(element, TimerId::CONTROL);

    let had_voice = sound_state(cx, element).is_some_and(|state| state.has_voice);
    if had_voice && let Some(sink) = &mut cx.runtime.sound {
        sink.event(SoundEvent::Stop { element });
    }
    if let Some(state) = sound_state(cx, element) {
        state.has_voice = false;
    }
}

/// Linear amplitude for `db`, or the Volume property; `None` when out of
/// range.
fn amplitude(cx: &mut Context<'_, '_>, element: ElementId, db: Option<f32>) -> Option<f32> {
    let db = match db {
        Some(db) => db,
        None => match cx.tree.property(element, &[props::sound_volume()], None) {
            Some(PropValue::Float(volume)) => volume,
            _ => -12.0,
        },
    };
    if !(-96.0..=6.0).contains(&db) {
        return None;
    }

    let mute = sound_state(cx, element).is_some_and(|state| state.mute);
    if mute || cx.runtime.global_mute {
        return Some(0.0);
    }

    Some(libm::powf(10.0, db * 0.05))
}

/// `ApplyVolume` 0x9214bb40.
fn apply_volume(cx: &mut Context<'_, '_>, element: ElementId, db: Option<f32>) {
    let Some(amplitude) = amplitude(cx, element, db) else {
        return;
    };

    let has_voice = sound_state(cx, element).is_some_and(|state| state.has_voice);
    if has_voice && let Some(sink) = &mut cx.runtime.sound {
        sink.event(SoundEvent::Volume { element, amplitude });
    }
}

impl Context<'_, '_> {
    /// `XuiSound_ScheduleStateApply` 0x9216f610.
    pub fn schedule_sound_apply(&mut self, element: ElementId) {
        if !self.is_builtin_a(element, Builtin::Sound) {
            return;
        }

        let Some(state) = sound_state(self, element) else {
            return;
        };
        if state.pending {
            return;
        }
        state.pending = true;
        self.runtime
            .timers
            .set(element, TimerId::CONTROL, Milliseconds(0));
    }

    /// `XuiSoundPlay`.
    pub fn sound_play(&mut self, element: ElementId) {
        self.send_body(element, Body::SoundPlay);
    }

    /// `XuiSoundStop`.
    pub fn sound_stop(&mut self, element: ElementId) {
        self.send_body(element, Body::SoundStop);
    }

    /// Query done (0x7f6), bit 0.
    fn sound_done(&mut self, element: ElementId) -> bool {
        let message = self.send_body(element, Body::SoundQueryDone { done: false });
        matches!(message.body, Body::SoundQueryDone { done: true })
    }

    fn voice_done(&mut self, element: ElementId) -> bool {
        let has_voice = sound_state(self, element).is_some_and(|state| state.has_voice);
        if !has_voice {
            return true;
        }

        self.runtime
            .sound
            .as_ref()
            .is_none_or(|sink| sink.is_done(element))
    }
}

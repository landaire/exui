//! Timeline playback, named frames and keyframe evaluation
//! (doc/xui-timelines.md sections 1 to 6).
//!
//! Time is float milliseconds at 60 frames per second. Each frame the caller
//! advances every registered timeline by the measured frame time.

mod build;
mod value;

#[cfg(test)]
mod tests;

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::vec::Vec;

use exui_core::schema::PropDef;

use crate::scene::ElementId;
use crate::scene::PropValue;

pub use build::build_single_track;
pub use build::build_timeline_data;
pub use value::Segment;

/// Milliseconds per frame, the float constant at 0x9200b9bc.
pub const FRAME_MS: f32 = 16.666_666;

/// Position tolerance in milliseconds, 0x9200b9c0.
const EPS: f32 = 0.1;

/// Look-back used when searching for a named frame at a seek, 0x9200b9e0.
const SEEK_BACK: f32 = 0.2;

/// Property definition flag: apply only on the keyframe itself.
const PROP_FLAG_KEYFRAME_ONLY: u32 = 0x20;

/// A signed frame number. Play arguments and positions may be negative.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Frame(pub i32);

impl Frame {
    /// Start of the frame in milliseconds, `(float)(double)f * FMS`.
    pub fn ms(self) -> f32 {
        self.0 as f32 * FRAME_MS
    }
}

/// End of a play. `XuiTimeline_Play` takes a signed end frame and treats
/// any negative value as the timeline length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayEnd {
    /// Ends at this frame; a saved play raises 0xf there.
    At(Frame),
    /// Plays to the timeline length and never raises 0xf.
    Length,
}

impl PlayEnd {
    /// The end selected by a signed end frame argument.
    pub fn from_frame(frame: Frame) -> Self {
        if frame.0 < 0 {
            Self::Length
        } else {
            Self::At(frame)
        }
    }

    /// The signed end frame argument, -1 for [`PlayEnd::Length`]. XuiControl
    /// passes it back to `XuiTimeline_Play` as the initial frame.
    pub fn frame(self) -> Frame {
        match self {
            Self::At(frame) => frame,
            Self::Length => Frame(-1),
        }
    }
}

/// Start of an unsigned frame in milliseconds. Named frame and keyframe
/// frames are converted unsigned in most comparisons.
fn unsigned_ms(frame: u32) -> f32 {
    frame as f32 * FRAME_MS
}

/// What a named frame does when playback reaches it (section 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameCommand {
    /// Command 0 and unknown commands.
    None,
    /// Command 1.
    Stop,
    /// Commands 2 to 4: restart at the target frame with loop set and the
    /// timeline length as the end.
    Jump {
        /// Index of the target named frame; `None` when the name was not
        /// found, which makes the jump a no-op.
        target: Option<usize>,
        then: JumpFollowUp,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JumpFollowUp {
    /// Command 2.
    Continue,
    /// Command 3: the next stop command is ignored.
    SkipNextStop,
    /// Command 4: stop after the target frame values were applied.
    StopNext,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedFrame {
    pub name: Box<[u16]>,
    pub frame: u32,
    pub command: FrameCommand,
}

/// Target property of a track: the class-level definition followed by
/// compound members, and the array index.
#[derive(Clone, Debug)]
pub struct PropertyPath {
    pub defs: Box<[&'static PropDef]>,
    pub index: Option<u32>,
}

impl PropertyPath {
    fn keyframe_only(&self) -> bool {
        self.defs
            .last()
            .is_some_and(|def| def.flags.bits() & PROP_FLAG_KEYFRAME_ONLY != 0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Key {
    pub frame: u32,
    pub segment: Segment,
}

/// Immutable part of a subtimeline, shared by every instance of the loaded
/// object.
#[derive(Clone, Debug)]
pub struct TrackData {
    /// `Id` of the child the track animates.
    pub target_id: Option<Box<[u16]>>,
    pub paths: Box<[PropertyPath]>,
    pub keys: Box<[Key]>,
    /// One value per path for each key, key-major.
    pub values: Box<[PropValue]>,
}

impl TrackData {
    fn last_frame(&self) -> u32 {
        self.keys.last().map_or(0, |key| key.frame)
    }

    fn value(&self, key: usize, path: usize) -> &PropValue {
        &self.values[key * self.paths.len() + path]
    }
}

/// Immutable part of a timeline: named frames sorted by frame and tracks.
#[derive(Clone, Debug)]
pub struct TimelineData {
    pub named_frames: Box<[NamedFrame]>,
    pub tracks: Box<[TrackData]>,
}

impl TimelineData {
    /// `XuiTimeline_ComputeLength`: the last keyframe frame over all tracks.
    fn length(&self) -> u32 {
        self.tracks
            .iter()
            .map(TrackData::last_frame)
            .max()
            .unwrap_or(0)
    }

    /// `XuiElementFindNamedFrame`: first frame with this name.
    pub fn find_named_frame(&self, name: &[u16]) -> Option<u32> {
        self.named_frames
            .iter()
            .find(|frame| &*frame.name == name)
            .map(|frame| frame.frame)
    }
}

/// `PlayState` (section 1.2).
#[derive(Clone, Copy, Debug)]
struct PlayState {
    stopped: bool,
    start_pending: bool,
    start: Frame,
    end: PlayEnd,
    looping: bool,
    /// End of the last play with `save`; 0xf is raised when it is a frame.
    saved_end: Option<PlayEnd>,
}

impl PlayState {
    /// `XuiPlayState_Init`.
    fn new() -> Self {
        Self {
            stopped: true,
            start_pending: false,
            start: Frame(0),
            end: PlayEnd::At(Frame(0)),
            looping: true,
            saved_end: None,
        }
    }

    fn play(&mut self, range: PlayRange, save: bool) {
        self.stopped = false;
        self.start_pending = true;
        self.start = range.start;
        self.end = range.end;
        self.looping = range.looping;
        if save {
            self.saved_end = Some(range.end);
        }
    }

    /// `flags = 1`.
    fn stop(&mut self) {
        self.stopped = true;
        self.start_pending = false;
    }

    fn end_hit(&self, position: f32) -> bool {
        match self.saved_end {
            Some(PlayEnd::At(end)) => position >= end.ms() - EPS,
            Some(PlayEnd::Length) | None => false,
        }
    }
}

/// Arguments of `XuiElementPlayTimeline`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayRange {
    pub start: Frame,
    pub initial: Frame,
    pub end: PlayEnd,
    pub looping: bool,
}

#[derive(Clone, Debug)]
struct SubTimeline {
    play: PlayState,
    target: Option<ElementId>,
    seek_pending: bool,
    initial_offset: i32,
    position: f32,
}

/// Identifies one timeline object so that a callback that replaced or
/// destroyed it can be detected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimelineSerial(u32);

/// A timeline object of an element (section 1.3).
#[derive(Clone, Debug)]
pub struct Timeline {
    serial: TimelineSerial,
    owner: ElementId,
    data: Rc<TimelineData>,
    play: PlayState,
    subs: Vec<SubTimeline>,
    length: u32,
    seek_pending: bool,
    initial_offset: i32,
    next_named: Option<usize>,
    stop_next: bool,
    skip_stop: bool,
    position: f32,
}

impl Timeline {
    pub fn owner(&self) -> ElementId {
        self.owner
    }

    pub fn data(&self) -> &Rc<TimelineData> {
        &self.data
    }

    /// Length in frames.
    pub fn length(&self) -> u32 {
        self.length
    }

    pub fn is_stopped(&self) -> bool {
        self.play.stopped
    }

    pub fn start(&self) -> Frame {
        self.play.start
    }

    pub fn end(&self) -> PlayEnd {
        self.play.end
    }

    pub fn is_looping(&self) -> bool {
        self.play.looping
    }

    pub fn saved_end(&self) -> Option<PlayEnd> {
        self.play.saved_end
    }

    /// Position in milliseconds.
    pub fn position(&self) -> f32 {
        self.position
    }

    pub fn find_named_frame(&self, name: &[u16]) -> Option<u32> {
        self.data.find_named_frame(name)
    }

    /// `XuiTimeline_Play` 0x921545d0.
    pub fn play(&mut self, range: PlayRange, save: bool) {
        let initial_offset = range.initial.0.wrapping_sub(range.start.0);

        self.play.play(range, save);
        self.stop_next = false;
        self.skip_stop = false;
        self.seek_pending = true;
        self.next_named = None;
        self.initial_offset = initial_offset;

        for sub in &mut self.subs {
            sub.play.play(range, save);
            sub.seek_pending = true;
            sub.initial_offset = initial_offset;
        }
    }

    /// `XuiTimeline_Stop` 0x921545c0.
    pub fn stop(&mut self) {
        self.play.stop();
    }

    /// `XuiTimeline_FindNextNamedFrame`.
    fn find_next(&self, position: f32) -> Option<usize> {
        self.data
            .named_frames
            .iter()
            .position(|frame| unsigned_ms(frame.frame) >= position + EPS)
    }

    fn end_ms(&self) -> f32 {
        match self.play.end {
            PlayEnd::At(end) => end.ms(),
            PlayEnd::Length => unsigned_ms(self.length),
        }
    }
}

/// Delivery of a timeline notification (section 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotifyRoute {
    /// Timeline object: bubbled from the owner.
    Bubble(ElementId),
    /// Subtimeline: sent to the target.
    Send(ElementId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimelineNotifyKind {
    /// Message 0x14.
    Started,
    /// Message 0xf.
    Ended,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimelineNotify {
    pub kind: TimelineNotifyKind,
    pub route: NotifyRoute,
    /// Message data +0.
    pub source: ElementId,
}

/// Services the timeline code calls back into. Every call may run message
/// handlers that play, replace or destroy timelines, so the timeline is
/// looked up again after each call.
pub trait TimelineHost {
    fn timelines(&mut self) -> &mut Timelines;

    fn apply_value(&mut self, target: ElementId, path: &PropertyPath, value: &PropValue);

    fn notify(&mut self, notify: TimelineNotify);
}

/// Every timeline object, and the active list in registration order.
#[derive(Debug, Default)]
pub struct Timelines {
    by_owner: BTreeMap<ElementId, Timeline>,
    active: Vec<ElementId>,
    /// Index of the next timeline the current tick advances.
    cursor: Option<usize>,
    next_serial: u32,
}

impl Timelines {
    pub fn new() -> Self {
        Self {
            by_owner: BTreeMap::new(),
            active: Vec::new(),
            cursor: None,
            next_serial: 0,
        }
    }

    pub fn get(&self, owner: ElementId) -> Option<&Timeline> {
        self.by_owner.get(&owner)
    }

    pub fn get_mut(&mut self, owner: ElementId) -> Option<&mut Timeline> {
        self.by_owner.get_mut(&owner)
    }

    /// Creates a stopped timeline for `owner` and resolves track targets
    /// with `resolve_target`. Replaces any previous timeline of the owner
    /// (`XuiElement_SetTimelineImpl`).
    pub fn create(
        &mut self,
        owner: ElementId,
        data: Rc<TimelineData>,
        mut resolve_target: impl FnMut(Option<&[u16]>) -> Option<ElementId>,
    ) -> &mut Timeline {
        let subs = data
            .tracks
            .iter()
            .map(|track| SubTimeline {
                play: PlayState::new(),
                target: resolve_target(track.target_id.as_deref()),
                seek_pending: false,
                initial_offset: 0,
                position: 0.0,
            })
            .collect();

        let serial = TimelineSerial(self.next_serial);
        self.next_serial = self.next_serial.wrapping_add(1);

        let length = data.length();
        let timeline = Timeline {
            serial,
            owner,
            data,
            play: PlayState::new(),
            subs,
            length,
            seek_pending: true,
            initial_offset: 0,
            next_named: None,
            stop_next: false,
            skip_stop: false,
            position: 0.0,
        };

        self.remove(owner);
        self.active.push(owner);
        self.by_owner.entry(owner).or_insert(timeline)
    }

    /// `XuiTimeline_Unregister` and destroy. Keeps the tick from skipping an
    /// entry when one at or before the current position is removed.
    pub fn remove(&mut self, owner: ElementId) -> Option<Timeline> {
        let timeline = self.by_owner.remove(&owner)?;

        if let Some(slot) = self.active.iter().position(|id| *id == owner) {
            self.active.remove(slot);
            if let Some(next) = self.cursor
                && slot < next
            {
                self.cursor = Some(next - 1);
            }
        }

        Some(timeline)
    }

    fn serial(&self, owner: ElementId) -> Option<TimelineSerial> {
        self.by_owner.get(&owner).map(|timeline| timeline.serial)
    }
}

/// `XuiTimelinesTick` 0x92155b18. Timelines registered during the tick are
/// advanced in the same tick.
pub fn tick(host: &mut impl TimelineHost, dt_ms: f32) {
    host.timelines().cursor = Some(0);

    loop {
        let timelines = host.timelines();
        let Some(index) = timelines.cursor else {
            break;
        };
        let Some(owner) = timelines.active.get(index).copied() else {
            break;
        };
        timelines.cursor = Some(index + 1);

        advance(host, owner, dt_ms);
    }

    host.timelines().cursor = None;
}

/// Looks the timeline up again and checks that it is the same object.
fn current(
    host: &mut impl TimelineHost,
    owner: ElementId,
    serial: TimelineSerial,
) -> Option<&mut Timeline> {
    let timeline = host.timelines().get_mut(owner)?;
    if timeline.serial != serial {
        return None;
    }

    Some(timeline)
}

/// `XuiTimeline_Advance` 0x92155600.
pub fn advance(host: &mut impl TimelineHost, owner: ElementId, dt_ms: f32) {
    let Some(serial) = host.timelines().serial(owner) else {
        return;
    };
    let Some(timeline) = current(host, owner, serial) else {
        return;
    };

    if timeline.play.stopped {
        return;
    }

    let mut dt = dt_ms;
    if timeline.seek_pending {
        let start = Frame(timeline.play.start.0.wrapping_add(timeline.initial_offset));
        timeline.position = start.ms();
        timeline.next_named = timeline.find_next(timeline.position - SEEK_BACK);
        dt = 0.0;
        timeline.initial_offset = 0;
        timeline.seek_pending = false;
    }

    let mut end_ms = timeline.end_ms();
    let mut start_ms = timeline.play.start.ms();
    if (end_ms - start_ms).abs() < EPS {
        dt = 0.0;
    }

    let mut first = true;
    loop {
        let Some(timeline) = current(host, owner, serial) else {
            return;
        };

        if timeline.next_named.is_none() {
            timeline.next_named = timeline.find_next(timeline.position);
        }

        // Cut the step so that playback lands on the next named frame.
        let mut step = dt;
        if let Some(index) = timeline.next_named {
            let frame = timeline.data.named_frames[index].frame;
            let frame_ms = unsigned_ms(frame);
            if frame_ms >= timeline.position && timeline.position + dt > Frame(frame as i32).ms() {
                step = frame_ms - timeline.position;
                if step < EPS {
                    step = 0.0;
                }
            }
        }

        let new_position = timeline.position + step;
        timeline.position = new_position;
        if new_position >= end_ms - EPS {
            step -= new_position - end_ms;
            if step < EPS {
                step = 0.0;
            }
            timeline.position = end_ms;
        }

        let sub_count = timeline.subs.len();
        for index in 0..sub_count {
            if !advance_sub(host, owner, serial, index, step) {
                return;
            }
        }

        let Some(timeline) = current(host, owner, serial) else {
            return;
        };
        if first && timeline.play.start_pending {
            host.notify(TimelineNotify {
                kind: TimelineNotifyKind::Started,
                route: NotifyRoute::Bubble(owner),
                source: owner,
            });
            first = false;

            let Some(timeline) = current(host, owner, serial) else {
                return;
            };
            timeline.play.start_pending = false;
        }

        let Some(timeline) = current(host, owner, serial) else {
            return;
        };
        let end_hit = timeline.play.end_hit(timeline.position);

        if timeline.stop_next {
            timeline.play.stop();
            timeline.stop_next = false;
        }

        if let Some(index) = timeline.next_named {
            let named = &timeline.data.named_frames[index];
            if timeline.position >= unsigned_ms(named.frame) - EPS {
                let command = named.command;
                let count = timeline.data.named_frames.len();
                timeline.next_named = (index + 1 < count).then_some(index + 1);

                run_command(timeline, command, &mut start_ms, &mut end_ms);
            }
        }

        if new_position >= end_ms && timeline.play.looping {
            timeline.position = start_ms;
        }

        if end_hit {
            host.notify(TimelineNotify {
                kind: TimelineNotifyKind::Ended,
                route: NotifyRoute::Bubble(owner),
                source: owner,
            });
            return;
        }

        if timeline.play.stopped {
            return;
        }

        if step < EPS {
            step = EPS;
        }
        dt -= step;
        if dt <= EPS {
            return;
        }
    }
}

fn run_command(
    timeline: &mut Timeline,
    command: FrameCommand,
    start_ms: &mut f32,
    end_ms: &mut f32,
) {
    match command {
        FrameCommand::None => {}
        FrameCommand::Stop => {
            if timeline.skip_stop {
                timeline.skip_stop = false;
            } else {
                timeline.play.stop();
            }
        }
        FrameCommand::Jump { target, then } => {
            if let Some(target) = target {
                let frame = Frame(timeline.data.named_frames[target].frame as i32);
                let range = PlayRange {
                    start: frame,
                    initial: frame,
                    end: PlayEnd::from_frame(Frame(timeline.length as i32)),
                    looping: true,
                };
                timeline.play(range, false);
                timeline.position = timeline.play.start.ms();
                *start_ms = timeline.position;
                timeline.next_named = timeline.find_next(timeline.position - SEEK_BACK);
                timeline.seek_pending = false;
                *end_ms = timeline.end_ms();
            }

            match then {
                JumpFollowUp::Continue => {}
                JumpFollowUp::SkipNextStop => timeline.skip_stop = true,
                JumpFollowUp::StopNext => timeline.stop_next = true,
            }
        }
    }
}

/// `XuiSubTimeline_Advance` 0x92155508. Returns false when the timeline is
/// gone.
fn advance_sub(
    host: &mut impl TimelineHost,
    owner: ElementId,
    serial: TimelineSerial,
    index: usize,
    step: f32,
) -> bool {
    let Some(timeline) = current(host, owner, serial) else {
        return false;
    };
    let data = Rc::clone(&timeline.data);
    let track = &data.tracks[index];
    let sub = &mut timeline.subs[index];

    if sub.play.stopped || track.keys.is_empty() {
        return true;
    }

    if sub.seek_pending {
        sub.seek_pending = false;
        sub.position = Frame(sub.play.start.0.wrapping_add(sub.initial_offset)).ms();
        sub.initial_offset = 0;
    }

    let position = sub.position + step;
    apply_sub_at(host, owner, serial, index, track, position)
}

/// `XuiSubTimeline_ApplyAt` 0x92154e10.
fn apply_sub_at(
    host: &mut impl TimelineHost,
    owner: ElementId,
    serial: TimelineSerial,
    index: usize,
    track: &TrackData,
    position: f32,
) -> bool {
    let Some(timeline) = current(host, owner, serial) else {
        return false;
    };
    let sub = &mut timeline.subs[index];

    let end_ms = match sub.play.end {
        PlayEnd::At(end) => end.ms(),
        PlayEnd::Length => unsigned_ms(track.last_frame()),
    };
    let start_ms = sub.play.start.ms();
    let end_hit = sub.play.end_hit(position);

    let mut live = true;
    if position <= end_ms + EPS {
        sub.position = position;
    } else if sub.play.looping {
        // One frame past the end maps onto the start frame.
        sub.position = start_ms.max(start_ms + position - end_ms - FRAME_MS);
    } else {
        sub.position = end_ms;
        live = false;
    }

    let target = sub.target;
    let sub_position = sub.position;
    let start_pending = sub.play.start_pending;

    let key = track
        .keys
        .iter()
        .rposition(|key| unsigned_ms(key.frame) - EPS <= sub_position);

    if let Some(key) = key {
        if !apply_key(host, owner, serial, track, target, key, sub_position, live) {
            return false;
        }

        if start_pending {
            if let Some(target) = target {
                host.notify(TimelineNotify {
                    kind: TimelineNotifyKind::Started,
                    route: NotifyRoute::Send(target),
                    source: target,
                });
            }

            let Some(timeline) = current(host, owner, serial) else {
                return false;
            };
            timeline.subs[index].play.start_pending = false;
        }
    }

    if end_hit && let Some(target) = target {
        host.notify(TimelineNotify {
            kind: TimelineNotifyKind::Ended,
            route: NotifyRoute::Send(target),
            source: target,
        });

        return current(host, owner, serial).is_some();
    }

    true
}

/// Evaluates the segment starting at `key` and writes every path.
#[expect(
    clippy::too_many_arguments,
    reason = "segment evaluation needs the track, target, and playback state together"
)]
fn apply_key(
    host: &mut impl TimelineHost,
    owner: ElementId,
    serial: TimelineSerial,
    track: &TrackData,
    target: Option<ElementId>,
    key: usize,
    position: f32,
    live: bool,
) -> bool {
    let next = (key + 1).min(track.keys.len() - 1);
    let from = track.keys[key];
    let from_ms = unsigned_ms(from.frame);
    let to = (next != key).then(|| track.keys[next].frame);
    let t = segment_progress(from.segment, from.frame, to, position);

    let on_key = live && (from_ms - position).abs() <= EPS;
    for (path_index, path) in track.paths.iter().enumerate() {
        if path.keyframe_only() && !on_key {
            continue;
        }

        let Some(target) = target else {
            continue;
        };

        let first = track.value(key, path_index);
        let interpolated = from.segment.value(first, track.value(next, path_index), t);

        host.apply_value(target, path, interpolated.as_ref().unwrap_or(first));
        if current(host, owner, serial).is_none() {
            return false;
        }
    }

    true
}

/// Shaped progress at `position` ms through the segment that starts at key
/// frame `from` and ends at key frame `to` (`None` at the last key), as
/// `XuiTimeline_Update` computes it.
pub(crate) fn segment_progress(segment: Segment, from: u32, to: Option<u32>, position: f32) -> f32 {
    let mut t = match to {
        Some(to) => {
            let span = to.wrapping_sub(from) as f32 * FRAME_MS;
            (position - unsigned_ms(from)) / span
        }
        None => 0.0,
    };
    if t < 0.0 {
        t = 0.0;
    }

    segment.shape(t)
}

/// `XuiTimeline_ApplyAtFrame`: applies every track at an integer frame
/// without notifications.
pub fn apply_at_frame(host: &mut impl TimelineHost, owner: ElementId, frame: Frame) {
    let Some(serial) = host.timelines().serial(owner) else {
        return;
    };
    let Some(timeline) = current(host, owner, serial) else {
        return;
    };
    let data = Rc::clone(&timeline.data);

    for (index, track) in data.tracks.iter().enumerate() {
        let Some(timeline) = current(host, owner, serial) else {
            return;
        };
        let sub = &mut timeline.subs[index];
        if track.keys.is_empty() {
            continue;
        }

        let end = match sub.play.end {
            PlayEnd::At(end) => end,
            PlayEnd::Length => Frame(track.last_frame() as i32),
        };
        let start = sub.play.start;
        let looping = sub.play.looping;

        // Wrapping 32-bit arithmetic, as the PowerPC integer instructions
        // compute it: a span of i32::MAX wraps instead of overflowing.
        let local = if looping {
            let span = end.0.wrapping_sub(start.0);
            if span > 0 {
                Frame(
                    frame
                        .0
                        .wrapping_rem(span.wrapping_add(1))
                        .wrapping_add(start.0),
                )
            } else {
                start
            }
        } else {
            frame
        };
        sub.position = local.ms();
        let target = sub.target;

        let Some(key) = track
            .keys
            .iter()
            .rposition(|key| key.frame as i64 <= i64::from(local.0))
        else {
            continue;
        };
        let next = if key + 1 < track.keys.len() {
            key + 1
        } else if looping {
            0
        } else {
            key
        };

        let from = track.keys[key];
        let t = if next != key {
            let span = track.keys[next].frame.wrapping_sub(from.frame) as f32;
            let offset = (local.0 as f32) - from.frame as f32;
            from.segment.shape((offset / span).max(0.0))
        } else {
            0.0
        };

        let on_key = from.frame as i64 == i64::from(local.0);
        let Some(target) = target else {
            continue;
        };

        for (path_index, path) in track.paths.iter().enumerate() {
            if path.keyframe_only() && !on_key {
                continue;
            }

            let first = track.value(key, path_index);
            let interpolated = from.segment.value(first, track.value(next, path_index), t);

            host.apply_value(target, path, interpolated.as_ref().unwrap_or(first));
            if current(host, owner, serial).is_none() {
                return;
            }
        }
    }
}

/// `XuiTimeline_FinishNow` 0x9215b5c0: applies the end frame (the length
/// when the end is negative) and bubbles 0xf.
pub fn finish_now(host: &mut impl TimelineHost, owner: ElementId) {
    let Some(timeline) = host.timelines().get_mut(owner) else {
        return;
    };

    let end = match timeline.play.end {
        PlayEnd::At(end) => end,
        PlayEnd::Length => Frame(timeline.length as i32),
    };
    timeline.position = end.ms();

    apply_at_frame(host, owner, end);

    host.notify(TimelineNotify {
        kind: TimelineNotifyKind::Ended,
        route: NotifyRoute::Bubble(owner),
        source: owner,
    });
}

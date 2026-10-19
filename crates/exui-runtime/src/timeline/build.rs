//! `XuiTimeline_BuildFromObject` 0x92152080: timeline data from a loaded
//! object.

use alloc::boxed::Box;
use alloc::vec::Vec;

use exui_core::Document;
use exui_core::wide::WideStr;
use exui_core::xur::KeyframeValue;
use exui_core::xur::ObjectRef;
use exui_core::xur::StringId;
use exui_core::xur::Timeline as XurTimeline;

use crate::scene::PropValue;
use crate::timeline::FrameCommand;
use crate::timeline::JumpFollowUp;
use crate::timeline::Key;
use crate::timeline::NamedFrame;
use crate::timeline::PropertyPath;
use crate::timeline::Segment;
use crate::timeline::TimelineData;
use crate::timeline::TrackData;

/// Named frame command byte of a XUR file (xui-timelines.md 5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum XurCommand {
    Label = 0,
    Stop = 1,
    Jump = 2,
    JumpSkipNextStop = 3,
    JumpStopNext = 4,
}

impl XurCommand {
    fn from_raw(raw: u8) -> Option<Self> {
        let command = match raw {
            0 => Self::Label,
            1 => Self::Stop,
            2 => Self::Jump,
            3 => Self::JumpSkipNextStop,
            4 => Self::JumpStopNext,
            _ => return None,
        };

        Some(command)
    }
}

/// Builds the shared timeline data of an object, or `None` when the object
/// has neither timelines nor named frames (no timeline object is created).
pub fn build_timeline_data(doc: &Document<'_>, object: ObjectRef<'_, '_>) -> Option<TimelineData> {
    let records = object.timelines();
    let frames = object.named_frames();
    if records.is_empty() && frames.is_empty() {
        return None;
    }

    let named_frames = build_named_frames(doc, object);
    let tracks = records
        .iter()
        .map(|record| build_track(doc, record))
        .collect();

    Some(TimelineData {
        named_frames,
        tracks,
    })
}

/// Builds the data of one timeline record alone, as the transition code
/// uses record 0 of a skin visual.
pub fn build_single_track(doc: &Document<'_>, record: &XurTimeline) -> TimelineData {
    TimelineData {
        named_frames: Box::new([]),
        tracks: Box::new([build_track(doc, record)]),
    }
}

fn units(doc: &Document<'_>, id: Option<StringId>) -> Box<[u16]> {
    id.and_then(|id| doc.string(id))
        .map(|text| text.units().collect())
        .unwrap_or_default()
}

fn build_named_frames(doc: &Document<'_>, object: ObjectRef<'_, '_>) -> Box<[NamedFrame]> {
    let frames = object.named_frames();

    // Targets are resolved by a linear name search, only for jump commands.
    let find = |name: Option<StringId>| -> Option<usize> {
        let wanted: Option<WideStr<'_>> = name.and_then(|id| doc.string(id));
        frames.iter().position(|frame| {
            let candidate = frame.name.and_then(|id| doc.string(id));
            match (candidate, wanted) {
                (Some(candidate), Some(wanted)) => candidate.units().eq(wanted.units()),
                (None, None) => true,
                (Some(candidate), None) => candidate.is_empty(),
                (None, Some(wanted)) => wanted.is_empty(),
            }
        })
    };

    frames
        .iter()
        .map(|frame| {
            let command = match XurCommand::from_raw(frame.command.0) {
                Some(XurCommand::Stop) => FrameCommand::Stop,
                Some(XurCommand::Jump) => FrameCommand::Jump {
                    target: find(frame.target),
                    then: JumpFollowUp::Continue,
                },
                Some(XurCommand::JumpSkipNextStop) => FrameCommand::Jump {
                    target: find(frame.target),
                    then: JumpFollowUp::SkipNextStop,
                },
                Some(XurCommand::JumpStopNext) => FrameCommand::Jump {
                    target: find(frame.target),
                    then: JumpFollowUp::StopNext,
                },
                // Other values are ignored at run time.
                Some(XurCommand::Label) | None => FrameCommand::None,
            };

            NamedFrame {
                name: units(doc, frame.name),
                frame: frame.time.0,
                command,
            }
        })
        .collect()
}

fn build_track(doc: &Document<'_>, record: &XurTimeline) -> TrackData {
    let properties = doc.timeline_properties(record);

    // Only properties with keyframe values can be applied; the others carry
    // no value in the file.
    let paths: Vec<PropertyPath> = properties
        .iter()
        .filter(|property| property.has_value())
        .filter_map(|property| {
            let path = property.path?;
            Some(PropertyPath {
                defs: doc.path_defs(&path).into(),
                index: property.element.map(|index| index.0),
            })
        })
        .collect();

    let keyframes = doc.keyframes(record);
    let mut keys = Vec::with_capacity(keyframes.len());
    let mut values = Vec::with_capacity(keyframes.len() * paths.len());
    for keyframe in keyframes {
        keys.push(Key {
            frame: keyframe.time.0,
            segment: Segment::from_xur(
                keyframe.interpolation.0,
                keyframe.ease_in.0,
                keyframe.ease_out.0,
                keyframe.ease_scale.0,
            ),
        });

        for value in doc.keyframe_values(keyframe).iter().take(paths.len()) {
            values.push(convert(doc, value));
        }
    }

    TrackData {
        target_id: record.element.map(|id| units(doc, Some(id))),
        paths: paths.into_boxed_slice(),
        keys: keys.into_boxed_slice(),
        values: values.into_boxed_slice(),
    }
}

fn convert(doc: &Document<'_>, value: &KeyframeValue) -> PropValue {
    match *value {
        KeyframeValue::Bool(value) => PropValue::Bool(value),
        KeyframeValue::Integer(value) => PropValue::Integer(value),
        KeyframeValue::Unsigned(value) => PropValue::Unsigned(value),
        KeyframeValue::Float(value) => PropValue::Float(value),
        KeyframeValue::String(id) => PropValue::String(
            id.and_then(|id| doc.string(id))
                .map(|text| text.units().collect()),
        ),
        KeyframeValue::Color(value) => PropValue::Color(value),
        KeyframeValue::Vector(value) => PropValue::Vector(value),
        KeyframeValue::Quaternion(value) => PropValue::Quaternion(value),
    }
}

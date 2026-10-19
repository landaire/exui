//! Scene patches on owned scene trees: element removal, overlay merge and
//! pose holds.

use alloc::string::String;
use alloc::vec::Vec;

use exui_core::tree::Keyframe;
use exui_core::tree::KeyframeValue;
use exui_core::tree::NamedFrame;
use exui_core::tree::Object;
use exui_core::tree::Timeline;
use exui_core::wide::WideString;
use exui_core::xur::EaseIn;
use exui_core::xur::EaseOut;
use exui_core::xur::EaseScale;
use exui_core::xur::FrameTime;
use exui_core::xur::Interpolation;
use exui_extension::ElementPath;
use exui_extension::PoseHold;

use crate::extension::ScenePatchError;
use crate::scene::PropValue;
use crate::timeline::FRAME_MS;
use crate::timeline::Segment;
use crate::timeline::segment_progress;

/// What [`add_keyframes`] does with a keyframe at the time of an existing
/// one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SameTime {
    Replace,
    Keep,
}

/// Removes the element `path` names, and the timelines of its parent that
/// animate it when no other child has its `Id`.
pub(crate) fn remove(root: &mut Object, path: &ElementPath) -> Result<(), ScenePatchError> {
    let not_found = || ScenePatchError::ElementNotFound { path: path.clone() };
    let Some((last, parents)) = path.0.split_last() else {
        return Err(ScenePatchError::RemoveRoot);
    };

    let mut parent = root;
    for id in parents {
        let id: Vec<u16> = id.encode_utf16().collect();
        parent = parent.child_by_id_mut(&id).ok_or_else(not_found)?;
    }

    let last: Vec<u16> = last.encode_utf16().collect();
    let position = parent
        .children
        .iter()
        .position(|child| child.has_id(&last))
        .ok_or_else(not_found)?;
    parent.children.remove(position);

    if parent.child_by_id(&last).is_none() {
        parent.timelines.retain(|timeline| {
            timeline
                .element
                .as_ref()
                .is_none_or(|element| element.units() != last.as_slice())
        });
    }

    Ok(())
}

/// Merges `overlay` into `root`, which it stands for.
///
/// - Properties of the overlay replace those of the target with the same
///   definition, or are added.
/// - A child with an `Id` that a target child has (the first such child,
///   as timelines bind) is merged into that child; its class must be the
///   same. Other children are inserted after the target position of the
///   overlay child before them, or first.
/// - Named frames replace those of the same name, or are added; frames
///   stay in time order.
/// - A timeline whose element and property paths equal one of the target's
///   adds its keyframes there (see [`add_keyframes`]), replacing keyframes
///   at the same time; other timelines are appended.
pub(crate) fn merge(root: &mut Object, overlay: Object) -> Result<(), ScenePatchError> {
    // Overlay objects with the child index path of the target they merge
    // into. Merging a subtree leaves the indices of other subtrees alone.
    let mut pending: Vec<(Vec<usize>, Object)> = alloc::vec![(Vec::new(), overlay)];

    while let Some((path, mut overlay)) = pending.pop() {
        let mut target = &mut *root;
        for &index in &path {
            target = &mut target.children[index];
        }

        if target.class != overlay.class {
            return Err(ScenePatchError::ClassMismatch {
                id: overlay.id().cloned(),
                target: target.class,
                overlay: overlay.class,
            });
        }

        for property in core::mem::take(&mut overlay.properties) {
            match target.property_mut(property.def) {
                Some(existing) => *existing = property,
                None => target.properties.push(property),
            }
        }

        let mut merges: Vec<(usize, Object)> = Vec::new();
        let mut cursor = 0;
        for child in core::mem::take(&mut overlay.children) {
            let matched = child.ids().find_map(|id| {
                target
                    .children
                    .iter()
                    .position(|existing| existing.has_id(id.units()))
            });
            match matched {
                Some(position) => {
                    merges.push((position, child));
                    cursor = position + 1;
                }
                None => {
                    target.children.insert(cursor, child);
                    for (position, _) in &mut merges {
                        if *position >= cursor {
                            *position += 1;
                        }
                    }
                    cursor += 1;
                }
            }
        }

        for frame in core::mem::take(&mut overlay.named_frames) {
            merge_named_frame(&mut target.named_frames, frame);
        }

        for timeline in core::mem::take(&mut overlay.timelines) {
            merge_timeline(&mut target.timelines, timeline);
        }

        for (position, child) in merges {
            let mut child_path = path.clone();
            child_path.push(position);
            pending.push((child_path, child));
        }
    }

    Ok(())
}

/// Applies a [`PoseHold`]: keys each timeline of the element at the named
/// frames with its value at the source frame, evaluated as the timeline
/// runtime does (see [`add_keyframes`]). Timelines with no keyframe at or
/// before the source frame are left alone.
pub(crate) fn hold(root: &mut Object, hold: &PoseHold) -> Result<(), ScenePatchError> {
    let mut element = root;
    for id in &hold.element.0 {
        let units: Vec<u16> = id.encode_utf16().collect();
        element =
            element
                .child_by_id_mut(&units)
                .ok_or_else(|| ScenePatchError::ElementNotFound {
                    path: hold.element.clone(),
                })?;
    }

    let frame_time = |name: &str| {
        element
            .named_frames
            .iter()
            .find(|frame| frame.name.as_ref().is_some_and(|text| text.eq_str(name)))
            .map(|frame| frame.time)
            .ok_or_else(|| ScenePatchError::FrameNotFound {
                element: hold.element.clone(),
                frame: String::from(name),
            })
    };
    let source = frame_time(&hold.source)?;
    let mut targets = Vec::with_capacity(hold.frames.len());
    for name in &hold.frames {
        targets.push(frame_time(name)?);
    }

    for timeline in &mut element.timelines {
        // Without a keyframe at or before the source the runtime applies
        // nothing there, so the element shows whatever was applied last;
        // the timeline has no pose to hold.
        let Some(values) = values_at(timeline, source) else {
            continue;
        };
        let keyframes = targets.iter().map(|&time| Keyframe {
            time,
            interpolation: Interpolation(0),
            ease_in: EaseIn(0),
            ease_out: EaseOut(0),
            ease_scale: EaseScale(0),
            values: values.clone(),
        });
        add_keyframes(timeline, keyframes, SameTime::Keep);
    }

    Ok(())
}

fn merge_named_frame(frames: &mut Vec<NamedFrame>, frame: NamedFrame) {
    let existing = frames
        .iter()
        .position(|existing| existing.name.is_some() && existing.name == frame.name);
    if let Some(existing) = existing {
        frames.remove(existing);
    }

    let position = frames.partition_point(|existing| existing.time <= frame.time);
    frames.insert(position, frame);
}

fn merge_timeline(timelines: &mut Vec<Timeline>, timeline: Timeline) {
    let existing = timelines.iter_mut().find(|existing| {
        existing.element == timeline.element && existing.properties == timeline.properties
    });
    let Some(existing) = existing else {
        timelines.push(timeline);
        return;
    };

    add_keyframes(existing, timeline.keyframes, SameTime::Replace);
}

/// Adds `keyframes` to `timeline` in time order. A keyframe added after
/// the last one would make that one interpolate toward it and change every
/// frame between them, so the former last keyframe holds instead, as it
/// did with nothing after it. It keeps its own interpolation when a new
/// keyframe replaces it.
fn add_keyframes(
    timeline: &mut Timeline,
    keyframes: impl IntoIterator<Item = Keyframe>,
    same_time: SameTime,
) {
    const HOLD: u8 = 0x1;

    // Tracked by index: a search by time can miss it when the keyframes of
    // the data are out of time order.
    let mut former_last = timeline.keyframes.len().checked_sub(1);
    for keyframe in keyframes {
        let position = timeline
            .keyframes
            .partition_point(|existing| existing.time < keyframe.time);
        let existing = timeline
            .keyframes
            .get_mut(position)
            .filter(|existing| existing.time == keyframe.time);
        match (existing, same_time) {
            (Some(existing), SameTime::Replace) => {
                if former_last == Some(position) {
                    former_last = None;
                }
                *existing = keyframe;
            }
            // The existing keyframe stays.
            (Some(_), SameTime::Keep) => {}
            (None, _) => {
                timeline.keyframes.insert(position, keyframe);
                if let Some(last) = former_last.as_mut()
                    && position <= *last
                {
                    *last += 1;
                }
            }
        }
    }

    let Some(former_last) = former_last else {
        return;
    };
    if former_last + 1 < timeline.keyframes.len() {
        let former = &mut timeline.keyframes[former_last];
        former.interpolation = Interpolation(former.interpolation.0 | HOLD);
    }
}

/// Values of `timeline` at frame `time` as `apply_sub_at` and `apply_key`
/// in the timeline runtime compute them: the segment of the last keyframe
/// at or before it. `None` without such a keyframe, where the runtime
/// applies nothing.
fn values_at(timeline: &Timeline, time: FrameTime) -> Option<Vec<KeyframeValue>> {
    let key = timeline
        .keyframes
        .partition_point(|keyframe| keyframe.time <= time)
        .checked_sub(1)?;
    let next = (key + 1).min(timeline.keyframes.len() - 1);
    let from = &timeline.keyframes[key];

    let segment = Segment::from_xur(
        from.interpolation.0,
        from.ease_in.0,
        from.ease_out.0,
        from.ease_scale.0,
    );
    let to = (next != key).then(|| timeline.keyframes[next].time.0);
    let t = segment_progress(segment, from.time.0, to, time.0 as f32 * FRAME_MS);

    let values = from
        .values
        .iter()
        .zip(&timeline.keyframes[next].values)
        .map(|(first, second)| {
            segment
                .value(&prop_value(first), &prop_value(second), t)
                .and_then(keyframe_value)
                .unwrap_or_else(|| first.clone())
        })
        .collect();

    Some(values)
}

fn prop_value(value: &KeyframeValue) -> PropValue {
    match value {
        KeyframeValue::Bool(value) => PropValue::Bool(*value),
        KeyframeValue::Integer(value) => PropValue::Integer(*value),
        KeyframeValue::Unsigned(value) => PropValue::Unsigned(*value),
        KeyframeValue::Float(value) => PropValue::Float(*value),
        KeyframeValue::String(text) => {
            PropValue::String(text.as_ref().map(|text| text.units().into()))
        }
        KeyframeValue::Color(value) => PropValue::Color(*value),
        KeyframeValue::Vector(value) => PropValue::Vector(*value),
        KeyframeValue::Quaternion(value) => PropValue::Quaternion(*value),
    }
}

/// `None` for figure points, which have no keyframe form.
fn keyframe_value(value: PropValue) -> Option<KeyframeValue> {
    let value = match value {
        PropValue::Bool(value) => KeyframeValue::Bool(value),
        PropValue::Integer(value) => KeyframeValue::Integer(value),
        PropValue::Unsigned(value) => KeyframeValue::Unsigned(value),
        PropValue::Float(value) => KeyframeValue::Float(value),
        PropValue::String(text) => {
            KeyframeValue::String(text.map(|text| WideString::new(text.into_vec())))
        }
        PropValue::Color(value) => KeyframeValue::Color(value),
        PropValue::Vector(value) => KeyframeValue::Vector(value),
        PropValue::Quaternion(value) => KeyframeValue::Quaternion(value),
        PropValue::Points(_) => return None,
    };

    Some(value)
}

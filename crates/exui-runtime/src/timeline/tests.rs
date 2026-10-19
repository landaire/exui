use std::boxed::Box;
use std::format;
use std::rc::Rc;
use std::string::String;
use std::vec;
use std::vec::Vec;

use exui_core::Registry;
use exui_core::schema::PropDef;
use exui_core::schema::xui;
use exui_core::value::Color;
use exui_core::value::Vector3;

use super::*;
use crate::scene::ElementTree;
use crate::test_support::def;

struct Host {
    timelines: Timelines,
    names: Vec<(ElementId, &'static str)>,
    log: Vec<String>,
}

impl Host {
    fn name(&self, element: ElementId) -> &'static str {
        self.names
            .iter()
            .find(|(id, _)| *id == element)
            .map_or("?", |(_, name)| *name)
    }
}

impl TimelineHost for Host {
    fn timelines(&mut self) -> &mut Timelines {
        &mut self.timelines
    }

    fn apply_value(&mut self, target: ElementId, path: &PropertyPath, value: &PropValue) {
        let name = self.name(target);
        let text = match value {
            PropValue::Float(value) => format!("{value:.4}"),
            PropValue::Integer(value) => format!("{value}"),
            PropValue::Unsigned(value) => format!("{value}"),
            PropValue::Bool(value) => format!("{value}"),
            PropValue::Color(value) => format!("{:08x}", value.argb()),
            PropValue::Vector(value) => format!("({:.3},{:.3},{:.3})", value.x, value.y, value.z),
            other => format!("{other:?}"),
        };
        self.log
            .push(format!("  set {name}.{}={text}", path.defs[0].name));
    }

    fn notify(&mut self, notify: TimelineNotify) {
        let (route, element) = match notify.route {
            NotifyRoute::Bubble(element) => ("bubble", element),
            NotifyRoute::Send(element) => ("send", element),
        };
        let id = match notify.kind {
            TimelineNotifyKind::Started => "0x14",
            TimelineNotifyKind::Ended => "0xf",
        };
        let name = self.name(element);
        self.log.push(format!("  {route} {id} {name}"));
    }
}

fn wide(text: &str) -> Box<[u16]> {
    text.encode_utf16().collect()
}

fn key(frame: u32, segment: Segment) -> Key {
    Key { frame, segment }
}

fn track(target: &str, defs: &[&'static PropDef], keys: Vec<(Key, Vec<PropValue>)>) -> TrackData {
    let mut frames = Vec::new();
    let mut values = Vec::new();
    for (key, row) in keys {
        frames.push(key);
        values.extend(row);
    }

    TrackData {
        target_id: Some(wide(target)),
        paths: defs
            .iter()
            .map(|def| PropertyPath {
                defs: Box::new([*def]),
                index: None,
            })
            .collect(),
        keys: frames.into_boxed_slice(),
        values: values.into_boxed_slice(),
    }
}

fn frame(name: &str, frame: u32, command: FrameCommand) -> NamedFrame {
    NamedFrame {
        name: wide(name),
        frame,
        command,
    }
}

fn opacity() -> &'static PropDef {
    def(&xui::XUI_ELEMENT, "Opacity")
}

/// A host with an owner and one child `box`, and a timeline built from
/// `data` on the owner.
fn setup(data: TimelineData) -> (Host, ElementId) {
    let registry = Registry::new();
    let class = registry
        .find("XuiElement")
        .and_then(|id| registry.class(id))
        .unwrap();
    let mut tree = ElementTree::new();
    let owner = tree.create(class);
    let child = tree.create(class);

    let mut host = Host {
        timelines: Timelines::new(),
        names: vec![(owner, "owner"), (child, "box")],
        log: Vec::new(),
    };
    host.timelines.create(owner, Rc::new(data), |_| Some(child));

    (host, owner)
}

fn play(host: &mut Host, owner: ElementId, start: i32, initial: i32, end: i32, looping: bool) {
    let range = PlayRange {
        start: Frame(start),
        initial: Frame(initial),
        end: PlayEnd::from_frame(Frame(end)),
        looping,
    };
    host.timelines.get_mut(owner).unwrap().play(range, true);
}

fn run(host: &mut Host, owner: ElementId, steps: &[f32]) -> String {
    for dt in steps {
        host.log.push(format!("tick {dt}"));
        tick(host, *dt);
        let timeline = host.timelines.get(owner).unwrap();
        host.log.push(format!(
            "  pos {:.3} stopped {}",
            timeline.position(),
            timeline.is_stopped()
        ));
    }

    host.log.join("\n")
}

fn fade() -> TrackData {
    track(
        "box",
        &[opacity()],
        vec![
            (key(0, Segment::Linear), vec![PropValue::Float(0.0)]),
            (key(10, Segment::Linear), vec![PropValue::Float(1.0)]),
            (key(20, Segment::Hold), vec![PropValue::Float(0.5)]),
        ],
    )
}

#[test]
fn first_tick_applies_the_initial_frame_and_drops_dt() {
    let data = TimelineData {
        named_frames: Box::new([]),
        tracks: Box::new([fade()]),
    };
    let (mut host, owner) = setup(data);
    play(&mut host, owner, 0, 5, 20, false);

    insta::assert_snapshot!(run(&mut host, owner, &[100.0, 50.0, 100.0, 1000.0, 16.0]));
}

#[test]
fn stop_command_cuts_the_step_at_its_frame() {
    let data = TimelineData {
        named_frames: Box::new([
            frame("Start", 0, FrameCommand::None),
            frame("Mid", 10, FrameCommand::Stop),
            frame("End", 20, FrameCommand::None),
        ]),
        tracks: Box::new([fade()]),
    };
    let (mut host, owner) = setup(data);
    play(&mut host, owner, 0, 0, -1, true);

    insta::assert_snapshot!(run(&mut host, owner, &[0.0, 1000.0, 1000.0]));
}

#[test]
fn end_notification_repeats_while_held() {
    let data = TimelineData {
        named_frames: Box::new([]),
        tracks: Box::new([fade()]),
    };
    let (mut host, owner) = setup(data);
    play(&mut host, owner, 0, 0, 10, false);

    insta::assert_snapshot!(run(&mut host, owner, &[0.0, 100.0, 100.0, 16.0]));
}

#[test]
fn any_negative_end_plays_to_the_length_without_notifying() {
    let data = TimelineData {
        named_frames: Box::new([]),
        tracks: Box::new([fade()]),
    };
    let steps = [0.0, 200.0, 200.0];

    let (mut host, owner) = setup(data.clone());
    play(&mut host, owner, 0, 0, -1, false);
    let minus_one = run(&mut host, owner, &steps);

    // A named frame past i32::MAX converts to a negative end.
    let (mut host, owner) = setup(data);
    play(&mut host, owner, 0, 0, 0x8000_0000_u32 as i32, false);
    let wrapped = run(&mut host, owner, &steps);

    assert_eq!(minus_one, wrapped);
    assert!(!minus_one.contains("0xf"), "{minus_one}");
    assert!(minus_one.contains("pos 333.333"), "{minus_one}");
}

#[test]
fn looping_play_wraps_and_notifies_once_per_pass() {
    let data = TimelineData {
        named_frames: Box::new([]),
        tracks: Box::new([fade()]),
    };
    let (mut host, owner) = setup(data);
    play(&mut host, owner, 0, 0, 10, true);

    insta::assert_snapshot!(run(&mut host, owner, &[0.0, 100.0, 100.0, 50.0]));
}

#[test]
fn jump_commands() {
    let data = TimelineData {
        named_frames: Box::new([
            frame("Loop", 2, FrameCommand::None),
            frame(
                "Back",
                8,
                FrameCommand::Jump {
                    target: Some(0),
                    then: JumpFollowUp::StopNext,
                },
            ),
        ]),
        tracks: Box::new([fade()]),
    };
    let (mut host, owner) = setup(data);
    play(&mut host, owner, 0, 0, 20, false);

    insta::assert_snapshot!(run(&mut host, owner, &[0.0, 200.0, 16.0, 16.0]));
}

#[test]
fn skip_next_stop() {
    let data = TimelineData {
        named_frames: Box::new([
            frame("A", 2, FrameCommand::None),
            frame("Stop", 4, FrameCommand::Stop),
            frame(
                "Again",
                6,
                FrameCommand::Jump {
                    target: Some(0),
                    then: JumpFollowUp::SkipNextStop,
                },
            ),
        ]),
        tracks: Box::new([fade()]),
    };
    let (mut host, owner) = setup(data);
    // Starting past Stop: the jump back arms the skip, so Stop never halts
    // playback again.
    play(&mut host, owner, 0, 5, -1, true);

    insta::assert_snapshot!(run(&mut host, owner, &[0.0, 250.0]));
}

#[test]
fn value_types_interpolate_or_step() {
    let position = def(&xui::XUI_ELEMENT, "Position");
    let anchor = def(&xui::XUI_ELEMENT, "Anchor");
    let data = TimelineData {
        named_frames: Box::new([]),
        tracks: Box::new([track(
            "box",
            &[position, anchor, opacity()],
            vec![
                (
                    key(0, Segment::from_xur(2, 100, 0, 100)),
                    vec![
                        PropValue::Vector(Vector3::new(0.0, 0.0, 0.0)),
                        PropValue::Unsigned(1),
                        PropValue::Float(0.0),
                    ],
                ),
                (
                    key(4, Segment::Linear),
                    vec![
                        PropValue::Vector(Vector3::new(100.0, -40.0, 8.0)),
                        PropValue::Unsigned(9),
                        PropValue::Float(1.0),
                    ],
                ),
            ],
        )]),
    };
    let (mut host, owner) = setup(data);
    play(&mut host, owner, 0, 0, 4, false);

    insta::assert_snapshot!(run(
        &mut host,
        owner,
        &[0.0, 16.666_666, 16.666_666, 16.666_666, 16.666_666]
    ));
}

#[test]
fn colors_lerp_per_channel() {
    let color = def(&xui::XUI_TEXT, "TextColor");
    let data = TimelineData {
        named_frames: Box::new([]),
        tracks: Box::new([track(
            "box",
            &[color],
            vec![
                (
                    key(0, Segment::Linear),
                    vec![PropValue::Color(Color::from_argb(0x00ff_0000))],
                ),
                (
                    key(3, Segment::Linear),
                    vec![PropValue::Color(Color::from_argb(0xff00_00ff))],
                ),
            ],
        )]),
    };
    let (mut host, owner) = setup(data);
    play(&mut host, owner, 0, 0, 3, false);

    insta::assert_snapshot!(run(
        &mut host,
        owner,
        &[0.0, 16.666_666, 16.666_666, 16.666_666]
    ));
}

#[test]
fn removing_the_current_timeline_during_a_tick_skips_nothing() {
    let mut timelines = Timelines::new();
    let registry = Registry::new();
    let class = registry
        .find("XuiElement")
        .and_then(|id| registry.class(id))
        .unwrap();
    let mut tree = ElementTree::new();
    let ids: Vec<ElementId> = (0..3).map(|_| tree.create(class)).collect();
    for id in &ids {
        let data = TimelineData {
            named_frames: Box::new([]),
            tracks: Box::new([]),
        };
        timelines.create(*id, Rc::new(data), |_| None);
    }

    timelines.cursor = Some(1);
    timelines.remove(ids[0]);
    assert_eq!(timelines.cursor, Some(0));
    assert_eq!(timelines.active, vec![ids[1], ids[2]]);
}

#[test]
fn apply_at_frame_wraps_a_full_range_loop() {
    let data = TimelineData {
        named_frames: Box::new([]),
        tracks: Box::new([fade()]),
    };
    let (mut host, owner) = setup(data);
    // end - start + 1 does not fit in 32 bits and wraps to i32::MIN.
    play(&mut host, owner, 0, 0, i32::MAX, true);
    host.log.clear();

    apply_at_frame(&mut host, owner, Frame(5));

    assert_eq!(host.log, ["  set box.Opacity=0.5000"]);
}

use std::any::Any;
use std::boxed::Box;
use std::cell::Cell;
use std::cell::RefCell;
use std::format;
use std::rc::Rc;
use std::string::String;
use std::vec;
use std::vec::Vec;

use exui_core::Document;
use exui_core::Registry;
use exui_core::schema::ClassDef;
use exui_core::schema::xui;

use crate::control::ClassState;
use crate::input::KeyKind;
use crate::input::Keystroke;
use crate::input::UserIndex;
use crate::input::VirtualKey;
use crate::message::Body;
use crate::message::ClassBehavior;
use crate::message::Context;
use crate::message::FocusUser;
use crate::message::KeyFlags;
use crate::message::Message;
use crate::message::NotifyKind;
use crate::message::Runtime;
use crate::message::TimerId;
use crate::message::TraceEvent;
use crate::resource::Resources;
use crate::scene::ElementId;
use crate::scene::ElementTree;
use crate::test_support::MapProvider;
use crate::test_support::Obj;
use crate::test_support::V;
use crate::test_support::build_scene;
use crate::time::Milliseconds;
use crate::timeline::FrameCommand;
use crate::timeline::Key;
use crate::timeline::NamedFrame;
use crate::timeline::PropertyPath;
use crate::timeline::Segment;
use crate::timeline::TimelineData;
use crate::timeline::TrackData;

pub(crate) static TEST_SCENE: ClassDef = ClassDef::new("TestScene", Some("XuiScene"), &[]);
pub(crate) static TEST_BUTTON: ClassDef = ClassDef::new("TestButton", Some("XuiButton"), &[]);

pub(crate) type Log = Rc<RefCell<Vec<String>>>;

/// Logs the interesting messages an application class sees, without
/// handling them.
struct Probe {
    log: Log,
}

impl ClassBehavior for Probe {
    fn on_message(&self, cx: &mut Context<'_, '_>, element: ElementId, message: &mut Message) {
        let name = name(cx.tree, element);
        let text = match &message.body {
            Body::KeyDown(key) => format!("0x24 key {:#x}", key.key.raw()),
            Body::Press { .. } => String::from("0x7e5 press"),
            Body::StartTransition { kind, action, .. } => format!("0x1d start {kind:?} {action:?}"),
            Body::TransitionEnded { kind, action } => format!("0x1e ended {kind:?} {action:?}"),
            Body::NavigatedBack { from } => format!("0x1a back from {}", name_of(cx.tree, *from)),
            Body::SetFocus { previous, .. } => {
                format!("0xc set focus (previous {})", opt_name(cx.tree, *previous))
            }
            Body::KillFocus { new, .. } => {
                format!("0xd kill focus (new {})", opt_name(cx.tree, *new))
            }
            Body::TimelineEnd { owner } => format!("0xf end of {}", name_of(cx.tree, *owner)),
            Body::Notify(notify) => {
                let source = name_of(cx.tree, notify.source);
                match &notify.kind {
                    NotifyKind::Press { .. } => format!("0xe press from {source}"),
                    NotifyKind::Pressing { repeat, .. } => {
                        format!("0xe pressing from {source} repeat {repeat}")
                    }
                    NotifyKind::SetFocus { .. } => format!("0xe set focus from {source}"),
                    NotifyKind::KillFocus { .. } => format!("0xe kill focus from {source}"),
                    other => format!("0xe code {} from {source}", other.code().0),
                }
            }
            _ => return,
        };
        self.log.borrow_mut().push(format!("{name}: {text}"));
    }
}

pub(crate) fn name_of(tree: &ElementTree, element: ElementId) -> String {
    tree.get(element)
        .and_then(|element| element.id())
        .map_or_else(|| String::from("?"), String::from_utf16_lossy)
}

fn name(tree: &ElementTree, element: ElementId) -> String {
    name_of(tree, element)
}

pub(crate) fn opt_name(tree: &ElementTree, element: Option<ElementId>) -> String {
    element.map_or_else(|| String::from("none"), |element| name_of(tree, element))
}

/// Registry with the probe classes registered after the XUI built-ins.
pub(crate) fn test_registry() -> Registry {
    let mut registry = Registry::new();
    registry.register(&TEST_SCENE).unwrap();
    registry.register(&TEST_BUTTON).unwrap();
    registry
}

pub(crate) struct World<'p> {
    pub tree: ElementTree,
    pub registry: &'p Registry,
    pub resources: Resources<'p>,
    pub runtime: Runtime<'p>,
    pub root: ElementId,
    pub log: Log,
}

impl<'p> World<'p> {
    pub fn new(registry: &'p Registry, provider: &'p MapProvider) -> Self {
        let log: Log = Rc::new(RefCell::new(Vec::new()));
        let mut runtime = Runtime::new(registry);
        for class in ["TestScene", "TestButton"] {
            runtime
                .register_behavior(
                    registry,
                    class,
                    Rc::new(Probe {
                        log: Rc::clone(&log),
                    }),
                )
                .unwrap();
        }
        let trace_log = Rc::clone(&log);
        runtime.set_trace(Some(Box::new(move |event| match event {
            TraceEvent::Play { start, end, .. } => {
                trace_log
                    .borrow_mut()
                    .push(format!("play {}..{}", start.0, end.frame().0));
            }
        })));

        let mut tree = ElementTree::new();
        let canvas = registry
            .find("XuiCanvas")
            .and_then(|id| registry.class(id))
            .unwrap();
        let root = tree.create(canvas);
        tree.set_bounds(root, 640.0, 480.0);

        Self {
            tree,
            registry,
            resources: Resources::new(provider),
            runtime,
            root,
            log,
        }
    }

    pub fn cx(&mut self) -> Context<'_, 'p> {
        Context::new(
            &mut self.tree,
            self.registry,
            &self.resources,
            &mut self.runtime,
        )
    }

    /// Instantiates `scene` under the root canvas. `timelines` are attached
    /// by element Id before the init pass.
    pub fn load(&mut self, scene: &Obj, timelines: Vec<(&str, TimelineData)>) -> ElementId {
        let bytes = build_scene(self.registry, scene);
        let document = Document::parse(&bytes, self.registry).unwrap();
        let root = self.root;
        let mut cx = self.cx();
        let element = cx.instantiate_document(&document, None).unwrap();
        cx.runtime.load_depth += 1;
        cx.insert_child(root, element, None).unwrap();
        cx.runtime.load_depth -= 1;

        for (id, data) in timelines {
            let target = find(&cx, element, id);
            let tree = &*cx.tree;
            cx.runtime.timelines.create(target, Rc::new(data), |child| {
                child.and_then(|child| tree.child_by_id(target, child))
            });
        }

        cx.init_element_tree(element);
        element
    }

    pub fn find(&mut self, id: &str) -> ElementId {
        let root = self.root;
        let cx = self.cx();
        find(&cx, root, id)
    }

    pub fn take_log(&mut self) -> Vec<String> {
        core::mem::take(&mut *self.log.borrow_mut())
    }

    pub fn key(&mut self, key: VirtualKey) -> bool {
        self.key_with_flags(key, KeyFlags::NONE)
    }

    pub fn key_with_flags(&mut self, key: VirtualKey, flags: KeyFlags) -> bool {
        let keystroke = Keystroke {
            key: Some(key.into()),
            user: UserIndex::new(0).unwrap(),
            kind: KeyKind::Down,
            character: None,
            flags,
        };
        let handled = self.cx().process_keystroke(&keystroke);
        handled == crate::focus::KeyResult::Handled
    }

    pub fn focused(&self) -> String {
        let user = FocusUser::User(UserIndex::new(0).unwrap());
        opt_name(&self.tree, self.runtime.focus.slot(user))
    }

    pub fn tick(&mut self, frames: u32) {
        for _ in 0..frames {
            self.cx().tick_timelines(crate::timeline::FRAME_MS);
        }
    }
}

/// Depth-first search by Id below `root`.
pub(crate) fn find(cx: &Context<'_, '_>, root: ElementId, id: &str) -> ElementId {
    let wanted: Vec<u16> = id.encode_utf16().collect();
    let mut stack = vec![root];
    while let Some(element) = stack.pop() {
        if cx.tree.get(element).and_then(|element| element.id()) == Some(wanted.as_slice()) {
            return element;
        }
        stack.extend(cx.tree.children(element));
    }

    panic!("no element {id}");
}

pub(crate) fn user0() -> FocusUser {
    FocusUser::User(UserIndex::new(0).unwrap())
}

pub(crate) fn named(frames: &[(&str, u32)]) -> TimelineData {
    let last = frames.iter().map(|(_, frame)| *frame).max().unwrap_or(0);
    TimelineData {
        named_frames: frames
            .iter()
            .map(|(name, frame)| NamedFrame {
                name: name.encode_utf16().collect(),
                frame: *frame,
                command: FrameCommand::None,
            })
            .collect(),
        tracks: Box::new([TrackData {
            target_id: None,
            paths: Box::new([PropertyPath {
                defs: Box::new([crate::test_support::def(&xui::XUI_ELEMENT, "Opacity")]),
                index: None,
            }]),
            keys: Box::new([
                Key {
                    frame: 0,
                    segment: Segment::Linear,
                },
                Key {
                    frame: last,
                    segment: Segment::Linear,
                },
            ]),
            values: Box::new([
                crate::scene::PropValue::Float(1.0),
                crate::scene::PropValue::Float(1.0),
            ]),
        }]),
    }
}

/// Gives a control a XuiVisual first child with state frames.
pub(crate) fn attach_visual(world: &mut World<'_>, control: ElementId, frames: &[(&str, u32)]) {
    let class = world
        .registry
        .find("XuiVisual")
        .and_then(|id| world.registry.class(id))
        .unwrap();
    let visual = world.tree.create(class);
    let first = world.tree.first_child(control);
    world.tree.insert_child(control, visual, first).unwrap();
    world
        .runtime
        .timelines
        .create(visual, Rc::new(named(frames)), |_| None);
    let mut cx = world.cx();
    cx.control_state(control).unwrap().visual_created = true;
}

/// Binds a registry, an empty provider and a [`World`] over them.
macro_rules! world {
    ($world:ident) => {
        let registry = test_registry();
        let provider = MapProvider::default();
        let mut $world = World::new(&registry, &provider);
    };
}
pub(crate) use world;

fn id(value: &'static str) -> V {
    V::Str(value)
}

fn button(name: &'static str) -> Obj {
    Obj::new("TestButton").with(&xui::XUI_ELEMENT, "Id", id(name))
}

const BUTTON_FRAMES: [(&str, u32); 8] = [
    ("Normal", 0),
    ("EndNormal", 1),
    ("Focus", 2),
    ("EndFocus", 3),
    ("Press", 4),
    ("EndPress", 8),
    ("NormalPress", 10),
    ("EndNormalPress", 12),
];

#[test]
fn press_notifies_when_the_press_animation_ends() {
    world!(world);
    let scene = Obj::new("TestScene")
        .with(&xui::XUI_ELEMENT, "Id", id("scene"))
        .child(button("ok"))
        .child(button("other"));
    world.load(&scene, vec![]);
    let ok = world.find("ok");
    attach_visual(&mut world, ok, &BUTTON_FRAMES);

    world.cx().set_focus(Some(ok), user0()).unwrap();
    world.tick(1);
    world.take_log();

    let mut lines = Vec::new();
    lines.push(format!("A handled {}", world.key(VirtualKey::PadA)));
    lines.extend(world.take_log());
    lines.push(format!(
        "B while pressed handled {}",
        world.key(VirtualKey::PadB)
    ));
    lines.extend(world.take_log());
    lines.push(format!(
        "down while pressed handled {}",
        world.key(VirtualKey::PadDpadDown)
    ));
    lines.extend(world.take_log());
    for frame in 0..6 {
        world.tick(1);
        let log = world.take_log();
        if !log.is_empty() {
            lines.push(format!("frame {frame}"));
            lines.extend(log);
        }
    }

    let visual = world.tree.first_child(ok).unwrap();
    let timeline = world.runtime.timelines.get(visual).unwrap();
    lines.push(format!(
        "visual timeline start {} end {} position {:.3}",
        timeline.start().0,
        timeline.end().frame().0,
        timeline.position()
    ));
    insta::assert_snapshot!(lines.join("\n"));
}

#[test]
fn state_frames_fall_back_through_the_table() {
    world!(world);
    let scene = Obj::new("TestScene")
        .with(&xui::XUI_ELEMENT, "Id", id("scene"))
        .child(button("a").with(&xui::XUI_CONTROL, "Enabled", V::Bool(false)))
        .child(button("b"));
    world.load(&scene, vec![]);
    let a = world.find("a");
    let b = world.find("b");
    // No disable or kill focus frames: FocusDisable falls back to Focus,
    // KillFocus to Normal.
    attach_visual(
        &mut world,
        a,
        &[
            ("Normal", 0),
            ("EndNormal", 1),
            ("Focus", 2),
            ("EndFocus", 3),
        ],
    );
    attach_visual(
        &mut world,
        b,
        &[
            ("Normal", 0),
            ("EndNormal", 1),
            ("Focus", 2),
            ("EndFocus", 3),
        ],
    );
    world.take_log();

    world.cx().set_focus(Some(a), user0()).unwrap();
    let after_focus = world.take_log();
    world.cx().set_focus(Some(b), user0()).unwrap();
    let after_move = world.take_log();

    let mut lines = vec![String::from("focus a (disabled)")];
    lines.extend(
        after_focus
            .into_iter()
            .filter(|line| line.starts_with("play")),
    );
    lines.push(String::from("focus b"));
    lines.extend(
        after_move
            .into_iter()
            .filter(|line| line.starts_with("play")),
    );
    insta::assert_snapshot!(lines.join("\n"));
}

fn tab_scene() -> Obj {
    Obj::new("XuiTabScene")
        .with(&xui::XUI_ELEMENT, "Id", id("tabs"))
        .with(&xui::XUI_TAB_SCENE, "TabCount", V::Unsigned(3))
        .with(&xui::XUI_TAB_SCENE, "DefaultTab", V::Unsigned(2))
        .child(Obj::new("XuiElement").with(&xui::XUI_ELEMENT, "Id", id("Tab1")))
        .child(Obj::new("XuiElement").with(&xui::XUI_ELEMENT, "Id", id("Tab2")))
        .child(Obj::new("XuiElement").with(&xui::XUI_ELEMENT, "Id", id("Tab3")))
}

fn tab_report(world: &mut World<'_>, tabs: ElementId) -> String {
    let mut cx = world.cx();
    let current = cx.current_tab(tabs);
    let shown: Vec<String> = ["Tab1", "Tab2", "Tab3"]
        .iter()
        .filter(|name| {
            let element = find(&cx, tabs, name);
            cx.is_shown(element)
        })
        .map(|name| String::from(*name))
        .collect();
    format!("current {current:?} shown {shown:?}")
}

#[test]
fn tab_scene_plays_transition_frames() {
    world!(world);
    let frames = named(&[
        ("1To2", 10),
        ("1To2End", 20),
        ("2To3", 30),
        ("3To2", 50),
        ("3To2End", 60),
        ("2To1", 70),
        ("2To1End", 80),
    ]);
    let tabs = world.load(&tab_scene(), vec![("tabs", frames)]);
    world.cx().set_focus(Some(tabs), user0()).unwrap();
    // The GotoTab snap raises 0xf on the first tick.
    world.tick(1);

    let mut lines = vec![String::from("after init")];
    lines.extend(world.take_log());
    lines.push(tab_report(&mut world, tabs));

    for (label, key, ticks) in [
        (
            "right (2To3 without End frame)",
            VirtualKey::PadDpadRight,
            200u32,
        ),
        (
            "right at the last tab without Wrap",
            VirtualKey::PadDpadRight,
            0,
        ),
        ("left while 2To3 never ends", VirtualKey::PadDpadLeft, 0),
    ] {
        let handled = world.key(key);
        lines.push(format!("{label}: handled {handled}"));
        lines.extend(world.take_log());
        lines.push(tab_report(&mut world, tabs));
        world.tick(ticks);
    }

    insta::assert_snapshot!(lines.join("\n"));
}

#[test]
fn tab_scene_end_frame_clears_animating() {
    world!(world);
    let frames = named(&[("1To2", 10), ("1To2End", 20), ("2To1", 30), ("2To1End", 40)]);
    let tabs = world.load(&tab_scene(), vec![("tabs", frames)]);
    world.cx().set_focus(Some(tabs), user0()).unwrap();

    let mut lines = Vec::new();
    lines.push(format!(
        "left before the snap's first tick: handled {}",
        world.key(VirtualKey::PadLeftShoulder)
    ));
    lines.extend(world.take_log());
    world.tick(1);
    lines.push(format!(
        "left: handled {}",
        world.key(VirtualKey::PadLeftShoulder)
    ));
    lines.extend(world.take_log());
    lines.push(tab_report(&mut world, tabs));
    lines.push(format!(
        "left at tab 0 without Wrap: handled {}",
        world.key(VirtualKey::PadLeftShoulder)
    ));
    lines.push(format!(
        "right while animating: handled {}",
        world.key(VirtualKey::PadRightShoulder)
    ));
    lines.extend(world.take_log());
    world.tick(12);
    lines.push(format!(
        "right after the end frame: handled {}",
        world.key(VirtualKey::PadRightShoulder)
    ));
    lines.extend(world.take_log());
    lines.push(tab_report(&mut world, tabs));

    insta::assert_snapshot!(lines.join("\n"));
}

#[test]
fn checkbox_toggles_at_press_start() {
    world!(world);
    let scene = Obj::new("TestScene")
        .with(&xui::XUI_ELEMENT, "Id", id("scene"))
        .child(Obj::new("XuiCheckbox").with(&xui::XUI_ELEMENT, "Id", id("check")));
    world.load(&scene, vec![]);
    let check = world.find("check");
    attach_visual(
        &mut world,
        check,
        &[
            ("Normal", 0),
            ("EndNormal", 1),
            ("Focus", 2),
            ("EndFocus", 3),
            ("FocusCheck", 4),
            ("EndFocusCheck", 6),
        ],
    );
    world.cx().set_focus(Some(check), user0()).unwrap();
    world.take_log();

    let mut lines = Vec::new();
    world.key(VirtualKey::PadA);
    lines.push(format!(
        "after key down value {}",
        world.cx().control_value(check)
    ));
    lines.extend(world.take_log());
    world.tick(10);
    lines.push(format!(
        "after animation value {}",
        world.cx().control_value(check)
    ));
    lines.extend(world.take_log());

    insta::assert_snapshot!(lines.join("\n"));
}

#[test]
fn timers_fire_once_per_frame_and_restart() {
    world!(world);
    let scene = Obj::new("TestScene").with(&xui::XUI_ELEMENT, "Id", id("scene"));
    let scene = world.load(&scene, vec![]);
    let timer = crate::message::TimerId(7);
    world.runtime.timers.set(scene, timer, Milliseconds(50));

    let mut fired = Vec::new();
    for dt in [20.0f32, 40.0, 200.0, 10.0, 45.0] {
        world.runtime.timers.advance_clock(dt);
        let due = world.runtime.timers.begin_pass(&world.tree);
        let count = due
            .into_iter()
            .filter(|(element, id)| world.runtime.timers.fire_if_due(*element, *id))
            .count();
        fired.push(count);
    }

    assert_eq!(fired, vec![0, 1, 1, 0, 1]);
}

fn press_scene(id_value: &'static str, children: Vec<Obj>) -> Obj {
    let mut scene = Obj::new("TestScene").with(&xui::XUI_ELEMENT, "Id", V::Str(id_value));
    for child in children {
        scene = scene.child(child);
    }
    scene
}

#[test]
fn navigation_runs_transitions_and_blocks_input() {
    let registry = test_registry();
    let second = press_scene(
        "second",
        vec![Obj::new("XuiBackButton").with(&xui::XUI_ELEMENT, "Id", V::Str("back"))],
    );
    // As authored files are: a canvas root holding a non-scene element and
    // the scene. Only the scene survives XuiSceneCreate.
    let second = Obj::new("XuiCanvas")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("file_root"))
        .child(Obj::new("XuiElement").with(&xui::XUI_ELEMENT, "Id", V::Str("decor")))
        .child(second);
    let second = build_scene(&registry, &second);
    let archive = crate::test_support::build_archive(&[("second.xur", &second)]);
    let provider = MapProvider::default().with_section("test", archive);
    let mut world = World::new(&registry, &provider);

    let first = press_scene(
        "first",
        vec![
            Obj::new("XuiNavButton")
                .with(&xui::XUI_ELEMENT, "Id", V::Str("go"))
                .with(&xui::XUI_NAV_BUTTON, "PressPath", V::Str("second.xur")),
        ],
    );
    let frames = named(&[
        ("TransFrom", 0),
        ("EndTransFrom", 6),
        ("TransBackTo", 10),
        ("EndTransBackTo", 14),
    ]);
    let first = world.load(&first, vec![("first", frames)]);
    let base: Vec<u16> = "section://0,test#".encode_utf16().collect();
    let base = world.tree.add_base_path(&base);
    let mut stack = vec![first];
    while let Some(element) = stack.pop() {
        world.tree.set_base_path(element, Some(base)).unwrap();
        stack.extend(world.tree.children(element));
    }

    let go = world.find("go");
    world.cx().set_focus(Some(go), user0()).unwrap();
    world.take_log();

    let mut lines = Vec::new();
    let report = |world: &mut World<'_>, lines: &mut Vec<String>, label: &str| {
        let first_input = world.cx().accepts_input(first);
        let attached = world.tree.parent(first).is_some();
        lines.push(format!(
            "{label}: focus {} first input {first_input} attached {attached}",
            world.focused()
        ));
        lines.extend(world.take_log());
    };

    world.key(VirtualKey::PadA);
    report(&mut world, &mut lines, "press go");
    let second = world.find("second");
    assert_eq!(world.tree.parent(second), world.tree.parent(first));
    assert_eq!(world.cx().back_scene(second), Some(first));
    let root = world.root;
    let cx = world.cx();
    let leftovers = ["file_root", "decor"].iter().any(|id| {
        let wanted: Vec<u16> = id.encode_utf16().collect();
        let mut stack = vec![root];
        let mut found = false;
        while let Some(element) = stack.pop() {
            found |=
                cx.tree.get(element).and_then(|element| element.id()) == Some(wanted.as_slice());
            stack.extend(cx.tree.children(element));
        }
        found
    });
    assert!(!leftovers, "the file root is discarded");
    world.key(VirtualKey::PadDpadDown);
    report(&mut world, &mut lines, "key during transition");
    world.tick(8);
    report(&mut world, &mut lines, "after TransFrom");

    world.key(VirtualKey::PadA);
    report(&mut world, &mut lines, "press back");
    world.tick(6);
    report(&mut world, &mut lines, "after TransBackTo");

    insta::assert_snapshot!(lines.join("\n"));
}

#[test]
fn skin_lookup_prefix_then_plain_and_class_chain() {
    let registry = test_registry();
    let base_skin = build_scene(
        &registry,
        &Obj::new("XuiCanvas")
            // The label inside never sees the blade prefix: this visual is
            // found without it.
            .child(
                Obj::new("XuiVisual")
                    .with(&xui::XUI_ELEMENT, "Id", V::Str("XuiButton"))
                    .child(Obj::new("XuiLabel").with(&xui::XUI_ELEMENT, "Id", V::Str("inner"))),
            )
            .child(Obj::new("XuiVisual").with(&xui::XUI_ELEMENT, "Id", V::Str("XuiScene"))),
    );
    let blade_skin = build_scene(
        &registry,
        &Obj::new("XuiCanvas").child(Obj::new("XuiVisual").with(
            &xui::XUI_ELEMENT,
            "Id",
            V::Str("XuiControl"),
        )),
    );
    let provider = MapProvider::default();
    let mut world = World::new(&registry, &provider);
    let base = world.tree.add_base_path(&[]);
    let prefix: Vec<u16> = "p".encode_utf16().collect();
    world
        .runtime
        .skins
        .load(&registry, &base_skin, base, None)
        .unwrap();
    world
        .runtime
        .skins
        .load(&registry, &blade_skin, base, Some(&prefix))
        .unwrap();
    assert!(
        world
            .runtime
            .skins
            .load(&registry, &base_skin, base, None)
            .is_err()
    );

    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().collect() };
    let skins = &world.runtime.skins;
    let mut lines = Vec::new();
    for name in ["XuiControl", "XuiButton", "XuiNavButton"] {
        let found = skins
            .lookup_scoped(&wide(name), Some(&prefix))
            .map(|(_, prefixed)| prefixed);
        lines.push(format!("{name} with prefix: {found:?}"));
    }

    let scene = Obj::new("XuiScene")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("scene"))
        .child(Obj::new("XuiNavButton").with(&xui::XUI_ELEMENT, "Id", V::Str("nav")))
        .child(Obj::new("XuiLabel").with(&xui::XUI_ELEMENT, "Id", V::Str("label")));
    let scene = world.load(&scene, vec![]);
    world.cx().send_body(
        scene,
        Body::SetSkinPrefix {
            prefix: Some(prefix.clone().into()),
        },
    );
    let root = world.root;
    world.cx().prepare_visuals(root);

    for control in ["nav", "label", "inner"] {
        let element = world.find(control);
        let mut cx = world.cx();
        let Some(visual) = cx.control_visual(element) else {
            let prefix = cx
                .skin_prefix(element)
                .map(|prefix| String::from_utf16_lossy(&prefix));
            lines.push(format!("{control}: no visual, prefix seen {prefix:?}"));
            continue;
        };
        let visual_id = name_of(cx.tree, visual);
        let prefix = cx
            .skin_prefix(visual)
            .map(|prefix| String::from_utf16_lossy(&prefix));
        lines.push(format!(
            "{control}: visual {visual_id} prefix inside {prefix:?}"
        ));
    }

    world.runtime.skins.free(Some(&prefix));
    let found = world
        .runtime
        .skins
        .lookup(&wide("XuiControl"), Some(&prefix))
        .is_some();
    lines.push(format!("after freeing p: XuiControl found {found}"));

    insta::assert_snapshot!(lines.join("\n"));
}

#[test]
fn loading_sends_child_added_for_indexes_and_unfocused_input() {
    world!(world);
    let scene = Obj::new("TestScene")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("scene"))
        .child(button("first"))
        .child(
            Obj::new("XuiBackButton")
                .with(&xui::XUI_ELEMENT, "Id", V::Str("legend_back"))
                .with(&xui::XUI_CONTROL, "UnfocusedInput", V::Bool(true)),
        )
        .child(button("last"));
    let scene = world.load(&scene, vec![]);
    let first = world.find("first");
    let back = world.find("legend_back");
    let last = world.find("last");

    let mut cx = world.cx();
    let indexes: Vec<Option<u32>> = [first, back, last]
        .iter()
        .map(|control| cx.control_index(*control))
        .collect();
    assert_eq!(indexes, vec![Some(0), Some(1), Some(2)]);
    let unfocused = cx.scene_state(scene).unwrap().unfocused_input.clone();
    assert_eq!(unfocused, vec![back]);
}

static TEST_CONTENT: ClassDef = ClassDef::new("TestContent", Some("XuiScene"), &[]);

/// Logs 0x13 for the scene loaded as image content.
struct InitCompleteProbe {
    log: Log,
}

impl ClassBehavior for InitCompleteProbe {
    fn on_message(&self, cx: &mut Context<'_, '_>, element: ElementId, message: &mut Message) {
        if matches!(message.body, Body::InitComplete) {
            self.log
                .borrow_mut()
                .push(format!("{}: 0x13 init complete", name_of(cx.tree, element)));
        }
    }
}

fn image_registry() -> Registry {
    let mut registry = test_registry();
    registry.register(&TEST_CONTENT).unwrap();
    registry
}

/// Section `test` holding `content.xur`: a canvas root around the scene.
fn content_provider(registry: &Registry) -> MapProvider {
    let content = Obj::new("XuiCanvas")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("content_root"))
        .child(Obj::new("TestContent").with(&xui::XUI_ELEMENT, "Id", V::Str("content")));
    let content = build_scene(registry, &content);
    let archive = crate::test_support::build_archive(&[("content.xur", &content)]);
    MapProvider::default().with_section("test", archive)
}

fn image(id_value: &'static str, path: &'static str) -> Obj {
    Obj::new("XuiImage")
        .with(&xui::XUI_ELEMENT, "Id", V::Str(id_value))
        .with(&xui::XUI_IMAGE, "ImagePath", V::Str(path))
}

/// Loads a group holding a shown and a hidden image of `content.xur`, with
/// paths relative to section `test`. Returns the group and both images.
fn load_image_page(world: &mut World<'_>, registry: &Registry) -> [ElementId; 3] {
    let log = Rc::clone(&world.log);
    world
        .runtime
        .register_behavior(registry, "TestContent", Rc::new(InitCompleteProbe { log }))
        .unwrap();

    let page = Obj::new("XuiGroup")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("page"))
        .child(image("shown", "content.xur"))
        .child(image("hidden", "Content.XUR").with(&xui::XUI_ELEMENT, "Show", V::Bool(false)));
    let page = world.load(&page, vec![]);
    let base: Vec<u16> = "section://0,test#".encode_utf16().collect();
    let base = world.tree.add_base_path(&base);
    let mut stack = vec![page];
    while let Some(element) = stack.pop() {
        world.tree.set_base_path(element, Some(base)).unwrap();
        stack.extend(world.tree.children(element));
    }

    let shown = world.find("shown");
    let hidden = world.find("hidden");
    world.take_log();

    [page, shown, hidden]
}

fn child_ids(world: &World<'_>, element: ElementId) -> Vec<String> {
    world
        .tree
        .children(element)
        .map(|child| name_of(&world.tree, child))
        .collect()
}

fn image_path(path: &str) -> crate::scene::PropValue {
    crate::scene::PropValue::String(Some(path.encode_utf16().collect()))
}

#[test]
fn image_scene_path_attaches_the_file_root_once() {
    let registry = image_registry();
    let provider = content_provider(&registry);
    let mut world = World::new(&registry, &provider);
    let [page, shown, hidden] = load_image_page(&mut world, &registry);
    let root = world.root;

    world.cx().prepare_visuals(root);
    assert_eq!(child_ids(&world, shown), vec!["content_root"]);
    assert!(
        child_ids(&world, hidden).is_empty(),
        "the prepare pass skips hidden images"
    );
    assert_eq!(world.take_log(), vec!["content: 0x13 init complete"]);
    let attached = world.tree.first_child(shown);

    world.cx().broadcast(page, &Body::CreateResources);
    world.cx().prepare_visuals(root);
    assert_eq!(world.tree.first_child(shown), attached);
    assert_eq!(child_ids(&world, shown), vec!["content_root"]);
    assert_eq!(child_ids(&world, hidden), vec!["content_root"]);
    assert_eq!(world.take_log(), vec!["content: 0x13 init complete"]);
}

#[test]
fn image_path_change_releases_the_scene_content() {
    let registry = image_registry();
    let provider = content_provider(&registry);
    let mut world = World::new(&registry, &provider);
    let [page, shown, hidden] = load_image_page(&mut world, &registry);
    let root = world.root;
    world.cx().broadcast(page, &Body::CreateResources);
    world.take_log();

    let attached = world.tree.first_child(shown).unwrap();
    let path = [crate::control::props::xui_image_path()];
    world
        .cx()
        .set_property(shown, &path, None, image_path("tex.png"))
        .unwrap();
    assert!(child_ids(&world, shown).is_empty());
    assert!(
        !world.tree.contains(attached),
        "the setter destroys the content"
    );
    world.cx().prepare_visuals(root);
    assert!(
        child_ids(&world, shown).is_empty(),
        "a texture path loads no scene"
    );

    // A store without a context leaves the content until the next load,
    // which destroys it and loads the path again.
    let previous = world.tree.first_child(hidden).unwrap();
    world
        .tree
        .set_property(hidden, &path, None, image_path("content.xur"))
        .unwrap();
    assert!(world.tree.contains(previous));
    world.cx().broadcast(hidden, &Body::CreateResources);
    assert!(!world.tree.contains(previous));
    assert_eq!(child_ids(&world, hidden), vec!["content_root"]);
    assert_eq!(world.take_log(), vec!["content: 0x13 init complete"]);
}

/// Section `test` holding `content.xur`, a 100x50 canvas, and `tex.png`.
fn sized_content_provider(registry: &Registry) -> MapProvider {
    let content = Obj::new("XuiCanvas")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("content_root"))
        .with(&xui::XUI_ELEMENT, "Width", V::Float(100.0))
        .with(&xui::XUI_ELEMENT, "Height", V::Float(50.0));
    let content = build_scene(registry, &content);
    let archive = crate::test_support::build_archive(&[("content.xur", &content)]);
    MapProvider::default().with_section("test", archive)
}

/// Sets the base path of `root` and its descendants to section `test`.
fn set_test_base_path(world: &mut World<'_>, root: ElementId) {
    let base: Vec<u16> = "section://0,test#".encode_utf16().collect();
    let base = world.tree.add_base_path(&base);
    let mut stack = vec![root];
    while let Some(element) = stack.pop() {
        world.tree.set_base_path(element, Some(base)).unwrap();
        stack.extend(world.tree.children(element));
    }
}

/// Lays out `content.xur` in an image of `width` by `height` with
/// `size_mode`. Returns the image size, the content size and the content
/// position and scale.
fn layout_with_size_mode(
    size_mode: u32,
    width: f32,
    height: f32,
) -> (crate::scene::Size, crate::scene::Size, [f32; 4]) {
    let registry = test_registry();
    let provider = sized_content_provider(&registry);
    let mut world = World::new(&registry, &provider);
    let image = image("image", "content.xur")
        .with(&xui::XUI_ELEMENT, "Width", V::Float(width))
        .with(&xui::XUI_ELEMENT, "Height", V::Float(height))
        .with(&xui::XUI_IMAGE, "SizeMode", V::Unsigned(size_mode));
    let image = world.load(&image, vec![]);
    set_test_base_path(&mut world, image);
    let root = world.root;
    world.cx().prepare_visuals(root);

    let content = world.tree.first_child(image).unwrap();
    let image = world.tree.get(image).unwrap();
    let content = world.tree.get(content).unwrap();
    let position = content.position();
    let scale = content.scale();
    (
        image.size(),
        content.size(),
        [position.x, position.y, scale.x, scale.y],
    )
}

#[test]
fn image_scene_content_is_placed_by_size_mode() {
    use crate::scene::Size;
    use crate::scene::image::SizeMode;

    let square = Size {
        width: 200.0,
        height: 200.0,
    };
    let flat = Size {
        width: 200.0,
        height: 50.0,
    };
    let content = Size {
        width: 100.0,
        height: 50.0,
    };
    let cases = [
        (SizeMode::NONE, square, square, [0.0, 0.0, 1.0, 1.0]),
        (
            SizeMode::RESIZE_TO_TEXTURE,
            square,
            content,
            [0.0, 0.0, 1.0, 1.0],
        ),
        (SizeMode::CENTER, square, square, [50.0, 75.0, 1.0, 1.0]),
        (SizeMode::STRETCH, square, square, [0.0, 0.0, 2.0, 4.0]),
        (SizeMode::FIT, square, square, [0.0, 0.0, 2.0, 2.0]),
        (SizeMode::FIT, flat, flat, [0.0, 0.0, 1.0, 1.0]),
        (
            SizeMode::FIT_CENTERED,
            square,
            square,
            [0.0, 50.0, 2.0, 2.0],
        ),
        (SizeMode::FIT_CENTERED, flat, flat, [50.0, 0.0, 1.0, 1.0]),
    ];
    for (mode, size, image_size, placement) in cases {
        assert_eq!(
            layout_with_size_mode(mode.0, size.width, size.height),
            (image_size, content, placement),
            "SizeMode {:#x} in {size:?}",
            mode.0
        );
    }
}

static TEST_IMAGE_OWNER: ClassDef = ClassDef::new("TestImageOwner", Some("XuiControl"), &[]);

/// Answers the source image query of its presenter with a shared path.
struct ImageOwner {
    path: Rc<RefCell<&'static str>>,
}

impl ClassBehavior for ImageOwner {
    fn on_message(&self, _cx: &mut Context<'_, '_>, _element: ElementId, message: &mut Message) {
        if let Body::GetSourceImage(query) = &mut message.body {
            let path: Vec<u16> = self.path.borrow().encode_utf16().collect();
            query.path = Some(Rc::from(path));
            message.handled = true;
        }
    }
}

#[test]
fn presenter_scene_path_attaches_content_until_the_path_changes() {
    let mut registry = test_registry();
    registry.register(&TEST_IMAGE_OWNER).unwrap();
    let provider = sized_content_provider(&registry);
    let mut world = World::new(&registry, &provider);
    let path = Rc::new(RefCell::new("content.xur"));
    world
        .runtime
        .register_behavior(
            &registry,
            "TestImageOwner",
            Rc::new(ImageOwner {
                path: Rc::clone(&path),
            }),
        )
        .unwrap();

    // The presenter path is relative to the base path of the owner's scene.
    let scene =
        Obj::new("TestScene").child(Obj::new("TestImageOwner").child(
            Obj::new("XuiImagePresenter").with(&xui::XUI_ELEMENT, "Id", V::Str("presenter")),
        ));
    let scene = world.load(&scene, vec![]);
    set_test_base_path(&mut world, scene);
    let presenter = world.find("presenter");
    let root = world.root;
    let mut source = crate::control::RuntimePresenterSource::new();

    world.cx().pull_presenters(root, &mut source);
    assert_eq!(child_ids(&world, presenter), vec!["content_root"]);
    assert!(
        !source.images.contains_key(&presenter),
        "scene content is not drawn as an image"
    );
    let attached = world.tree.first_child(presenter).unwrap();

    world.cx().pull_presenters(root, &mut source);
    assert_eq!(
        world.tree.first_child(presenter),
        Some(attached),
        "an unchanged path keeps the content"
    );

    *path.borrow_mut() = "tex.png";
    world.cx().pull_presenters(root, &mut source);
    assert!(child_ids(&world, presenter).is_empty());
    assert!(!world.tree.contains(attached));
    assert!(source.images.contains_key(&presenter));
}

static TEST_LIFE: ClassDef = ClassDef::new("TestLife", Some("XuiElement"), &[]);

/// Logs 0xa and message box results. On timer 1 it destroys
/// `destroy_on_timer`. The Destroy handler of `spawn_on_destroy` creates a
/// child with state and a timer, stored in `spawned`.
#[derive(Default)]
struct Lifecycle {
    log: Log,
    watched: Cell<Option<ElementId>>,
    destroy_on_timer: Cell<Option<ElementId>>,
    spawn_on_destroy: Cell<Option<ElementId>>,
    spawned: Cell<Option<ElementId>>,
}

impl ClassBehavior for Lifecycle {
    fn create_instance(&self) -> Option<Box<dyn Any>> {
        Some(Box::new(0u32))
    }

    fn on_message(&self, cx: &mut Context<'_, '_>, element: ElementId, message: &mut Message) {
        let name = name_of(cx.tree, element);
        match &message.body {
            Body::Destroy => {
                self.log.borrow_mut().push(format!("{name}: 0xa destroy"));
                if self.spawn_on_destroy.get() != Some(element) {
                    return;
                }

                let class = cx
                    .registry
                    .find("TestLife")
                    .and_then(|class| cx.registry.class(class))
                    .unwrap();
                let child = cx.tree.create(class);
                cx.insert_child(element, child, None).unwrap();
                assert!(cx.state_mut(child).is_some());
                cx.runtime.timers.set(child, TimerId(2), Milliseconds(0));
                self.spawned.set(Some(child));
            }
            Body::Timer { id } if *id == TimerId(1) => {
                let Some(target) = self.destroy_on_timer.get() else {
                    return;
                };

                cx.destroy(target);
                let line = format!(
                    "{name}: destroyed {}, parent {}, allocated {}",
                    name_of(cx.tree, target),
                    opt_name(cx.tree, cx.tree.parent(target)),
                    cx.tree.contains(target)
                );
                self.log.borrow_mut().push(line);
            }
            Body::MessageBoxResult { button } => {
                let parent = self
                    .watched
                    .get()
                    .and_then(|watched| cx.tree.parent(watched));
                let line = format!(
                    "{name}: 0x7ec result {button:?}, box parent {}",
                    opt_name(cx.tree, parent)
                );
                self.log.borrow_mut().push(line);
            }
            _ => {}
        }
    }
}

fn life_registry() -> Registry {
    let mut registry = test_registry();
    registry.register(&TEST_LIFE).unwrap();
    registry
}

fn register_life(world: &mut World<'_>) -> Rc<Lifecycle> {
    let life = Rc::new(Lifecycle {
        log: Rc::clone(&world.log),
        ..Lifecycle::default()
    });
    let behavior: Rc<dyn ClassBehavior> = Rc::<Lifecycle>::clone(&life);
    world
        .runtime
        .register_behavior(world.registry, "TestLife", behavior)
        .unwrap();

    life
}

fn life(id_value: &'static str) -> Obj {
    Obj::new("TestLife").with(&xui::XUI_ELEMENT, "Id", V::Str(id_value))
}

#[test]
fn destroy_frees_after_a_descendant_leaves_dispatch() {
    let registry = life_registry();
    let provider = MapProvider::default();
    let mut world = World::new(&registry, &provider);
    let life_behavior = register_life(&mut world);
    let outer = world.load(&life("outer").child(life("inner")), vec![]);
    let inner = world.find("inner");
    life_behavior.destroy_on_timer.set(Some(outer));
    world.take_log();

    world.cx().send_body(inner, Body::Timer { id: TimerId(1) });

    assert_eq!(
        world.take_log(),
        vec![
            "outer: 0xa destroy",
            "inner: 0xa destroy",
            "inner: destroyed outer, parent none, allocated true",
        ]
    );
    assert!(!world.tree.contains(outer));
    assert!(!world.tree.contains(inner));
    assert!(!world.runtime.states.contains_key(&inner));
}

#[test]
fn destroy_frees_children_created_by_destroy_handlers() {
    let registry = life_registry();
    let provider = MapProvider::default();
    let mut world = World::new(&registry, &provider);
    let life_behavior = register_life(&mut world);
    let maker = world.load(&life("maker"), vec![]);
    life_behavior.spawn_on_destroy.set(Some(maker));
    world.take_log();

    world.cx().destroy(maker);

    let spawned = life_behavior.spawned.get().unwrap();
    assert_eq!(
        world.take_log(),
        vec!["maker: 0xa destroy", "?: 0xa destroy"]
    );
    assert!(!world.tree.contains(spawned));
    assert!(!world.runtime.states.contains_key(&spawned));
    assert!(!world.runtime.timers.kill(spawned, TimerId(2)));
}

#[test]
fn freed_elements_get_no_instance_or_timer() {
    let registry = life_registry();
    let provider = MapProvider::default();
    let mut world = World::new(&registry, &provider);
    register_life(&mut world);
    let gone = world.load(&life("gone"), vec![]);
    let class = registry.find("TestLife").unwrap();
    world.cx().destroy(gone);

    assert!(world.cx().instance_mut::<u32>(gone, class).is_none());

    world.runtime.timers.set(gone, TimerId(3), Milliseconds(0));
    world.cx().process_timers();
    assert!(!world.runtime.timers.kill(gone, TimerId(3)));
}

#[test]
fn message_box_is_destroyed_before_the_result_is_sent() {
    let registry = life_registry();
    let provider = MapProvider::default();
    let mut world = World::new(&registry, &provider);
    let life_behavior = register_life(&mut world);
    let page = Obj::new("XuiGroup")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("page"))
        .child(life("owner"))
        .child(
            Obj::new("XuiMessageBox")
                .with(&xui::XUI_ELEMENT, "Id", V::Str("box"))
                .child(life("Button1")),
        );
    world.load(&page, vec![]);
    let owner = world.find("owner");
    let message_box = world.find("box");
    let button = world.find("Button1");
    if let Some(ClassState::MessageBox(state)) = world.cx().class_state(message_box) {
        state.owner = Some(owner);
    }
    life_behavior.watched.set(Some(message_box));
    world.take_log();

    world.cx().send_body(
        message_box,
        Body::Notify(crate::message::Notify {
            kind: NotifyKind::Press {
                user: FocusUser::Shared,
            },
            source: button,
        }),
    );

    assert_eq!(
        world.take_log(),
        vec![
            "Button1: 0xa destroy",
            "owner: 0x7ec result Some(1), box parent none",
        ]
    );
    assert!(!world.tree.contains(message_box));
}

fn source_text(world: &mut World<'_>, element: ElementId) -> Option<Rc<[u16]>> {
    let query = crate::message::SourceTextQuery {
        item: 0,
        association: 0,
        item_data: false,
        text: None,
        point_size: None,
        extended: true,
        y_offset: 0.0,
        style: 0,
    };
    match world
        .cx()
        .send_body(element, Body::GetSourceText(query))
        .body
    {
        Body::GetSourceText(query) => query.text,
        _ => None,
    }
}

#[test]
fn control_text_answer_is_shared_until_the_text_changes() {
    world!(world);
    let button = Obj::new("XuiButton")
        .with(&xui::XUI_ELEMENT, "Id", id("button"))
        .with(&xui::XUI_CONTROL, "Text", id("first"));
    let button = world.load(&button, vec![]);

    let first = source_text(&mut world, button).unwrap();
    let again = source_text(&mut world, button).unwrap();
    assert!(
        Rc::ptr_eq(&first, &again),
        "an unchanged text is not copied"
    );

    let text: Vec<u16> = "second".encode_utf16().collect();
    world.cx().send_body(
        button,
        Body::SetSourceText {
            item: 0,
            association: 0,
            item_data: false,
            text: Some(text.as_slice().into()),
        },
    );
    let changed = source_text(&mut world, button).unwrap();
    assert_eq!(&*changed, text.as_slice());
}

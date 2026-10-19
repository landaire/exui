use std::boxed::Box;
use std::format;
use std::string::String;
use std::vec;
use std::vec::Vec;

use exui_core::schema::xui;

use crate::control::tests::World;
use crate::control::tests::test_registry;
use crate::control::tests::user0;
use crate::control::tests::world;
use crate::input::VirtualKey;
use crate::message::KeyFlags;
use crate::scene::PropValue;
use crate::test_support::MapProvider;
use crate::test_support::Obj;
use crate::test_support::V;
use crate::test_support::def;
use crate::timeline::Frame;
use crate::timeline::Key;
use crate::timeline::PlayEnd;
use crate::timeline::PlayRange;
use crate::timeline::PropertyPath;
use crate::timeline::Segment;
use crate::timeline::TimelineData;
use crate::timeline::TrackData;

fn button(id: &'static str) -> Obj {
    Obj::new("TestButton").with(&xui::XUI_ELEMENT, "Id", V::Str(id))
}

fn nav(object: Obj, direction: &str, path: &'static str) -> Obj {
    object.with(&xui::XUI_CONTROL, direction, V::Str(path))
}

/// Scene used by the navigation tests:
/// a -> down b (hidden) -> down c; c down names itself; c up d (disabled);
/// group g holds e whose right path climbs out with `..`.
fn nav_scene() -> Obj {
    Obj::new("TestScene")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("scene"))
        .child(nav(nav(button("a"), "NavDown", "b"), "NavRight", "g\\e"))
        .child(nav(button("b"), "NavDown", "c").with(&xui::XUI_ELEMENT, "Show", V::Bool(false)))
        .child(nav(nav(button("c"), "NavDown", "c"), "NavUp", "d"))
        .child(button("d").with(&xui::XUI_CONTROL, "Enabled", V::Bool(false)))
        .child(
            Obj::new("XuiElement")
                .with(&xui::XUI_ELEMENT, "Id", V::Str("g"))
                .child(nav(button("e"), "NavRight", "..\\a")),
        )
}

#[test]
fn explicit_navigation_paths() {
    world!(world);
    world.load(&nav_scene(), vec![]);
    let a = world.find("a");
    world.cx().set_focus(Some(a), user0()).unwrap();
    world.take_log();

    let mut lines = vec![format!("start {}", world.focused())];
    for (label, key) in [
        (
            "down (b hidden, follows b.NavDown)",
            VirtualKey::PadDpadDown,
        ),
        ("down (c.NavDown names c)", VirtualKey::PadDpadDown),
        ("up (d disabled)", VirtualKey::PadDpadUp),
        ("left (no NavLeft)", VirtualKey::PadDpadLeft),
    ] {
        let handled = world.key(key);
        lines.push(format!(
            "{label}: handled {handled} focus {}",
            world.focused()
        ));
    }

    let a = world.find("a");
    world.cx().set_focus(Some(a), user0()).unwrap();
    let handled = world.key(VirtualKey::PadDpadRight);
    lines.push(format!(
        "right from a into g\\e: handled {handled} focus {}",
        world.focused()
    ));
    let handled = world.key(VirtualKey::PadDpadRight);
    lines.push(format!(
        "right from e via ..\\a: handled {handled} focus {}",
        world.focused()
    ));

    insta::assert_snapshot!(lines.join("\n"));
}

#[test]
fn set_focus_sends_kill_then_set_even_for_the_same_element() {
    world!(world);
    world.load(&nav_scene(), vec![]);
    let a = world.find("a");
    let c = world.find("c");

    world.cx().set_focus(Some(a), user0()).unwrap();
    let mut lines = vec![String::from("focus a")];
    lines.extend(world.take_log());
    world.cx().set_focus(Some(a), user0()).unwrap();
    lines.push(String::from("focus a again"));
    lines.extend(world.take_log());
    world.cx().set_focus(Some(c), user0()).unwrap();
    lines.push(String::from("focus c"));
    lines.extend(world.take_log());

    insta::assert_snapshot!(lines.join("\n"));
}

#[test]
fn initial_focus_skips_hidden_controls_but_descends_plain_elements() {
    world!(world);
    let scene = Obj::new("TestScene")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("scene"))
        .child(
            Obj::new("XuiElement")
                .with(&xui::XUI_ELEMENT, "Id", V::Str("plain"))
                .child(
                    Obj::new("XuiScene")
                        .with(&xui::XUI_ELEMENT, "Id", V::Str("hidden"))
                        .with(&xui::XUI_ELEMENT, "Show", V::Bool(false))
                        .child(button("inside_hidden")),
                )
                .child(button("target")),
        );
    let scene = world.load(&scene, vec![]);

    let result = world.cx().init_focus(scene, user0(), true);
    assert!(result.is_ok());
    assert_eq!(world.focused(), "target");
}

#[test]
fn hiding_the_focused_control_moves_focus_to_its_parent() {
    world!(world);
    world.load(&nav_scene(), vec![]);
    let e = world.find("e");
    world.cx().set_focus(Some(e), user0()).unwrap();

    // The parent g is a plain element that does not answer the focus
    // query, so set focus fails before touching the slot and e keeps it.
    world.cx().set_show(e, false);
    assert_eq!(world.focused(), "e");

    // The scene takes focus and forwards it: its remembered child a is
    // hidden, so it runs initial focus over its children (b is hidden).
    let a = world.find("a");
    world.cx().set_focus(Some(a), user0()).unwrap();
    world.cx().set_show(a, false);
    assert_eq!(world.focused(), "c");
}

#[test]
fn hiding_an_ancestor_of_the_focused_control_keeps_focus() {
    world!(world);
    let scene = Obj::new("TestScene")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("scene"))
        .child(
            Obj::new("XuiScene")
                .with(&xui::XUI_ELEMENT, "Id", V::Str("inner"))
                .child(button("x")),
        );
    world.load(&scene, vec![]);
    let x = world.find("x");
    let inner = world.find("inner");
    world.cx().set_focus(Some(x), user0()).unwrap();

    world.cx().set_show(inner, false);
    assert_eq!(world.focused(), "x");
}

/// The boot timeline of the dashboard keys `Show` of the start tab to false
/// and back while a button inside it has focus.
#[test]
fn timeline_show_keys_on_an_ancestor_keep_focus() {
    world!(world);
    let scene = Obj::new("TestScene")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("scene"))
        .child(
            Obj::new("XuiScene")
                .with(&xui::XUI_ELEMENT, "Id", V::Str("tab"))
                .child(button("x")),
        );
    let key = |frame| Key {
        frame,
        segment: Segment::Linear,
    };
    let timeline = TimelineData {
        named_frames: Box::new([]),
        tracks: Box::new([TrackData {
            target_id: Some("tab".encode_utf16().collect()),
            paths: Box::new([PropertyPath {
                defs: Box::new([def(&xui::XUI_ELEMENT, "Show")]),
                index: None,
            }]),
            keys: Box::new([key(0), key(2), key(4)]),
            values: Box::new([
                PropValue::Bool(true),
                PropValue::Bool(false),
                PropValue::Bool(true),
            ]),
        }]),
    };
    let scene = world.load(&scene, vec![("scene", timeline)]);
    let x = world.find("x");
    let tab = world.find("tab");
    world.cx().set_focus(Some(x), user0()).unwrap();
    world.cx().play_timeline(
        scene,
        PlayRange {
            start: Frame(0),
            initial: Frame(0),
            end: PlayEnd::At(Frame(4)),
            looping: false,
        },
        false,
    );

    let mut lines = Vec::new();
    for _ in 0..6 {
        world.tick(1);
        let shown = world.cx().is_shown(tab);
        lines.push(format!("shown {shown} focus {}", world.focused()));
    }
    assert_eq!(
        lines,
        [
            "shown true focus x",
            "shown true focus x",
            "shown false focus x",
            "shown false focus x",
            "shown true focus x",
            "shown true focus x",
        ]
    );
}

#[test]
fn keys_without_a_focus_target_count_as_handled() {
    world!(world);
    world.load(&nav_scene(), vec![]);
    assert!(world.key(VirtualKey::PadA));

    let a = world.find("a");
    world.cx().set_focus(Some(a), user0()).unwrap();
    let scene = world.find("scene");
    world.cx().set_input_enabled(scene, false);
    assert!(world.key(VirtualKey::PadDpadDown));
    assert_eq!(world.focused(), "a");
}

#[test]
fn tab_order_walks_siblings_and_wraps() {
    world!(world);
    world.load(&nav_scene(), vec![]);
    let c = world.find("c");
    world.cx().set_focus(Some(c), user0()).unwrap();

    let mut order = Vec::new();
    for _ in 0..4 {
        world.key(VirtualKey::Tab);
        order.push(world.focused());
    }

    insta::assert_snapshot!(order.join(" "));
}

#[test]
fn shift_tab_walks_siblings_backward() {
    world!(world);
    world.load(&nav_scene(), vec![]);
    let c = world.find("c");
    world.cx().set_focus(Some(c), user0()).unwrap();

    let mut order = Vec::new();
    for _ in 0..4 {
        world.key_with_flags(VirtualKey::Tab, KeyFlags::SHIFT);
        order.push(world.focused());
    }

    insta::assert_snapshot!(order.join(" "));
}

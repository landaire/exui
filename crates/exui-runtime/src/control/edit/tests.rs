//! A multi-line edit below a derived class that takes every timer it sees,
//! and an editable single-line edit.

use std::cell::RefCell;
use std::rc::Rc;
use std::string::String;
use std::vec;
use std::vec::Vec;

use exui_core::schema::ClassDef;
use exui_core::schema::xui;

use crate::control::edit::CharRange;
use crate::control::edit::parse_allowed_chars;
use crate::control::props;
use crate::control::tests::World;
use crate::control::tests::test_registry;
use crate::input::UserIndex;
use crate::message::Body;
use crate::message::CharData;
use crate::message::ClassBehavior;
use crate::message::Context;
use crate::message::EditLineInfoKind;
use crate::message::KeyFlags;
use crate::message::Message;
use crate::message::SourceTextQuery;
use crate::message::TimerId;
use crate::scene::ElementId;
use crate::scene::PropValue;
use crate::test_support::MapProvider;
use crate::test_support::Obj;
use crate::test_support::V;
use crate::time::Milliseconds;

static TEST_EDIT: ClassDef = ClassDef::new("TestEdit", Some("XuiEdit"), &[]);

/// Handles every timer of the derived level.
struct TimerSink {
    seen: Rc<RefCell<Vec<TimerId>>>,
}

impl ClassBehavior for TimerSink {
    fn on_message(&self, _cx: &mut Context<'_, '_>, _element: ElementId, message: &mut Message) {
        if let Body::Timer { id } = message.body {
            self.seen.borrow_mut().push(id);
            message.handled = true;
        }
    }
}

fn edit_scene() -> Obj {
    let text = Obj::new("XuiTextPresenter")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("Text"))
        .with(&xui::XUI_ELEMENT, "Width", V::Float(200.0))
        .with(&xui::XUI_ELEMENT, "Height", V::Float(50.0))
        .with(&xui::XUI_TEXT_PRESENTER, "PointSize", V::Float(15.0));
    let visual = Obj::new("XuiVisual")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("visual"))
        .child(text);
    let edit = Obj::new("TestEdit")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("edit"))
        .with(&xui::XUI_EDIT, "ReadOnly", V::Bool(true))
        .with(&xui::XUI_EDIT, "Multiline", V::Bool(true))
        .with(&xui::XUI_EDIT, "SmoothScroll", V::Bool(true))
        .child(visual);

    Obj::new("TestScene")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("scene"))
        .child(edit)
}

fn line_info(world: &mut World<'_>, edit: ElementId, kind: EditLineInfoKind) -> Option<u32> {
    match world
        .cx()
        .send_body(
            edit,
            Body::EditLineInfo {
                kind,
                current: false,
                value: None,
            },
        )
        .body
    {
        Body::EditLineInfo { value, .. } => value,
        _ => None,
    }
}

fn top_line(world: &mut World<'_>, edit: ElementId, current: bool) -> Option<u32> {
    match world
        .cx()
        .send_body(
            edit,
            Body::EditGetTopLine {
                current,
                line: None,
            },
        )
        .body
    {
        Body::EditGetTopLine { line, .. } => line,
        _ => None,
    }
}

/// Text and vertical offset a presenter pulls.
fn shown(world: &mut World<'_>, edit: ElementId) -> (Vec<u16>, f32) {
    let query = SourceTextQuery {
        item: 0,
        association: 0,
        item_data: false,
        text: None,
        point_size: None,
        extended: true,
        y_offset: 0.0,
        style: 0,
    };
    match world.cx().send_body(edit, Body::GetSourceText(query)).body {
        Body::GetSourceText(query) => (
            query.text.as_deref().unwrap_or_default().to_vec(),
            query.y_offset,
        ),
        _ => (Vec::new(), 0.0),
    }
}

#[test]
fn smooth_scroll_steps_on_the_edit_level_timer() {
    let mut registry = test_registry();
    registry.register(&TEST_EDIT).unwrap();
    let provider = MapProvider::default();
    let mut world = World::new(&registry, &provider);
    world.runtime.fonts = crate::text::tests::library();
    let seen = Rc::new(RefCell::new(Vec::new()));
    world
        .runtime
        .register_behavior(
            &registry,
            "TestEdit",
            Rc::new(TimerSink {
                seen: Rc::clone(&seen),
            }),
        )
        .unwrap();

    world.load(&edit_scene(), vec![]);
    let edit = world.find("edit");
    let text: Vec<u16> = "I\nI\nI\nI".encode_utf16().collect();
    world.cx().send_body(
        edit,
        Body::SetSourceText {
            item: 0,
            association: 0,
            item_data: false,
            text: Some(text.as_slice().into()),
        },
    );
    assert_eq!(
        line_info(&mut world, edit, EditLineInfoKind::Lines),
        Some(4)
    );

    let derived = TimerId(1);
    world.runtime.timers.set(edit, derived, Milliseconds(100));
    world.cx().send_body(
        edit,
        Body::EditScrollToLine {
            line: 2,
            animate: true,
        },
    );
    // The target moves at once; the view follows the timer.
    assert_eq!(top_line(&mut world, edit, false), Some(2));
    assert_eq!(top_line(&mut world, edit, true), Some(0));

    let mut offsets = Vec::new();
    for _ in 0..200 {
        world.runtime.timers.advance_clock(16.0);
        world.cx().process_timers();
        offsets.push(shown(&mut world, edit).1);
    }

    assert_eq!(top_line(&mut world, edit, true), Some(2));
    assert!(
        offsets.iter().any(|offset| *offset > 0.0),
        "the view moves by fractions of a line"
    );
    let (display, offset) = shown(&mut world, edit);
    assert_eq!(display, &text[4..]);
    assert_eq!(offset, 0.0);
    assert!(
        seen.borrow().iter().all(|id| *id == derived),
        "the derived level sees only its own timer"
    );
    assert!(!seen.borrow().is_empty());
}

/// An editable single-line edit with TextLimit `limit` and AllowedChars
/// `allowed`.
fn input_scene(limit: u32, allowed: &'static str) -> Obj {
    let visual = Obj::new("XuiVisual")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("visual"))
        .child(Obj::new("XuiTextPresenter").with(&xui::XUI_ELEMENT, "Id", V::Str("Text")));
    let edit = Obj::new("XuiEdit")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("edit"))
        .with(&xui::XUI_EDIT, "TextLimit", V::Unsigned(limit))
        .with(&xui::XUI_EDIT, "AllowedChars", V::Str(allowed))
        .child(visual);

    Obj::new("TestScene")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("scene"))
        .child(edit)
}

fn type_text(world: &mut World<'_>, edit: ElementId, text: &str) {
    for character in text.encode_utf16() {
        world.cx().send_body(
            edit,
            Body::Char(CharData {
                character,
                flags: KeyFlags::NONE,
                user: UserIndex::new(0).unwrap(),
            }),
        );
    }
}

fn set_text(world: &mut World<'_>, edit: ElementId, text: &str) {
    let text: Vec<u16> = text.encode_utf16().collect();
    world.cx().send_body(
        edit,
        Body::SetSourceText {
            item: 0,
            association: 0,
            item_data: false,
            text: Some(text.as_slice().into()),
        },
    );
}

/// The buffer (`extended` false) or the display copy.
fn source_text(world: &mut World<'_>, edit: ElementId, extended: bool) -> Option<Rc<[u16]>> {
    let query = SourceTextQuery {
        item: 0,
        association: 0,
        item_data: false,
        text: None,
        point_size: None,
        extended,
        y_offset: 0.0,
        style: 0,
    };
    match world.cx().send_body(edit, Body::GetSourceText(query)).body {
        Body::GetSourceText(query) => query.text,
        _ => None,
    }
}

fn source(world: &mut World<'_>, edit: ElementId, extended: bool) -> Option<String> {
    source_text(world, edit, extended).map(|text| String::from_utf16_lossy(&text))
}

fn set_allowed(world: &mut World<'_>, edit: ElementId, allowed: Option<&str>) {
    let value = PropValue::String(allowed.map(|text| text.encode_utf16().collect()));
    world
        .cx()
        .set_property(edit, &[props::edit_allowed_chars()], None, value)
        .unwrap();
}

#[test]
fn text_limit_bounds_set_and_typed_text() {
    let registry = test_registry();
    let provider = MapProvider::default();
    let mut world = World::new(&registry, &provider);
    world.load(&input_scene(5, "a-z"), vec![]);
    let edit = world.find("edit");

    // Over the limit the edit keeps no text; XuiControl stores it.
    set_text(&mut world, edit, "abcdef");
    assert_eq!(source(&mut world, edit, false), None);

    set_text(&mut world, edit, "abcde");
    type_text(&mut world, edit, "z");
    assert_eq!(source(&mut world, edit, false).as_deref(), Some("abcde"));

    // A lower limit cuts the text but leaves the display copy.
    world
        .cx()
        .set_property(
            edit,
            &[props::edit_text_limit()],
            None,
            PropValue::Unsigned(3),
        )
        .unwrap();
    assert_eq!(source(&mut world, edit, false).as_deref(), Some("abc"));
    assert_eq!(source(&mut world, edit, true).as_deref(), Some("abcde"));
}

#[test]
fn allowed_chars_filter_typed_characters() {
    let registry = test_registry();
    let provider = MapProvider::default();
    let mut world = World::new(&registry, &provider);
    world.load(&input_scene(20, "a-cx-"), vec![]);
    let edit = world.find("edit");

    type_text(&mut world, edit, "adbx-A");
    assert_eq!(source(&mut world, edit, true).as_deref(), Some("abx-"));

    // A malformed string leaves no list and a null property.
    set_allowed(&mut world, edit, Some("-a"));
    assert_eq!(
        world.tree.string(edit, &[props::edit_allowed_chars()]),
        None
    );
    type_text(&mut world, edit, "d");
    assert_eq!(source(&mut world, edit, true).as_deref(), Some("abx-d"));

    // Null keeps the list of the last valid string.
    set_allowed(&mut world, edit, Some("q"));
    set_allowed(&mut world, edit, None);
    type_text(&mut world, edit, "zq");
    assert_eq!(source(&mut world, edit, true).as_deref(), Some("abx-dq"));
}

#[test]
fn allowed_chars_syntax() {
    let range = |low: char, high: char| CharRange {
        low: low as u16,
        high: high as u16,
    };
    let parse = |text: &str| parse_allowed_chars(&text.encode_utf16().collect::<Vec<u16>>());

    assert_eq!(parse("a-c"), Some(vec![range('a', 'c')]));
    assert_eq!(
        parse("ab-"),
        Some(vec![range('a', 'a'), range('b', 'b'), range('-', '-')])
    );
    assert_eq!(parse("-"), Some(vec![range('-', '-')]));
    assert_eq!(parse(""), Some(vec![]));
    assert_eq!(parse("-a"), None);
    assert_eq!(parse("a--b"), None);
    assert_eq!(parse("c-a"), None);
}

#[test]
fn presenters_share_the_display_copy() {
    let registry = test_registry();
    let provider = MapProvider::default();
    let mut world = World::new(&registry, &provider);
    world.load(&input_scene(20, "a-z"), vec![]);
    let edit = world.find("edit");
    set_text(&mut world, edit, "text");

    let first = source_text(&mut world, edit, true).unwrap();
    let second = source_text(&mut world, edit, true).unwrap();
    assert!(Rc::ptr_eq(&first, &second));
}

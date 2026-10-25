//! Multi-line read-only edits scrolled by key and message, and editable
//! single-line edits. The test font is 20 px at PointSize 15: 'I' 8 px,
//! 'O' 16 px, 'H' 14 px, characters outside its cmap 12 px, line pitch
//! 22 px.

use std::cell::RefCell;
use std::rc::Rc;
use std::string::String;
use std::vec;
use std::vec::Vec;

use exui_core::Registry;
use exui_core::schema::ClassDef;
use exui_core::schema::xui;

use crate::control::ClassState;
use crate::control::edit::CharRange;
use crate::control::edit::parse_allowed_chars;
use crate::control::props;
use crate::control::tests::World;
use crate::control::tests::test_registry;
use crate::input::UserIndex;
use crate::input::VirtualKey;
use crate::message::Body;
use crate::message::CharData;
use crate::message::ClassBehavior;
use crate::message::Context;
use crate::message::EditLineInfoKind;
use crate::message::KeyData;
use crate::message::KeyFlags;
use crate::message::Message;
use crate::message::Notify;
use crate::message::NotifyKind;
use crate::message::ScrollEndState;
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

fn registry() -> Registry {
    let mut registry = test_registry();
    registry.register(&TEST_EDIT).unwrap();
    registry
}

fn world<'p>(registry: &'p Registry, provider: &'p MapProvider) -> World<'p> {
    let mut world = World::new(registry, provider);
    world.runtime.fonts = crate::text::tests::library();
    world
}

fn scroll_end(id: &'static str) -> Obj {
    Obj::new("XuiScrollEnd")
        .with(&xui::XUI_ELEMENT, "Id", V::Str(id))
        .with(&xui::XUI_ELEMENT, "Width", V::Float(20.0))
        .with(&xui::XUI_ELEMENT, "Height", V::Float(20.0))
}

/// A read-only multi-line edit of class `class` with a `Text` box
/// `width` by 50 px and the up and down arrows.
fn multi_scene(class: &'static str, smooth: bool, width: f32) -> Obj {
    let text = Obj::new("XuiTextPresenter")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("Text"))
        .with(&xui::XUI_ELEMENT, "Width", V::Float(width))
        .with(&xui::XUI_ELEMENT, "Height", V::Float(50.0))
        .with(&xui::XUI_TEXT_PRESENTER, "PointSize", V::Float(15.0));
    let visual = Obj::new("XuiVisual")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("visual"))
        .child(text)
        .child(scroll_end("ScrollUp"))
        .child(scroll_end("ScrollDown"));
    let edit = Obj::new(class)
        .with(&xui::XUI_ELEMENT, "Id", V::Str("edit"))
        .with(&xui::XUI_EDIT, "ReadOnly", V::Bool(true))
        .with(&xui::XUI_EDIT, "Multiline", V::Bool(true))
        .with(&xui::XUI_EDIT, "SmoothScroll", V::Bool(smooth))
        .child(visual);

    Obj::new("TestScene")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("scene"))
        .child(edit)
}

/// An editable single-line edit with TextLimit `limit`, AllowedChars
/// `allowed`, a `Text` box 100 by 30 px, a caret and the left and right
/// arrows.
fn input_scene(limit: u32, allowed: &'static str) -> Obj {
    let text = Obj::new("XuiTextPresenter")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("Text"))
        .with(&xui::XUI_ELEMENT, "Width", V::Float(100.0))
        .with(&xui::XUI_ELEMENT, "Height", V::Float(30.0))
        .with(&xui::XUI_TEXT_PRESENTER, "PointSize", V::Float(15.0));
    let caret = Obj::new("XuiCaret")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("Caret"))
        .with(&xui::XUI_ELEMENT, "Width", V::Float(2.0))
        .with(&xui::XUI_ELEMENT, "Height", V::Float(22.0));
    let visual = Obj::new("XuiVisual")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("visual"))
        .child(text)
        .child(caret)
        .child(scroll_end("ScrollLeft"))
        .child(scroll_end("ScrollRight"));
    let edit = Obj::new("XuiEdit")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("edit"))
        .with(&xui::XUI_EDIT, "TextLimit", V::Unsigned(limit))
        .with(&xui::XUI_EDIT, "AllowedChars", V::Str(allowed))
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

fn line_start(world: &mut World<'_>, edit: ElementId, line: u32) -> Option<u32> {
    match world
        .cx()
        .send_body(edit, Body::EditGetLineStart { line, start: None })
        .body
    {
        Body::EditGetLineStart { start, .. } => start,
        _ => None,
    }
}

fn caret(world: &mut World<'_>, edit: ElementId) -> Option<u32> {
    match world
        .cx()
        .send_body(edit, Body::EditGetCaret { position: None })
        .body
    {
        Body::EditGetCaret { position } => position,
        _ => None,
    }
}

/// Key down of `key`; true when the edit handled it.
fn key_down(world: &mut World<'_>, edit: ElementId, key: VirtualKey) -> bool {
    key_message(world, edit, key, true)
}

fn key_up(world: &mut World<'_>, edit: ElementId, key: VirtualKey) -> bool {
    key_message(world, edit, key, false)
}

fn key_message(world: &mut World<'_>, edit: ElementId, key: VirtualKey, down: bool) -> bool {
    let data = KeyData {
        key: key.into(),
        character: None,
        flags: KeyFlags::NONE,
        user: UserIndex::new(0).unwrap(),
    };
    let body = if down {
        Body::KeyDown(data)
    } else {
        Body::KeyUp(data)
    };

    world.cx().send_body(edit, body).handled
}

/// Runs the 16 ms scroll timer `steps` times.
fn run_timer(world: &mut World<'_>, steps: usize) {
    for _ in 0..steps {
        world.runtime.timers.advance_clock(16.0);
        world.cx().process_timers();
    }
}

fn arrow_state(world: &mut World<'_>, id: &str) -> ScrollEndState {
    let arrow = world.find(id);
    match world.cx().class_state(arrow) {
        Some(ClassState::ScrollEnd { state }) => *state,
        _ => panic!("{id} is no scroll end"),
    }
}

fn arrow_shown(world: &mut World<'_>, id: &str) -> bool {
    let arrow = world.find(id);
    world.tree.is_shown(arrow)
}

/// The end-of-scroll report (notify 5) of an arrow.
fn arrow_reports(world: &mut World<'_>, edit: ElementId, id: &str) {
    let source = world.find(id);
    world.cx().send_body(
        edit,
        Body::Notify(Notify {
            kind: NotifyKind::ScrollEndStateRequest,
            source,
        }),
    );
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

/// Index of the first shown character: the text length less the length
/// of the display copy.
fn first_shown(world: &mut World<'_>, edit: ElementId) -> usize {
    let text = source_text(world, edit, false).map_or(0, |text| text.len());
    let shown = source_text(world, edit, true).map_or(0, |text| text.len());

    text - shown
}

fn caret_x(world: &mut World<'_>) -> f32 {
    let caret = world.find("Caret");
    world.tree.get(caret).unwrap().position().x
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

fn insert(world: &mut World<'_>, edit: ElementId, index: u32, text: Option<&str>) {
    let text = text.map(|text| text.encode_utf16().collect::<Vec<u16>>().into());
    world
        .cx()
        .send_body(edit, Body::EditInsertText { index, text });
}

fn delete(world: &mut World<'_>, edit: ElementId, index: u32, count: u32) {
    world
        .cx()
        .send_body(edit, Body::EditDeleteText { index, count });
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
fn smooth_scroll_steps_on_the_edit_level_timer() {
    let registry = registry();
    let provider = MapProvider::default();
    let mut world = world(&registry, &provider);
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

    world.load(&multi_scene("TestEdit", true, 200.0), vec![]);
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
        run_timer(&mut world, 1);
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

#[test]
fn line_starts_follow_the_wrapped_lines() {
    let registry = registry();
    let provider = MapProvider::default();
    let mut world = world(&registry, &provider);
    world.load(&multi_scene("XuiEdit", false, 40.0), vec![]);
    let edit = world.find("edit");

    // 40 px holds five 'I': lines break after a space, at a line feed,
    // and inside a word with no break opportunity.
    set_text(&mut world, edit, "IIII IIII II\nIIIIIIII");
    let starts: Vec<Option<u32>> = (0..6)
        .map(|line| line_start(&mut world, edit, line))
        .collect();
    assert_eq!(
        starts,
        [Some(0), Some(5), Some(10), Some(13), Some(18), None]
    );
    assert_eq!(
        line_info(&mut world, edit, EditLineInfoKind::Lines),
        Some(5)
    );

    // A read-only edit takes no inserted text.
    insert(&mut world, edit, 0, Some("I"));
    assert_eq!(
        line_info(&mut world, edit, EditLineInfoKind::Lines),
        Some(5)
    );

    // A single-line edit answers its first shown character for line 0.
    let mut world2 = self::world(&registry, &provider);
    world2.load(&input_scene(100, ""), vec![]);
    let single = world2.find("edit");
    set_text(&mut world2, single, "IIII");
    assert_eq!(line_start(&mut world2, single, 0), Some(0));
    assert_eq!(line_start(&mut world2, single, 1), None);
}

#[test]
fn smooth_key_scroll_stops_at_the_last_page() {
    let registry = registry();
    let provider = MapProvider::default();
    let mut world = world(&registry, &provider);
    world.load(&multi_scene("XuiEdit", true, 200.0), vec![]);
    let edit = world.find("edit");

    // Two lines fit: no arrows and the keys go to navigation.
    set_text(&mut world, edit, "I\nI");
    assert!(!arrow_shown(&mut world, "ScrollUp"));
    assert!(!arrow_shown(&mut world, "ScrollDown"));
    assert!(!key_down(&mut world, edit, VirtualKey::PadDpadDown));

    // Four lines of 22 px in 50 px: two lines of scroll, 44 px.
    set_text(&mut world, edit, "I\nI\nI\nI");
    assert!(arrow_shown(&mut world, "ScrollUp"));
    assert!(arrow_shown(&mut world, "ScrollDown"));
    assert_eq!(arrow_state(&mut world, "ScrollUp"), ScrollEndState::Normal);
    assert_eq!(arrow_state(&mut world, "ScrollDown"), ScrollEndState::More);
    assert!(!key_down(&mut world, edit, VirtualKey::PadDpadUp));

    // Releasing the key stops the scroll at the next line.
    assert!(key_down(&mut world, edit, VirtualKey::PadDpadDown));
    assert_eq!(
        arrow_state(&mut world, "ScrollDown"),
        ScrollEndState::Scrolling
    );
    assert_eq!(arrow_state(&mut world, "ScrollUp"), ScrollEndState::More);
    assert!(key_up(&mut world, edit, VirtualKey::PadDpadDown));
    run_timer(&mut world, 100);
    assert_eq!(top_line(&mut world, edit, true), Some(1));
    assert_eq!(
        shown(&mut world, edit),
        ("I\nI\nI".encode_utf16().collect(), 0.0)
    );

    // The arrow keeps the scrolling state until it reports the end.
    assert_eq!(
        arrow_state(&mut world, "ScrollDown"),
        ScrollEndState::Scrolling
    );
    arrow_reports(&mut world, edit, "ScrollDown");
    assert_eq!(arrow_state(&mut world, "ScrollDown"), ScrollEndState::More);

    // Held down, the scroll runs to the last page and no further.
    assert!(key_down(&mut world, edit, VirtualKey::PadDpadDown));
    run_timer(&mut world, 100);
    assert_eq!(top_line(&mut world, edit, true), Some(2));
    arrow_reports(&mut world, edit, "ScrollDown");
    assert_eq!(
        arrow_state(&mut world, "ScrollDown"),
        ScrollEndState::Normal
    );
    assert!(!key_down(&mut world, edit, VirtualKey::PadDpadDown));
    assert_eq!(
        shown(&mut world, edit),
        ("I\nI".encode_utf16().collect(), 0.0)
    );

    assert!(key_down(&mut world, edit, VirtualKey::PadDpadUp));
    assert_eq!(
        arrow_state(&mut world, "ScrollUp"),
        ScrollEndState::Scrolling
    );
    assert_eq!(arrow_state(&mut world, "ScrollDown"), ScrollEndState::More);
}

#[test]
fn key_scroll_without_smooth_scroll_moves_one_line_per_key() {
    let registry = registry();
    let provider = MapProvider::default();
    let mut world = world(&registry, &provider);
    world.load(&multi_scene("XuiEdit", false, 200.0), vec![]);
    let edit = world.find("edit");
    set_text(&mut world, edit, "I\nI\nI\nI\nI");

    // Each key adds a line to the target; each step moves one line.
    assert!(key_down(&mut world, edit, VirtualKey::PadDpadDown));
    assert!(key_down(&mut world, edit, VirtualKey::PadDpadDown));
    assert_eq!(top_line(&mut world, edit, false), Some(2));
    run_timer(&mut world, 1);
    assert_eq!(top_line(&mut world, edit, true), Some(1));
    run_timer(&mut world, 1);
    assert_eq!(top_line(&mut world, edit, true), Some(2));

    // The target stops at the last page, three lines down.
    for _ in 0..3 {
        assert!(key_down(&mut world, edit, VirtualKey::PadDpadDown));
    }
    assert_eq!(top_line(&mut world, edit, false), Some(3));
    run_timer(&mut world, 5);
    assert_eq!(top_line(&mut world, edit, true), Some(3));
    assert!(!key_down(&mut world, edit, VirtualKey::PadDpadDown));
}

#[test]
fn caret_moves_shift_the_view_by_five_characters() {
    let registry = registry();
    let provider = MapProvider::default();
    let mut world = world(&registry, &provider);
    world.load(&input_scene(100, ""), vec![]);
    let edit = world.find("edit");

    // Twelve 'I' fill 96 of 100 px.
    type_text(&mut world, edit, "IIIIIIIIIIII");
    assert_eq!(caret(&mut world, edit), Some(12));
    assert_eq!(caret_x(&mut world), 96.0);
    // Line starts come from the last layout, which had no text.
    assert_eq!(line_start(&mut world, edit, 0), None);
    assert_eq!(first_shown(&mut world, edit), 0);
    assert!(!arrow_shown(&mut world, "ScrollLeft"));
    assert!(!arrow_shown(&mut world, "ScrollRight"));

    // The thirteenth does not fit: the view moves right by five.
    type_text(&mut world, edit, "I");
    assert_eq!(caret(&mut world, edit), Some(13));
    assert_eq!(first_shown(&mut world, edit), 5);
    assert_eq!(source(&mut world, edit, true).as_deref(), Some("IIIIIIII"));
    assert_eq!(caret_x(&mut world), 64.0);
    assert!(arrow_shown(&mut world, "ScrollLeft"));
    assert!(arrow_shown(&mut world, "ScrollRight"));
    assert_eq!(arrow_state(&mut world, "ScrollLeft"), ScrollEndState::More);
    assert_eq!(
        arrow_state(&mut world, "ScrollRight"),
        ScrollEndState::Scrolling
    );
    arrow_reports(&mut world, edit, "ScrollRight");
    assert_eq!(
        arrow_state(&mut world, "ScrollRight"),
        ScrollEndState::Normal
    );

    // Left from the first shown character moves the view back.
    for _ in 0..7 {
        assert!(key_down(&mut world, edit, VirtualKey::PadDpadLeft));
    }
    assert_eq!(caret(&mut world, edit), Some(6));
    assert_eq!(first_shown(&mut world, edit), 5);
    assert_eq!(caret_x(&mut world), 8.0);
    assert!(key_down(&mut world, edit, VirtualKey::PadDpadLeft));
    assert_eq!(caret(&mut world, edit), Some(5));
    assert_eq!(first_shown(&mut world, edit), 0);
    assert_eq!(caret_x(&mut world), 40.0);
    assert_eq!(
        arrow_state(&mut world, "ScrollLeft"),
        ScrollEndState::Scrolling
    );
    assert_eq!(arrow_state(&mut world, "ScrollRight"), ScrollEndState::More);

    for _ in 0..5 {
        assert!(key_down(&mut world, edit, VirtualKey::PadDpadLeft));
    }
    assert_eq!(caret(&mut world, edit), Some(0));
    assert!(!key_down(&mut world, edit, VirtualKey::PadDpadLeft));

    // Setting the caret moves it one character at a time.
    world
        .cx()
        .send_body(edit, Body::EditSetCaret { position: 13 });
    assert_eq!(caret(&mut world, edit), Some(13));
    assert_eq!(first_shown(&mut world, edit), 5);
    world
        .cx()
        .send_body(edit, Body::EditSetCaret { position: 14 });
    assert_eq!(caret(&mut world, edit), Some(13));
    assert!(!key_down(&mut world, edit, VirtualKey::PadDpadRight));
}

#[test]
fn insert_and_delete_messages_move_the_caret() {
    let registry = registry();
    let provider = MapProvider::default();
    let mut world = world(&registry, &provider);
    world.load(&input_scene(100, ""), vec![]);
    let edit = world.find("edit");
    set_text(&mut world, edit, "IOIO");

    // Text inserted at or before the caret moves it.
    insert(&mut world, edit, 0, Some("HH"));
    assert_eq!(caret(&mut world, edit), Some(2));
    insert(&mut world, edit, 4, Some("I"));
    assert_eq!(caret(&mut world, edit), Some(2));
    insert(&mut world, edit, 0, None);
    assert_eq!(source(&mut world, edit, false).as_deref(), Some("HHIOIIO"));
    assert_eq!(caret_x(&mut world), 28.0);

    delete(&mut world, edit, 0, 1);
    assert_eq!(source(&mut world, edit, false).as_deref(), Some("HIOIIO"));
    assert_eq!(caret(&mut world, edit), Some(1));

    // A caret inside the deleted range moves back by the count.
    world
        .cx()
        .send_body(edit, Body::EditSetCaret { position: 4 });
    delete(&mut world, edit, 2, 3);
    assert_eq!(source(&mut world, edit, false).as_deref(), Some("HIO"));
    assert_eq!(caret(&mut world, edit), Some(1));

    // Moving back past the start fails and leaves the caret.
    delete(&mut world, edit, 0, 3);
    assert_eq!(source(&mut world, edit, false).as_deref(), Some(""));
    assert_eq!(caret(&mut world, edit), Some(1));

    // A range past the end deletes nothing.
    set_text(&mut world, edit, "IO");
    delete(&mut world, edit, 1, 2);
    assert_eq!(source(&mut world, edit, false).as_deref(), Some("IO"));
}

#[test]
fn password_characters_replace_the_display_copy() {
    let registry = registry();
    let provider = MapProvider::default();
    let mut world = world(&registry, &provider);
    world.load(&input_scene(100, ""), vec![]);
    let edit = world.find("edit");
    type_text(&mut world, edit, "IO");

    // The display copy changes at its next rebuild.
    world
        .cx()
        .set_property(
            edit,
            &[props::edit_password_char()],
            None,
            PropValue::String(Some("*#".encode_utf16().collect())),
        )
        .unwrap();
    assert_eq!(
        world.tree.string(edit, &[props::edit_password_char()]),
        Some("*".encode_utf16().collect::<Vec<u16>>().as_slice())
    );
    assert_eq!(source(&mut world, edit, true).as_deref(), Some("IO"));

    type_text(&mut world, edit, "H");
    assert_eq!(source(&mut world, edit, true).as_deref(), Some("***"));
    assert_eq!(source(&mut world, edit, false).as_deref(), Some("IOH"));
    // The caret stands after the shown characters, 12 px each.
    assert_eq!(caret_x(&mut world), 36.0);
}

#[test]
fn text_limit_bounds_set_and_typed_text() {
    let registry = registry();
    let provider = MapProvider::default();
    let mut world = world(&registry, &provider);
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
    let registry = registry();
    let provider = MapProvider::default();
    let mut world = world(&registry, &provider);
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
fn typing_without_a_font_inserts_at_the_start() {
    let registry = registry();
    let provider = MapProvider::default();
    let mut world = World::new(&registry, &provider);
    world.load(&input_scene(20, ""), vec![]);
    let edit = world.find("edit");

    // Caret moves need the font, so the caret stays at 0.
    type_text(&mut world, edit, "abc");
    assert_eq!(source(&mut world, edit, false).as_deref(), Some("cba"));
    assert_eq!(caret(&mut world, edit), Some(0));
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
    let registry = registry();
    let provider = MapProvider::default();
    let mut world = world(&registry, &provider);
    world.load(&input_scene(20, "a-z"), vec![]);
    let edit = world.find("edit");
    set_text(&mut world, edit, "text");

    let first = source_text(&mut world, edit, true).unwrap();
    let second = source_text(&mut world, edit, true).unwrap();
    assert!(Rc::ptr_eq(&first, &second));
}

#[test]
fn loaded_properties_run_their_setters_in_serialized_order() {
    let registry = registry();
    let provider = MapProvider::default();
    let mut world = world(&registry, &provider);
    let visual = Obj::new("XuiVisual")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("visual"))
        .child(Obj::new("XuiTextPresenter").with(&xui::XUI_ELEMENT, "Id", V::Str("Text")));
    let edit = Obj::new("XuiEdit")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("edit"))
        .with(&xui::XUI_EDIT, "PasswordChar", V::Str("*#"))
        .with(&xui::XUI_EDIT, "TextLimit", V::Unsigned(3))
        .with(&xui::XUI_CONTROL, "Text", V::Str("abcdef"))
        .child(visual);
    let scene = Obj::new("TestScene")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("scene"))
        .child(edit);
    world.load(&scene, vec![]);
    let edit = world.find("edit");

    // Text (XuiControl level) is set before TextLimit (XuiEdit level), so
    // the limit cuts the text. The layout of 0x13 rebuilds the display copy
    // with the password character. The edit takes the text; XuiControl
    // stores none.
    assert_eq!(source(&mut world, edit, false).as_deref(), Some("abc"));
    assert_eq!(source(&mut world, edit, true).as_deref(), Some("***"));
    assert_eq!(world.tree.string(edit, &[props::text()]), None);
    assert_eq!(
        world.tree.string(edit, &[props::edit_password_char()]),
        Some("*".encode_utf16().collect::<Vec<u16>>().as_slice())
    );
}

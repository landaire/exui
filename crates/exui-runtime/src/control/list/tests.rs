//! XuiCommonList over a skin visual with scroll ends and a list item
//! template: item controls, navigation, scrolling, data changes and visual
//! recreation.

use std::format;
use std::rc::Rc;
use std::string::String;
use std::vec;
use std::vec::Vec;

use exui_core::Registry;
use exui_core::schema::xui;

use crate::control::ClassState;
use crate::control::tests::World;
use crate::control::tests::name_of;
use crate::control::tests::test_registry;
use crate::control::tests::user0;
use crate::input::VirtualKey;
use crate::message::Body;
use crate::message::Context;
use crate::message::SourceTextQuery;
use crate::scene::ElementId;
use crate::test_support::MapProvider;
use crate::test_support::Obj;
use crate::test_support::V;
use crate::test_support::build_scene;

/// List size: 100 x 60.
const LIST_HEIGHT: f32 = 60.0;

/// A skin whose `XuiCommonList` visual holds a prev and a next scroll end
/// and a list item template `item_height` high.
fn skin(registry: &Registry, item_height: f32) -> Vec<u8> {
    let end = |id: &'static str, direction: u32| {
        Obj::new("XuiScrollEnd")
            .with(&xui::XUI_ELEMENT, "Id", V::Str(id))
            .with(&xui::XUI_SCROLL_END, "Direction", V::Unsigned(direction))
    };
    let visual = Obj::new("XuiVisual")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("XuiCommonList"))
        .with(&xui::XUI_ELEMENT, "Width", V::Float(100.0))
        .with(&xui::XUI_ELEMENT, "Height", V::Float(LIST_HEIGHT))
        .child(end("prev", 0))
        .child(end("next", 1))
        .child(
            Obj::new("XuiListItem")
                .with(&xui::XUI_ELEMENT, "Id", V::Str("item"))
                .with(&xui::XUI_ELEMENT, "Width", V::Float(100.0))
                .with(&xui::XUI_ELEMENT, "Height", V::Float(item_height)),
        );

    build_scene(registry, &Obj::new("XuiCanvas").child(visual))
}

/// Loads `skin`, a scene with the list, creates the visuals and inserts
/// `count` items.
fn load_list<'p>(world: &mut World<'p>, skin: &'p [u8], wrap: bool, count: u32) -> ElementId {
    let registry = world.registry;
    let base = world.tree.add_base_path(&[]);
    world
        .runtime
        .skins
        .load(registry, skin, base, None)
        .unwrap();

    let list = Obj::new("XuiCommonList")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("list"))
        .with(&xui::XUI_ELEMENT, "Width", V::Float(100.0))
        .with(&xui::XUI_ELEMENT, "Height", V::Float(LIST_HEIGHT))
        .with(&xui::XUI_LIST, "Wrap", V::Bool(wrap));
    let scene = Obj::new("TestScene")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("scene"))
        .child(list);
    world.load(&scene, vec![]);
    let root = world.root;
    world.cx().prepare_visuals(root);

    let list = world.find("list");
    world
        .cx()
        .send_body(list, Body::InsertItems { index: 0, count });

    list
}

fn end_state(cx: &mut Context<'_, '_>, end: Option<ElementId>) -> String {
    match end.and_then(|end| cx.class_state(end)) {
        Some(ClassState::ScrollEnd { state }) => format!("{state:?}"),
        _ => String::from("none"),
    }
}

/// Top, current item and slot, the data index of each shown item control,
/// the focus and the scroll end states.
fn describe(world: &mut World<'_>, list: ElementId) -> String {
    let mut cx = world.cx();
    let state = match cx.class_state(list) {
        Some(ClassState::List { list, .. }) => (**list).clone(),
        _ => panic!("not a list"),
    };

    let items: Vec<String> = state
        .items
        .iter()
        .map(|item| match (cx.is_shown(*item), cx.control_index(*item)) {
            (true, Some(index)) => format!("{index}"),
            _ => String::from("-"),
        })
        .collect();
    let focus = match cx.runtime.focus.slot(user0()) {
        None => String::from("none"),
        Some(focused) if focused == list => String::from("list"),
        Some(focused) => state
            .items
            .iter()
            .position(|item| *item == focused)
            .map_or_else(|| name_of(cx.tree, focused), |slot| format!("item {slot}")),
    };
    let prev = end_state(&mut cx, state.prev_end);
    let next = end_state(&mut cx, state.next_end);

    format!(
        "count {} top {:?} cur {:?} slot {:?} items [{}] focus {focus} prev {prev} next {next}",
        state.count,
        state.top,
        state.cur,
        state.slot,
        items.join(" "),
    )
}

fn focus_list(world: &mut World<'_>, list: ElementId) {
    world.cx().set_focus(Some(list), user0()).unwrap();
}

fn item_text(world: &mut World<'_>, list: ElementId, item: u32) -> Option<String> {
    let reply = world.cx().send_body(
        list,
        Body::GetSourceText(SourceTextQuery {
            item,
            association: 0,
            item_data: true,
            text: None,
            point_size: None,
            extended: false,
            y_offset: 0.0,
            style: 0,
        }),
    );

    match reply.body {
        Body::GetSourceText(query) => query.text.map(|text| String::from_utf16_lossy(&text)),
        _ => None,
    }
}

#[test]
fn moves_between_items_and_scrolls_by_line_and_page() {
    let registry = test_registry();
    let skin = skin(&registry, 20.0);
    let provider = MapProvider::default();
    let mut world = World::new(&registry, &provider);
    let list = load_list(&mut world, &skin, false, 10);
    let mut lines = vec![describe(&mut world, list)];

    focus_list(&mut world, list);
    lines.push(describe(&mut world, list));
    for _ in 0..3 {
        world.key(VirtualKey::PadDpadDown);
        lines.push(describe(&mut world, list));
    }
    world.key(VirtualKey::PadRightTrigger);
    lines.push(describe(&mut world, list));
    for _ in 0..2 {
        world.key(VirtualKey::PadLeftTrigger);
        lines.push(describe(&mut world, list));
    }
    for _ in 0..2 {
        world.key(VirtualKey::PadDpadUp);
        lines.push(describe(&mut world, list));
    }

    assert_eq!(
        lines,
        [
            "count 10 top Some(0) cur Some(0) slot Some(0) items [0 1 2] focus none prev Normal next Scrolling",
            "count 10 top Some(0) cur Some(0) slot Some(0) items [0 1 2] focus item 0 prev Normal next More",
            "count 10 top Some(0) cur Some(1) slot Some(1) items [0 1 2] focus item 1 prev Normal next More",
            "count 10 top Some(0) cur Some(2) slot Some(2) items [0 1 2] focus item 2 prev Normal next More",
            // The last slot moves the list down one line.
            "count 10 top Some(1) cur Some(3) slot Some(2) items [1 2 3] focus item 2 prev More next Scrolling",
            // On the last visible slot a page moves the list by the visible count.
            "count 10 top Some(4) cur Some(6) slot Some(2) items [4 5 6] focus item 2 prev More next Scrolling",
            // Off the first slot, page up only selects the top item.
            "count 10 top Some(4) cur Some(4) slot Some(0) items [4 5 6] focus item 0 prev More next Scrolling",
            "count 10 top Some(1) cur Some(1) slot Some(0) items [1 2 3] focus item 0 prev Scrolling next More",
            "count 10 top Some(0) cur Some(0) slot Some(0) items [0 1 2] focus item 0 prev Normal next More",
            // Without Wrap the first item stays.
            "count 10 top Some(0) cur Some(0) slot Some(0) items [0 1 2] focus item 0 prev Normal next More",
        ]
    );
}

#[test]
fn wrap_moves_between_the_ends() {
    let registry = test_registry();
    let skin = skin(&registry, 20.0);
    let provider = MapProvider::default();
    let mut world = World::new(&registry, &provider);
    let list = load_list(&mut world, &skin, true, 10);
    focus_list(&mut world, list);

    world.key(VirtualKey::PadDpadUp);
    let to_last = describe(&mut world, list);
    world.key(VirtualKey::PadDpadDown);
    let to_first = describe(&mut world, list);

    assert_eq!(
        [to_last, to_first],
        [
            "count 10 top Some(7) cur Some(9) slot Some(2) items [7 8 9] focus item 2 prev More next Normal",
            "count 10 top Some(0) cur Some(0) slot Some(0) items [0 1 2] focus item 0 prev Normal next More",
        ]
    );
}

#[test]
fn insert_and_delete_rebind_the_item_controls() {
    let registry = test_registry();
    let skin = skin(&registry, 20.0);
    let provider = MapProvider::default();
    let mut world = World::new(&registry, &provider);
    let list = load_list(&mut world, &skin, false, 10);
    for item in 0..10u32 {
        let text: Rc<[u16]> = format!("t{item}").encode_utf16().collect();
        world.cx().send_body(
            list,
            Body::SetSourceText {
                item,
                association: 0,
                item_data: true,
                text: Some(text),
            },
        );
    }

    world.cx().list_set_cur_sel_visible(list, 9);
    let mut lines = vec![describe(&mut world, list)];

    world
        .cx()
        .send_body(list, Body::DeleteItems { index: 5, count: 5 });
    lines.push(describe(&mut world, list));
    let texts_after_delete = [
        item_text(&mut world, list, 4),
        item_text(&mut world, list, 5),
    ];

    world
        .cx()
        .send_body(list, Body::InsertItems { index: 5, count: 5 });
    lines.push(describe(&mut world, list));
    let text_after_insert = item_text(&mut world, list, 5);

    world.cx().send_body(
        list,
        Body::DeleteItems {
            index: 0,
            count: 10,
        },
    );
    lines.push(describe(&mut world, list));

    assert_eq!(
        lines,
        [
            "count 10 top Some(7) cur Some(9) slot Some(2) items [7 8 9] focus none prev More next Normal",
            // The current item is gone and the top moves back to the last page.
            "count 5 top Some(2) cur None slot None items [2 3 4] focus none prev Scrolling next Normal",
            "count 10 top Some(2) cur None slot None items [2 3 4] focus none prev More next More",
            "count 0 top None cur None slot None items [- - -] focus none prev Normal next Normal",
        ]
    );
    assert_eq!(texts_after_delete, [Some(String::from("t4")), None]);
    assert_eq!(text_after_insert, None);
}

/// A visual recreated with fewer item controls than the current slot leaves
/// no slot, and focusing the list does not reach past the new controls.
#[test]
fn recreated_visual_with_fewer_controls_clears_the_slot() {
    let registry = test_registry();
    let small = skin(&registry, 15.0);
    let large = skin(&registry, 30.0);
    let provider = MapProvider::default();
    let mut world = World::new(&registry, &provider);
    let list = load_list(&mut world, &small, false, 10);
    let prefix: Vec<u16> = "p".encode_utf16().collect();
    let base = world.tree.add_base_path(&[]);
    world
        .runtime
        .skins
        .load(&registry, &large, base, Some(&prefix))
        .unwrap();

    focus_list(&mut world, list);
    for _ in 0..3 {
        world.key(VirtualKey::PadDpadDown);
    }
    let before = describe(&mut world, list);

    let scene = world.find("scene");
    world.cx().apply_skin_prefix(scene, Some(&prefix));
    let recreated = describe(&mut world, list);

    focus_list(&mut world, list);
    let refocused = describe(&mut world, list);

    world.cx().list_set_cur_sel(list, Some(0));
    let selected = describe(&mut world, list);

    assert_eq!(
        [before, recreated, refocused, selected],
        [
            "count 10 top Some(0) cur Some(3) slot Some(3) items [0 1 2 3] focus item 3 prev Normal next More",
            "count 10 top Some(0) cur Some(3) slot None items [0 1] focus list prev Normal next More",
            "count 10 top Some(0) cur Some(3) slot None items [0 1] focus list prev Normal next More",
            "count 10 top Some(0) cur Some(0) slot Some(0) items [0 1] focus item 0 prev Normal next More",
        ]
    );
}

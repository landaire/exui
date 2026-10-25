use std::string::String;
use std::vec::Vec;

use exui_core::schema::xui;

use crate::control::props;
use crate::control::tests::World;
use crate::control::tests::find;
use crate::control::tests::test_registry;
use crate::message::Body;
use crate::scene::ElementId;
use crate::scene::PropValue;
use crate::test_support::MapProvider;
use crate::test_support::Obj;
use crate::test_support::V;
use crate::test_support::build_archive;
use crate::test_support::build_scene;

/// Key of the XuiControl Text property: level 1, position 10.
const TEXT_KEY: u32 = 0x10A << 16;

/// An id-keyed per-scene `.xus` table.
fn id_table(entries: &[(u32, &str)]) -> Vec<u8> {
    let mut body = Vec::new();
    for (id, value) in entries {
        let units: Vec<u16> = value.encode_utf16().collect();
        body.extend_from_slice(&(units.len() as u16).to_be_bytes());
        for unit in units {
            body.extend_from_slice(&unit.to_be_bytes());
        }
        body.extend_from_slice(&id.to_be_bytes());
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"XUIS");
    out.push(1);
    out.push(1);
    out.extend_from_slice(&((12 + body.len()) as u32).to_be_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_be_bytes());
    out.extend_from_slice(&body);
    out
}

fn label(id: &'static str, text: Option<&'static str>) -> Obj {
    let label = Obj::new("XuiLabel").with(&xui::XUI_ELEMENT, "Id", V::Str(id));
    match text {
        Some(text) => label.with(&xui::XUI_CONTROL, "Text", V::Str(text)),
        None => label,
    }
}

/// A canvas holding a scene with three labels. Load indexes: first 1,
/// second 2, plain 3, the scene 4, the canvas 5.
fn scene_file(registry: &exui_core::Registry) -> Vec<u8> {
    let scene = Obj::new("XuiScene")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("scene"))
        .child(label("first", Some("First")))
        .child(label("second", Some("Second")))
        .child(label("plain", None));
    build_scene(registry, &Obj::new("XuiCanvas").child(scene))
}

fn provider(registry: &exui_core::Registry) -> MapProvider {
    let scene = scene_file(registry);
    let german = id_table(&[(TEXT_KEY | 1, "Erste"), (TEXT_KEY | 3, "Schlicht")]);
    let archive = build_archive(&[("scene.xur", &scene), ("de-de\\scene.xus", &german)]);
    MapProvider::default().with_section("test", archive)
}

fn create(world: &mut World<'_>) -> ElementId {
    let base: Vec<u16> = "section://0,test#".encode_utf16().collect();
    let path: Vec<u16> = "scene.xur".encode_utf16().collect();
    let mut cx = world.cx();
    let base = cx.tree.add_base_path(&base);
    cx.create_scene(Some(base), &path).expect("scene loads")
}

fn texts(world: &mut World<'_>, scene: ElementId) -> Vec<Option<String>> {
    let cx = world.cx();
    ["first", "second", "plain"]
        .iter()
        .map(|id| {
            let element = find(&cx, scene, id);
            cx.tree
                .string(element, &[props::text()])
                .map(String::from_utf16_lossy)
        })
        .collect()
}

fn set_text(world: &mut World<'_>, scene: ElementId, id: &str, text: &str) {
    let mut cx = world.cx();
    let element = find(&cx, scene, id);
    let value = PropValue::String(Some(text.encode_utf16().collect()));
    cx.set_property(element, &[props::text()], None, value)
        .unwrap();
}

fn strings(values: [Option<&str>; 3]) -> Vec<Option<String>> {
    values.iter().map(|value| value.map(String::from)).collect()
}

#[test]
fn a_scene_loaded_with_a_locale_takes_its_table_by_load_index() {
    let registry = test_registry();
    let provider = provider(&registry);
    let mut world = World::new(&registry, &provider);

    let english = create(&mut world);
    world.resources.set_locale(Some("de-de"));
    let german = create(&mut world);

    assert_eq!(
        texts(&mut world, english),
        strings([Some("First"), Some("Second"), None])
    );
    assert_eq!(
        texts(&mut world, german),
        strings([Some("Erste"), Some("Second"), Some("Schlicht")])
    );
}

#[test]
fn relocalizing_restores_the_file_text_and_clears_text_the_file_lacks() {
    let registry = test_registry();
    let provider = provider(&registry);
    let mut world = World::new(&registry, &provider);
    world.resources.set_locale(Some("de-de"));
    let scene = create(&mut world);

    world.resources.set_locale(None);
    set_text(&mut world, scene, "second", "set by code");
    world.cx().relocalize(scene);
    let english = texts(&mut world, scene);

    set_text(&mut world, scene, "second", "set by code");
    world.resources.set_locale(Some("de-de"));
    world.cx().relocalize(scene);
    let german = texts(&mut world, scene);

    assert_eq!(english, strings([Some("First"), Some("Second"), None]));
    assert_eq!(
        german,
        strings([Some("Erste"), Some("Second"), Some("Schlicht")])
    );
}

#[test]
fn message_0x1f_relocalizes_a_scene_and_marks_it() {
    let registry = test_registry();
    let provider = provider(&registry);
    let mut world = World::new(&registry, &provider);
    let scene = create(&mut world);

    world.resources.set_locale(Some("de-de"));
    world.cx().broadcast(scene, &Body::Relocalize);

    assert_eq!(
        texts(&mut world, scene),
        strings([Some("Erste"), Some("Second"), Some("Schlicht")])
    );
    assert_eq!(
        world.cx().scene_state(scene).map(|state| state.relocalized),
        Some(true)
    );
}

#[test]
fn navigating_back_from_a_marked_scene_relocalizes_the_target_once() {
    let registry = test_registry();
    let provider = provider(&registry);
    let mut world = World::new(&registry, &provider);
    let target = create(&mut world);
    let current = create(&mut world);

    world.resources.set_locale(Some("de-de"));
    world.cx().relocalize_back_target(current, target);
    let unmarked = texts(&mut world, target);

    world.cx().broadcast(current, &Body::Relocalize);
    world.cx().relocalize_back_target(current, target);
    let marked = texts(&mut world, target);
    let cleared = world
        .cx()
        .scene_state(current)
        .map(|state| state.relocalized);

    assert_eq!(unmarked, strings([Some("First"), Some("Second"), None]));
    assert_eq!(
        marked,
        strings([Some("Erste"), Some("Second"), Some("Schlicht")])
    );
    assert_eq!(cleared, Some(false));
}

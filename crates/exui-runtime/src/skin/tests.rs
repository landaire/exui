use std::vec::Vec;

use exui_core::schema::xui;
use exui_core::value::Color;
use exui_core::value::Quaternion;
use exui_core::value::Vector3;

use crate::control::tests::World;
use crate::control::tests::test_registry;
use crate::scene::PropValue;
use crate::test_support::Bytes;
use crate::test_support::MapProvider;
use crate::test_support::Obj;
use crate::test_support::V;
use crate::test_support::build_scene;
use crate::test_support::def;

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

fn name_table(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut body = Bytes::default();
    for (key, value) in entries {
        body.u16(value.encode_utf16().count() as u16)
            .wide(value)
            .u16(key.encode_utf16().count() as u16)
            .wide(key);
    }

    let mut table = Bytes::default();
    table
        .raw(b"XUIS")
        .u8(1)
        .u8(0)
        .u32((12 + body.0.len()) as u32)
        .u16(entries.len() as u16)
        .raw(&body.0);
    table.0
}

#[test]
fn skin_string_table_applies_prefix_ids_class_fallback_and_numeric_values() {
    let registry = test_registry();
    let skin = Obj::new("XuiCanvas").child(
        Obj::new("XuiVisual")
            .with(&xui::XUI_ELEMENT, "Id", V::Str("VisualA"))
            .with(&xui::XUI_ELEMENT, "Width", V::Float(10.0))
            .child(
                Obj::new("XuiLabel")
                    .with(&xui::XUI_ELEMENT, "Id", V::Str("Title"))
                    .with(&xui::XUI_CONTROL, "Text", V::Str("authored")),
            )
            .child(Obj::new("XuiLabel").with(&xui::XUI_CONTROL, "Text", V::Str("class authored")))
            .child(Obj::new("XuiText").with(&xui::XUI_ELEMENT, "Id", V::Str("ColorText")))
            .child(Obj::new("XuiControl").with(&xui::XUI_ELEMENT, "Id", V::Str("Dynamic"))),
    );
    let mut provider = MapProvider::default();
    provider
        .files
        .insert(units("skin.xur"), build_scene(&registry, &skin));
    provider.files.insert(
        units("skin.xus"),
        name_table(&[
            ("PVisualA.Width", "100.5"),
            ("PVisualA.Height", "50"),
            ("PVisualA.Title.Text", "named localized"),
            ("PVisualA.XuiLabel.Text", "class localized"),
            ("PVisualA.Show", "false"),
            ("PVisualA.Rotation", "0,0,0,1"),
            ("PVisualA.Title.Position", "1,2,3"),
            ("PVisualA.ColorText.TextColor", "0x80402010"),
            ("PVisualA.Dynamic.ClassOverride", "XuiButton"),
        ]),
    );
    let mut world = World::new(&registry, &provider);
    world.resources.set_locale(Some("en-us"));

    world
        .cx()
        .load_visuals(&units("file://skin.xur"), Some(&units("P")))
        .unwrap();
    let visual = world
        .cx()
        .create_skin_visual(&units("VisualA"), Some(&units("P")), false)
        .unwrap();
    let title = world.tree.child_by_id(visual, &units("Title")).unwrap();
    let color_text = world.tree.child_by_id(visual, &units("ColorText")).unwrap();
    let dynamic = world.tree.child_by_id(visual, &units("Dynamic")).unwrap();
    let anonymous = world
        .tree
        .children(visual)
        .find(|&child| {
            world
                .tree
                .get(child)
                .and_then(|element| element.id())
                .is_none()
        })
        .unwrap();

    assert_eq!(
        world
            .tree
            .property(visual, &[def(&xui::XUI_ELEMENT, "Width")], None),
        Some(PropValue::Float(100.5))
    );
    assert_eq!(
        world
            .tree
            .property(visual, &[def(&xui::XUI_ELEMENT, "Height")], None),
        Some(PropValue::Float(50.0))
    );
    assert_eq!(
        world
            .tree
            .property(title, &[def(&xui::XUI_CONTROL, "Text")], None),
        Some(PropValue::String(Some(units("named localized").into())))
    );
    assert_eq!(
        world
            .tree
            .property(anonymous, &[def(&xui::XUI_CONTROL, "Text")], None),
        Some(PropValue::String(Some(units("class localized").into())))
    );
    assert_eq!(
        world.tree.get(title).unwrap().design.parent_size.width,
        100.5
    );
    assert_eq!(
        world
            .tree
            .property(visual, &[def(&xui::XUI_ELEMENT, "Show")], None),
        Some(PropValue::Bool(false))
    );
    assert_eq!(
        world
            .tree
            .property(visual, &[def(&xui::XUI_ELEMENT, "Rotation")], None),
        Some(PropValue::Quaternion(Quaternion::new(0.0, 0.0, 0.0, 1.0)))
    );
    assert_eq!(
        world
            .tree
            .property(title, &[def(&xui::XUI_ELEMENT, "Position")], None),
        Some(PropValue::Vector(Vector3::new(1.0, 2.0, 3.0)))
    );
    assert_eq!(
        world
            .tree
            .property(color_text, &[def(&xui::XUI_TEXT, "TextColor")], None),
        Some(PropValue::Color(Color::from_argb(0x8040_2010)))
    );
    assert_eq!(
        registry
            .class(world.tree.class(dynamic).unwrap())
            .unwrap()
            .name(),
        "XuiButton"
    );

    world.resources.set_locale(None);
    let localized_again = world
        .cx()
        .create_skin_visual(&units("VisualA"), Some(&units("P")), false)
        .unwrap();
    let localized_title = world
        .tree
        .child_by_id(localized_again, &units("Title"))
        .unwrap();
    assert_eq!(
        world
            .tree
            .property(localized_title, &[def(&xui::XUI_CONTROL, "Text")], None),
        Some(PropValue::String(Some(units("authored").into())))
    );

    let mut unlocalized = World::new(&registry, &provider);
    unlocalized
        .cx()
        .load_visuals(&units("file://skin.xur"), Some(&units("P")))
        .unwrap();
    let visual = unlocalized
        .cx()
        .create_skin_visual(&units("VisualA"), Some(&units("P")), false)
        .unwrap();
    assert_eq!(
        unlocalized
            .tree
            .property(visual, &[def(&xui::XUI_ELEMENT, "Width")], None),
        Some(PropValue::Float(10.0))
    );
}

#[test]
fn list_item_visual_falls_back_through_checkbox_class() {
    let registry = test_registry();
    let skin = build_scene(
        &registry,
        &Obj::new("XuiCanvas").child(
            Obj::new("XuiVisual")
                .with(&xui::XUI_ELEMENT, "Id", V::Str("XuiCheckbox"))
                .child(Obj::new("XuiElement").with(
                    &xui::XUI_ELEMENT,
                    "Id",
                    V::Str("checkbox_skin"),
                )),
        ),
    );
    let provider = MapProvider::default();
    let mut world = World::new(&registry, &provider);
    let base = world.tree.add_base_path(&[]);
    world
        .runtime
        .skins
        .load(&registry, &skin, base, None)
        .unwrap();
    let item = Obj::new("XuiListItem").with(&xui::XUI_ELEMENT, "Id", V::Str("item"));
    let scene = Obj::new("TestScene")
        .with(&xui::XUI_ELEMENT, "Id", V::Str("scene"))
        .child(item);
    world.load(&scene, Vec::new());

    let item = world.find("item");
    world.cx().create_control_visual(item).unwrap();
    let visual = world.cx().control_visual(item).unwrap();

    assert!(
        world
            .tree
            .child_by_id(visual, &units("checkbox_skin"))
            .is_some()
    );
}

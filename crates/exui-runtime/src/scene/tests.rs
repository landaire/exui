use core::num::NonZeroU32;
use std::format;
use std::string::String;
use std::vec;
use std::vec::Vec;

use exui_core::Document;
use exui_core::Registry;
use exui_core::custom::FigurePoint;
use exui_core::schema::xui;
use xbox360_draw::draw::DrawList;
use xbox360_draw::draw::Rgba8;
use xbox360_draw::draw::TextureId;
use xbox360_draw::error::TextureError;
use xbox360_draw::image::Image;
use xbox360_draw::texture::TextureSlots;
use xbox360_draw::texture::TextureStore;
use xbox360_raster::SoftwareRenderer;

use super::*;
use crate::DisplaySize;
use crate::Ui;
use crate::math::Matrix4;
use crate::resource::SectionName;
use crate::scene::figure::GradientStop;
use crate::scene::figure::GradientStops;
use crate::scene::figure::ear_clip;
use crate::scene::figure::fan;
use crate::scene::figure::flatten;
use crate::scene::figure::gradient_texels;
use crate::scene::figure::stroke_strip;
use crate::test_support::MapProvider;
use crate::test_support::Obj;
use crate::test_support::V;
use crate::test_support::build_archive;
use crate::test_support::build_scene;
use crate::test_support::def;
use crate::test_support::encode_png;
use crate::test_support::member;
use crate::test_support::pixel_dump;
use crate::test_support::prop;
use crate::test_support::rasterize;

/// Texture store that counts live textures.
#[derive(Default)]
struct CountingStore {
    slots: TextureSlots<()>,
    live: usize,
}

impl TextureStore for CountingStore {
    fn create_texture(&mut self, _image: Image) -> Result<TextureId, TextureError> {
        let id = self.slots.insert(())?;
        self.live += 1;

        Ok(id)
    }

    fn update_texture(&mut self, id: TextureId, _image: Image) -> Result<(), TextureError> {
        self.slots
            .get(id)
            .map(|_| ())
            .ok_or(TextureError::UnknownTexture { id })
    }

    fn destroy_texture(&mut self, id: TextureId) -> Result<(), TextureError> {
        self.slots
            .remove(id)
            .ok_or(TextureError::UnknownTexture { id })?;
        self.live -= 1;

        Ok(())
    }
}

fn registry() -> Registry {
    Registry::new()
}

fn instantiate_scene(
    registry: &Registry,
    root: &Obj,
) -> Result<(ElementTree, ElementId), LoadError> {
    let bytes = build_scene(registry, root);
    let document = Document::parse(&bytes, registry).expect("fixture parses");
    let mut tree = ElementTree::new();
    let id = instantiate(
        &mut tree,
        registry,
        &document,
        document.root(),
        None,
        &mut |_, _, _| {},
    )?;

    Ok((tree, id))
}

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

fn text(units: Option<&[u16]>) -> String {
    units.map_or(String::from("-"), String::from_utf16_lossy)
}

fn dump(tree: &ElementTree, registry: &Registry, root: ElementId) -> String {
    let mut out = String::new();
    let mut pending = vec![(root, 0usize)];
    while let Some((id, depth)) = pending.pop() {
        let element = tree.get(id).expect("live element");
        let class = registry
            .class(element.class())
            .expect("registered class")
            .name();
        let size = element.size();
        let position = element.position();
        out += &format!(
            "{:indent$}{class} id={} size={}x{} pos=({},{},{}) scale=({},{}) opacity={} anchor={:#x} blend={} flags={:#x} visual={}\n",
            "",
            text(element.id()),
            size.width,
            size.height,
            position.x,
            position.y,
            position.z,
            element.scale().x,
            element.scale().y,
            element.opacity(),
            element.anchor().0,
            element.blend_mode().raw(),
            element.flags().0,
            text(element.visual()),
            indent = depth * 2,
        );
        let children: Vec<_> = tree.children(id).collect();
        pending.extend(children.into_iter().rev().map(|child| (child, depth + 1)));
    }

    out
}

/// Straight-edged figure points of a `width` x `height` rectangle.
fn rectangle(width: f32, height: f32) -> V {
    polygon(
        width,
        height,
        &[[0.0, 0.0], [width, 0.0], [width, height], [0.0, height]],
    )
}

fn polygon(width: f32, height: f32, corners: &[[f32; 2]]) -> V {
    let points = (0..corners.len())
        .map(|index| {
            let [x, y] = corners[index];
            let [nx, ny] = corners[(index + 1) % corners.len()];
            [x, y, x, y, nx, ny]
        })
        .collect();

    V::Points {
        width,
        height,
        points,
    }
}

fn figure_points(corners: &[[f32; 2]]) -> Vec<FigurePoint> {
    (0..corners.len())
        .map(|index| {
            let [x, y] = corners[index];
            let [nx, ny] = corners[(index + 1) % corners.len()];
            FigurePoint {
                coords: [x, y, x, y, nx, ny],
            }
        })
        .collect()
}

fn fill(values: Vec<(&str, V)>) -> V {
    let fill = def(&xui::XUI_FIGURE, "Fill");
    V::Compound(
        values
            .into_iter()
            .map(|(name, value)| prop(member(fill, name), vec![value]))
            .collect(),
    )
}

fn gradient(radial: bool, stops: &[(u32, f32)]) -> V {
    let gradient = member(def(&xui::XUI_FIGURE, "Fill"), "Gradient");
    V::Compound(vec![
        prop(member(gradient, "Radial"), vec![V::Bool(radial)]),
        prop(
            member(gradient, "NumStops"),
            vec![V::Integer(stops.len() as i32)],
        ),
        prop(
            member(gradient, "StopColor"),
            stops.iter().map(|(color, _)| V::Color(*color)).collect(),
        ),
        prop(
            member(gradient, "StopPos"),
            stops
                .iter()
                .map(|(_, position)| V::Float(*position))
                .collect(),
        ),
    ])
}

fn sized(class: &'static str, width: f32, height: f32) -> Obj {
    Obj::new(class)
        .with(&xui::XUI_ELEMENT, "Width", V::Float(width))
        .with(&xui::XUI_ELEMENT, "Height", V::Float(height))
}

fn figure(width: f32, height: f32, x: f32, y: f32) -> Obj {
    sized("XuiFigure", width, height)
        .with(&xui::XUI_ELEMENT, "Position", V::Vector([x, y, 0.0]))
        .with(&xui::XUI_FIGURE, "Points", rectangle(width, height))
}

#[test]
fn instantiation_applies_properties_over_class_defaults() {
    let registry = registry();
    let scene = sized("XuiCanvas", 1120.0, 770.0)
        .child(
            sized("XuiGroup", 300.0, 200.0)
                .with(&xui::XUI_ELEMENT, "Id", V::Str("group"))
                .with(&xui::XUI_ELEMENT, "Position", V::Vector([10.0, 20.0, 0.0]))
                .with(&xui::XUI_ELEMENT, "Opacity", V::Float(0.5))
                .with(&xui::XUI_ELEMENT, "ClipChildren", V::Bool(true))
                .with(&xui::XUI_ELEMENT, "ColorWriteFlags", V::Unsigned(7))
                .child(
                    figure(100.0, 50.0, 1.0, 2.0)
                        .with(&xui::XUI_ELEMENT, "Anchor", V::Unsigned(15))
                        .with(&xui::XUI_ELEMENT, "BlendMode", V::Unsigned(5)),
                )
                .child(Obj::new("XuiImage")),
        )
        .child(
            Obj::new("XuiControl")
                .with(&xui::XUI_ELEMENT, "Id", V::Str("control"))
                .with(&xui::XUI_ELEMENT, "Show", V::Bool(false))
                .with(&xui::XUI_CONTROL, "ClassOverride", V::Str("XuiButton"))
                .with(&xui::XUI_CONTROL, "Visual", V::Str("BG_animation")),
        );

    let (tree, root) = instantiate_scene(&registry, &scene).expect("scene instantiates");

    insta::assert_snapshot!(dump(&tree, &registry, root));
}

#[test]
fn class_override_must_derive_from_the_authored_class() {
    let registry = registry();
    let unknown =
        Obj::new("XuiControl").with(&xui::XUI_CONTROL, "ClassOverride", V::Str("NotAClass"));
    let unrelated =
        Obj::new("XuiControl").with(&xui::XUI_CONTROL, "ClassOverride", V::Str("XuiGroup"));

    let (tree, root) = instantiate_scene(&registry, &unknown).expect("unknown override is ignored");
    let error = instantiate_scene(&registry, &unrelated)
        .map(|_| ())
        .expect_err("unrelated override fails");

    assert_eq!(tree.class(root), registry.find("XuiControl"));
    assert!(
        matches!(
            error,
            LoadError::ClassOverrideNotDerived {
                authored: "XuiControl",
                override_name: "XuiGroup"
            }
        ),
        "{error:?}"
    );
}

#[test]
fn properties_read_back_from_fields_store_and_defaults() {
    let registry = registry();
    let scene = Obj::new("XuiButton")
        .with(&xui::XUI_CONTROL, "NavLeft", V::Str("left"))
        .with(&xui::XUI_CONTROL, "Text", V::Str(""));
    let (mut tree, button) = instantiate_scene(&registry, &scene).expect("scene instantiates");
    let enabled = [def(&xui::XUI_CONTROL, "Enabled")];
    let nav_left = [def(&xui::XUI_CONTROL, "NavLeft")];
    let text_def = [def(&xui::XUI_CONTROL, "Text")];
    let width = [def(&xui::XUI_ELEMENT, "Width")];

    assert_eq!(
        tree.property(button, &enabled, None),
        Some(PropValue::Bool(true))
    );
    assert_eq!(
        tree.string(button, &nav_left),
        Some(units("left").as_slice())
    );
    // An empty Text stores the null string.
    assert_eq!(tree.string(button, &text_def), None);
    assert_eq!(
        tree.property(button, &width, None),
        Some(PropValue::Float(60.0))
    );

    tree.set_property(button, &enabled, None, PropValue::Bool(false))
        .expect("Enabled is a Bool");
    let mismatch = tree.set_property(button, &enabled, None, PropValue::Float(1.0));

    assert_eq!(
        tree.property(button, &enabled, None),
        Some(PropValue::Bool(false))
    );
    assert!(matches!(
        mismatch,
        Err(PropertyError::TypeMismatch {
            name: "Enabled",
            ..
        })
    ));
}

#[test]
fn group_resize_re_anchors_children_from_their_design_layout() {
    let registry = registry();
    let child = |id: &'static str, anchor: u32, x: f32| {
        sized("XuiFigure", 20.0, 10.0)
            .with(&xui::XUI_ELEMENT, "Id", V::Str(id))
            .with(&xui::XUI_ELEMENT, "Position", V::Vector([x, 5.0, 0.0]))
            .with(&xui::XUI_ELEMENT, "Anchor", V::Unsigned(anchor))
    };
    let scene = sized("XuiGroup", 100.0, 50.0)
        .child(child("right", Anchor::RIGHT, 70.0))
        .child(child("stretch", Anchor::LEFT | Anchor::RIGHT, 10.0))
        .child(child("center", Anchor::CENTER_X | Anchor::BOTTOM, 40.0))
        .child(child("fixed", Anchor::LEFT, 30.0));
    let (mut tree, group) = instantiate_scene(&registry, &scene).expect("scene instantiates");
    tree.take_size_changes();

    tree.set_property(
        group,
        &[def(&xui::XUI_ELEMENT, "Width")],
        None,
        PropValue::Float(200.0),
    )
    .expect("Width is a Float");
    tree.set_property(
        group,
        &[def(&xui::XUI_ELEMENT, "Height")],
        None,
        PropValue::Float(90.0),
    )
    .expect("Height is a Float");

    let state = |id: &str| {
        let element = tree
            .get(tree.child_by_id(group, &units(id)).expect("child exists"))
            .expect("live");
        (
            element.position().x,
            element.position().y,
            element.size().width,
        )
    };
    assert_eq!(state("right"), (170.0, 5.0, 20.0));
    assert_eq!(state("stretch"), (10.0, 5.0, 120.0));
    assert_eq!(state("center"), (90.0, 45.0, 20.0));
    assert_eq!(state("fixed"), (30.0, 5.0, 20.0));

    let changes = tree.take_size_changes();
    let stretch = tree.child_by_id(group, &units("stretch"));
    assert_eq!(changes.len(), 3);
    assert_eq!(changes[0].element, group);
    assert_eq!(Some(changes[1].element), stretch);
    assert_eq!(
        changes[1].new,
        Size {
            width: 120.0,
            height: 10.0
        }
    );
}

#[test]
fn canvas_does_not_lay_out_children() {
    let registry = registry();
    let scene = sized("XuiCanvas", 100.0, 50.0).child(sized("XuiFigure", 20.0, 10.0).with(
        &xui::XUI_ELEMENT,
        "Anchor",
        V::Unsigned(Anchor::RIGHT),
    ));
    let (mut tree, canvas) = instantiate_scene(&registry, &scene).expect("scene instantiates");

    tree.set_bounds(canvas, 300.0, 50.0);

    let child = tree.first_child(canvas).expect("child");
    assert_eq!(tree.get(child).expect("live").position().x, 0.0);
}

#[test]
fn world_transform_composes_pivot_rotation_and_parents() {
    let registry = registry();
    let half = core::f32::consts::FRAC_1_SQRT_2;
    let scene = sized("XuiGroup", 100.0, 100.0)
        .with(&xui::XUI_ELEMENT, "Position", V::Vector([100.0, 0.0, 0.0]))
        .with(&xui::XUI_ELEMENT, "Scale", V::Vector([2.0, 2.0, 1.0]))
        .child(
            sized("XuiGroup", 10.0, 10.0)
                .with(&xui::XUI_ELEMENT, "Position", V::Vector([10.0, 10.0, 0.0]))
                .with(
                    &xui::XUI_ELEMENT,
                    "Rotation",
                    V::Quaternion([0.0, 0.0, half, half]),
                )
                .with(&xui::XUI_ELEMENT, "Pivot", V::Vector([5.0, 5.0, 0.0])),
        );
    let (mut tree, group) = instantiate_scene(&registry, &scene).expect("scene instantiates");
    let child = tree.first_child(group).expect("child");

    let world = tree.world_transform(child).expect("live element");
    // Local (10, 5): 5 right of the pivot, turned to 5 below it, at
    // (15, 20) in the parent, scaled by 2 and moved by 100.
    let [x, y, _, _] = world.transform(10.0, 5.0, 0.0);

    assert!(
        (x - 130.0).abs() < 1e-3 && (y - 40.0).abs() < 1e-3,
        "({x}, {y})"
    );
}

#[test]
fn destroyed_ids_do_not_resolve() {
    let registry = registry();
    let scene = sized("XuiGroup", 10.0, 10.0).child(Obj::new("XuiFigure"));
    let (mut tree, group) = instantiate_scene(&registry, &scene).expect("scene instantiates");
    let child = tree.first_child(group).expect("child");

    tree.destroy(group).expect("destroy");
    let reused = tree.create(
        registry
            .class(registry.find("XuiCanvas").expect("canvas"))
            .expect("canvas"),
    );

    assert!(tree.get(group).is_none());
    assert!(tree.get(child).is_none());
    assert!(tree.get(reused).is_some());
}

#[test]
fn convex_outline_is_a_fan() {
    let mut points = Vec::new();
    let flattened = flatten(
        &figure_points(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]),
        1.0,
        1.0,
        &mut points,
    );
    let mut triangles = Vec::new();
    fan(points.len(), &mut triangles);

    assert!(flattened.convex);
    assert_eq!(points.len(), 5);
    assert_eq!(points[4], points[0]);
    assert_eq!(triangles, vec![[0, 1, 2], [0, 2, 3]]);
}

#[test]
fn collinear_points_keep_an_outline_convex() {
    let mut points = Vec::new();
    let flattened = flatten(
        &figure_points(&[
            [0.0, 0.0],
            [5.0, 0.0],
            [10.0, 0.0],
            [10.0, 10.0],
            [0.0, 10.0],
        ]),
        1.0,
        1.0,
        &mut points,
    );
    let mut triangles = Vec::new();
    ear_clip(&points, &mut triangles);

    assert!(flattened.convex);
    assert_eq!(triangles.len(), points.len() - 3);
    assert_eq!(total_area(&points, &triangles), 100.0);
}

#[test]
fn concave_outline_is_ear_clipped() {
    // L shape, counter-clockwise on screen, and the same mirrored.
    let shape = [
        [0.0, 0.0],
        [20.0, 0.0],
        [20.0, 10.0],
        [10.0, 10.0],
        [10.0, 20.0],
        [0.0, 20.0],
    ];
    let mirrored: Vec<[f32; 2]> = shape.iter().rev().copied().collect();

    let mut results = Vec::new();
    for corners in [&shape[..], &mirrored[..]] {
        let mut points = Vec::new();
        let flattened = flatten(&figure_points(corners), 1.0, 1.0, &mut points);
        let mut triangles = Vec::new();
        ear_clip(&points, &mut triangles);

        assert!(!flattened.convex);
        assert_eq!(triangles.len(), points.len() - 3);
        assert_eq!(total_area(&points, &triangles), 300.0);
        results.push(triangles);
    }

    insta::assert_debug_snapshot!(results);
}

fn total_area(points: &[[f32; 2]], triangles: &[[u32; 3]]) -> f32 {
    triangles
        .iter()
        .map(|triangle| {
            let [a, b, c] = triangle.map(|index| points[index as usize]);
            ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])).abs() / 2.0
        })
        .sum()
}

#[test]
fn curves_flatten_until_control_points_are_near_the_chord() {
    // Quarter circle approximation from (0, 0) to (100, 100).
    let points = [
        FigurePoint {
            coords: [0.0, 0.0, 55.0, 0.0, 100.0, 45.0],
        },
        FigurePoint {
            coords: [100.0, 100.0, 100.0, 100.0, 0.0, 0.0],
        },
    ];
    let mut out = Vec::new();
    flatten(&points, 1.0, 1.0, &mut out);

    assert_eq!(out.first(), Some(&[0.0, 0.0]));
    assert_eq!(out.last(), Some(&[0.0, 0.0]));
    assert!(out.contains(&[100.0, 100.0]));
    insta::assert_debug_snapshot!(out);
}

#[test]
fn stroke_strip_lengths() {
    let mut points = Vec::new();
    let flattened = flatten(
        &figure_points(&[[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]),
        1.0,
        1.0,
        &mut points,
    );
    let mut closed = Vec::new();
    let mut open = Vec::new();
    stroke_strip(&points, flattened.open_count, 2.0, true, &mut closed);
    stroke_strip(&points, flattened.open_count, 2.0, false, &mut open);

    // 5 * count - 5 vertices; open: 5 * m - 8 primitives.
    assert_eq!(closed.len(), 20);
    assert_eq!(flattened.open_count, 4);
    assert_eq!(open.len(), 14);
    // One-sided: the first segment runs along +x and is offset by (0, -w).
    assert_eq!(
        &closed[..4],
        &[[0.0, 0.0], [0.0, -2.0], [10.0, 0.0], [10.0, -2.0]]
    );
}

#[test]
fn gradient_texels_follow_the_stop_walk() {
    let stops = |list: &[(u32, f32)]| {
        GradientStops::from_stops(
            list.iter()
                .map(|&(argb, position)| GradientStop {
                    color: exui_core::value::Color::from_argb(argb),
                    position,
                })
                .collect(),
        )
    };
    let ramp = gradient_texels(&stops(&[(0xff00_0000, 0.0), (0xffff_ffff, 1.0)]));
    // The search advances one stop per texel, so dense stops lag behind.
    let dense = gradient_texels(&stops(&[
        (0xffff_0000, 0.0),
        (0xff00_ff00, 0.001),
        (0xff00_00ff, 0.002),
        (0x0000_0000, 0.5),
    ]));

    assert_eq!(ramp[0], 0xff00_0000);
    assert_eq!(ramp[64], 0xff80_8080);
    assert_eq!(ramp[127], 0xfffd_fdfd);
    let lines: Vec<String> = dense
        .chunks(8)
        .map(|chunk| {
            chunk
                .iter()
                .map(|texel| format!("{texel:08x}"))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    insta::assert_snapshot!(lines.join("\n"));
}

/// Stores `scene` as `scene.xur` in section `test`, with the provider's
/// fixture files beside it.
fn add_scene_section(registry: &Registry, scene: &Obj, provider: &mut MapProvider) {
    let bytes = build_scene(registry, scene);
    let archive_entries: Vec<(String, Vec<u8>)> = provider.files_for_archive();
    let mut entries: Vec<(&str, &[u8])> = archive_entries
        .iter()
        .map(|(name, data)| (name.as_str(), data.as_slice()))
        .collect();
    entries.push(("scene.xur", &bytes));
    let archive = build_archive(&entries);
    provider
        .sections
        .insert(SectionName::new("test").expect("name"), archive);
}

/// Renders `scene` (authored on a 1120x720 canvas) scaled to a `width` x
/// `height` target.
fn render_scene(
    registry: Registry,
    scene: &Obj,
    mut provider: MapProvider,
    width: u32,
    height: u32,
) -> String {
    add_scene_section(&registry, scene, &mut provider);

    let mut ui = Ui::new(registry, &provider, DisplaySize { width, height });
    ui.set_view(Matrix4::scaling(
        width as f32 / 1120.0,
        height as f32 / 720.0,
        1.0,
    ));
    ui.set_clear_color(Some(xbox360_draw::draw::Rgba8::new(0, 0, 0, 0xff)));
    let root = ui
        .load_scene("section://0,test#", "scene.xur")
        .expect("scene loads");
    let canvas = ui.root();
    ui.tree_mut().append_child(canvas, root).expect("attach");

    let mut renderer = SoftwareRenderer::new();
    let mut list = DrawList::new();
    ui.draw(&mut renderer, &mut list).expect("draw");

    pixel_dump(&rasterize(&renderer, &list, width, height), width)
}

impl MapProvider {
    /// Fixture images stored as `files` under ASCII names go into the
    /// scene archive instead.
    fn files_for_archive(&mut self) -> Vec<(String, Vec<u8>)> {
        core::mem::take(&mut self.files)
            .into_iter()
            .map(|(name, data)| (String::from_utf16_lossy(&name), data))
            .collect()
    }
}

#[test]
fn render_solid_fills_opacity_and_blend_modes() {
    let registry = registry();
    let solid = |color: u32| {
        fill(vec![
            ("FillType", V::Unsigned(1)),
            ("FillColor", V::Color(color)),
        ])
    };
    let stroke_def = def(&xui::XUI_FIGURE, "Stroke");
    let scene = sized("XuiCanvas", 1120.0, 770.0)
        .child(figure(1120.0, 770.0, 0.0, 0.0).with(&xui::XUI_FIGURE, "Fill", solid(0xff20_4060)))
        .child(
            figure(300.0, 300.0, 40.0, 100.0)
                .with(&xui::XUI_FIGURE, "Fill", solid(0xffff_0000))
                .with(&xui::XUI_ELEMENT, "Opacity", V::Float(0.5)),
        )
        .child(
            sized("XuiGroup", 400.0, 400.0)
                .with(
                    &xui::XUI_ELEMENT,
                    "Position",
                    V::Vector([400.0, 100.0, 0.0]),
                )
                .with(&xui::XUI_ELEMENT, "Opacity", V::Float(0.5))
                .child(
                    figure(300.0, 300.0, 0.0, 0.0)
                        .with(&xui::XUI_FIGURE, "Fill", solid(0xff00_ff00))
                        .with(&xui::XUI_ELEMENT, "Opacity", V::Float(0.5)),
                ),
        )
        // Zero opacity with multiply blending still multiplies.
        .child(
            figure(250.0, 600.0, 800.0, 80.0)
                .with(&xui::XUI_FIGURE, "Fill", solid(0xff80_80ff))
                .with(&xui::XUI_ELEMENT, "Opacity", V::Float(0.0))
                .with(&xui::XUI_ELEMENT, "BlendMode", V::Unsigned(2)),
        )
        // Zero opacity with normal blending is skipped.
        .child(
            figure(200.0, 200.0, 100.0, 450.0)
                .with(&xui::XUI_FIGURE, "Fill", solid(0xffff_ffff))
                .with(&xui::XUI_ELEMENT, "Opacity", V::Float(0.0)),
        )
        // Stroke only.
        .child(
            figure(200.0, 150.0, 450.0, 550.0)
                .with(
                    &xui::XUI_FIGURE,
                    "Fill",
                    fill(vec![("FillType", V::Unsigned(0))]),
                )
                .with(
                    &xui::XUI_FIGURE,
                    "Stroke",
                    V::Compound(vec![
                        prop(member(stroke_def, "StrokeWidth"), vec![V::Float(20.0)]),
                        prop(
                            member(stroke_def, "StrokeColor"),
                            vec![V::Color(0xffff_ff00)],
                        ),
                    ]),
                ),
        );

    insta::assert_snapshot!(render_scene(
        registry,
        &scene,
        MapProvider::default(),
        56,
        36
    ));
}

#[test]
fn render_linear_and_radial_gradients() {
    let registry = registry();
    let scene = sized("XuiCanvas", 1120.0, 770.0)
        .child(figure(560.0, 660.0, 0.0, 60.0).with(
            &xui::XUI_FIGURE,
            "Fill",
            fill(vec![
                ("FillType", V::Unsigned(2)),
                (
                    "Gradient",
                    gradient(false, &[(0xffff_0000, 0.0), (0xff00_00ff, 1.0)]),
                ),
            ]),
        ))
        .child(figure(560.0, 660.0, 560.0, 60.0).with(
            &xui::XUI_FIGURE,
            "Fill",
            fill(vec![
                ("FillType", V::Unsigned(3)),
                (
                    "Gradient",
                    gradient(
                        true,
                        &[(0xffff_ffff, 0.2), (0xff00_8000, 0.6), (0x0000_0000, 1.0)],
                    ),
                ),
            ]),
        ));

    insta::assert_snapshot!(render_scene(
        registry,
        &scene,
        MapProvider::default(),
        56,
        36
    ));
}

/// A multiply blended white gradient fading to alpha 0 changes no color:
/// multiply reads the straight texel color, white at every stop.
#[test]
fn render_multiply_gradient_keeps_transparent_stop_color() {
    let background = || {
        sized("XuiCanvas", 1120.0, 770.0).child(figure(1120.0, 770.0, 0.0, 0.0).with(
            &xui::XUI_FIGURE,
            "Fill",
            fill(vec![
                ("FillType", V::Unsigned(1)),
                ("FillColor", V::Color(0xff20_4060)),
            ]),
        ))
    };
    let faded = background().child(
        figure(1120.0, 770.0, 0.0, 0.0)
            .with(
                &xui::XUI_FIGURE,
                "Fill",
                fill(vec![
                    ("FillType", V::Unsigned(2)),
                    (
                        "Gradient",
                        gradient(false, &[(0x00ff_ffff, 0.3), (0xffff_ffff, 0.7)]),
                    ),
                ]),
            )
            .with(&xui::XUI_ELEMENT, "BlendMode", V::Unsigned(2)),
    );

    assert_eq!(
        render_scene(registry(), &faded, MapProvider::default(), 56, 36),
        render_scene(registry(), &background(), MapProvider::default(), 56, 36)
    );
}

#[test]
fn render_images_size_modes_and_rotated_clip() {
    let registry = registry();
    let checker: Vec<[u8; 4]> = (0..16)
        .map(|index| {
            if (index % 4 + index / 4) % 2 == 0 {
                [255, 255, 255, 255]
            } else {
                [255, 0, 0, 255]
            }
        })
        .collect();
    let mut provider = MapProvider::default();
    provider
        .files
        .insert(units("checker.png"), encode_png(4, 4, &checker));
    let image = |mode: u32, x: f32| {
        sized("XuiImage", 300.0, 150.0)
            .with(&xui::XUI_ELEMENT, "Position", V::Vector([x, 80.0, 0.0]))
            .with(&xui::XUI_IMAGE, "ImagePath", V::Str("checker.png"))
            .with(&xui::XUI_IMAGE, "SizeMode", V::Unsigned(mode))
            .with(&xui::XUI_IMAGE, "BrushFlags", V::Unsigned(0x30))
    };
    let degrees_30 = [0.0, 0.0, 0.258_819, 0.965_926];
    let scene = sized("XuiCanvas", 1120.0, 770.0)
        .child(image(4, 20.0))
        .child(image(0x10, 400.0))
        .child(image(0, 780.0))
        .child(
            sized("XuiGroup", 300.0, 200.0)
                .with(
                    &xui::XUI_ELEMENT,
                    "Position",
                    V::Vector([450.0, 380.0, 0.0]),
                )
                .with(&xui::XUI_ELEMENT, "Rotation", V::Quaternion(degrees_30))
                .with(&xui::XUI_ELEMENT, "ClipChildren", V::Bool(true))
                .child(figure(600.0, 400.0, -150.0, -100.0).with(
                    &xui::XUI_FIGURE,
                    "Fill",
                    fill(vec![
                        ("FillType", V::Unsigned(1)),
                        ("FillColor", V::Color(0xff00_ffff)),
                    ]),
                )),
        );

    insta::assert_snapshot!(render_scene(registry, &scene, provider, 56, 36));
}

#[test]
fn release_textures_destroys_every_texture() {
    let registry = registry();
    let mut provider = MapProvider::default();
    provider.files.insert(
        units("white.png"),
        encode_png(1, 1, &[[255, 255, 255, 255]]),
    );
    let scene = sized("XuiCanvas", 1120.0, 720.0)
        .child(sized("XuiImage", 100.0, 100.0).with(
            &xui::XUI_IMAGE,
            "ImagePath",
            V::Str("white.png"),
        ))
        .child(
            sized("XuiText", 400.0, 100.0)
                .with(&xui::XUI_TEXT, "Text", V::Str("IOH"))
                .with(&xui::XUI_TEXT, "TextColor", V::Color(0xffff_ffff))
                .with(&xui::XUI_TEXT, "PointSize", V::Float(20.0)),
        )
        .child(figure(100.0, 100.0, 0.0, 200.0).with(
            &xui::XUI_FIGURE,
            "Fill",
            fill(vec![
                ("FillType", V::Unsigned(2)),
                (
                    "Gradient",
                    gradient(false, &[(0xffff_0000, 0.0), (0xff00_00ff, 1.0)]),
                ),
            ]),
        ));
    add_scene_section(&registry, &scene, &mut provider);

    let display = DisplaySize {
        width: 1120,
        height: 720,
    };
    let mut ui = Ui::new(registry, &provider, display);
    *ui.fonts_mut() = crate::text::tests::library();
    let root = ui
        .load_scene("section://0,test#", "scene.xur")
        .expect("scene loads");
    let canvas = ui.root();
    ui.tree_mut().append_child(canvas, root).expect("attach");
    let pixel = || Image::new(NonZeroU32::MIN, NonZeroU32::MIN, vec![Rgba8::WHITE]).unwrap();
    let held = ui.runtime().create_texture_object(pixel());
    let dropped = ui.runtime().create_texture_object(pixel());

    let mut store = CountingStore::default();
    let mut list = DrawList::new();
    ui.draw(&mut store, &mut list).expect("draw");
    held.texture(&mut store).expect("upload");
    dropped.texture(&mut store).expect("upload");
    drop(dropped);
    // Image, glyph page, gradient and both texture objects.
    assert_eq!(store.live, 5);

    ui.release_textures(&mut store);
    assert_eq!(store.live, 0);
    assert_eq!(held.texture(&mut store), Ok(None));

    // A later draw loads the image, glyphs and gradient again.
    ui.draw(&mut store, &mut list).expect("draw");
    assert_eq!(store.live, 3);
    drop(held);
    ui.release_textures(&mut store);
    assert_eq!(store.live, 0);
}

#[test]
fn free_resources_flushes_textures_no_element_draws() {
    let registry = registry();
    let mut provider = MapProvider::default();
    provider.files.insert(
        units("white.png"),
        encode_png(1, 1, &[[255, 255, 255, 255]]),
    );
    provider
        .files
        .insert(units("red.png"), encode_png(1, 1, &[[255, 0, 0, 255]]));
    let image = |path: &'static str| {
        sized("XuiImage", 100.0, 100.0).with(&xui::XUI_IMAGE, "ImagePath", V::Str(path))
    };
    let scene = sized("XuiCanvas", 1120.0, 720.0)
        .child(sized("XuiGroup", 100.0, 100.0).child(image("white.png")))
        .child(image("red.png"));
    add_scene_section(&registry, &scene, &mut provider);

    let display = DisplaySize {
        width: 1120,
        height: 720,
    };
    let mut ui = Ui::new(registry, &provider, display);
    let root = ui
        .load_scene("section://0,test#", "scene.xur")
        .expect("scene loads");
    let canvas = ui.root();
    ui.tree_mut().append_child(canvas, root).expect("attach");
    let children: Vec<ElementId> = ui.tree().children(root).collect();
    let [group, kept] = children[..] else {
        panic!("scene has a group and an image");
    };
    let freed = ui.tree().children(group).next().expect("image in group");
    let texture = |ui: &Ui<'_>, image: ElementId| {
        ui.tree()
            .image_content(image)
            .and_then(|image| image.texture)
            .map(|texture| texture.id)
    };

    let mut store = CountingStore::default();
    let mut list = DrawList::new();
    ui.draw(&mut store, &mut list).expect("draw");
    let freed_before = texture(&ui, freed);
    let kept_before = texture(&ui, kept);
    assert!(freed_before.is_some() && kept_before.is_some());
    assert_eq!(ui.resources().textures().len(), 2);

    // The image in the subtree drops its texture on 0x28, so the flush
    // forgets it; the image outside keeps its own.
    ui.context()
        .free_resources(Some(group), crate::message::ResourceMask::TEXTURES);
    assert_eq!(texture(&ui, freed), None);
    assert_eq!(texture(&ui, kept), kept_before);
    assert_eq!(ui.resources().textures().len(), 1);

    // The next draw destroys the flushed texture and loads the path again.
    ui.draw(&mut store, &mut list).expect("draw");
    assert_eq!(store.live, 2);
    assert!(texture(&ui, freed).is_some());
    assert_ne!(texture(&ui, freed), freed_before);
    assert_eq!(texture(&ui, kept), kept_before);
}

#[test]
fn num_stops_does_not_allocate_unset_stops() {
    let registry = registry();
    let gradient = member(def(&xui::XUI_FIGURE, "Fill"), "Gradient");
    let ramp = [(0xff00_0000, 0.0), (0xffff_ffff, 1.0)];
    let scene = sized("XuiCanvas", 100.0, 100.0).child(figure(10.0, 10.0, 0.0, 0.0).with(
        &xui::XUI_FIGURE,
        "Fill",
        fill(vec![
            ("FillType", V::Unsigned(2)),
            (
                "Gradient",
                V::Compound(vec![
                    prop(member(gradient, "NumStops"), vec![V::Integer(i32::MAX)]),
                    prop(
                        member(gradient, "StopColor"),
                        ramp.iter().map(|(color, _)| V::Color(*color)).collect(),
                    ),
                    prop(
                        member(gradient, "StopPos"),
                        ramp.iter().map(|(_, position)| V::Float(*position)).collect(),
                    ),
                ]),
            ),
        ]),
    ));

    let (tree, root) = instantiate_scene(&registry, &scene).expect("scene instantiates");
    let figure = tree.children(root).next().expect("figure");
    let Some(crate::scene::content::Content::Figure(figure)) =
        tree.get(figure).map(|element| &element.content)
    else {
        panic!("figure content");
    };
    let stops = &figure.fill.stops;
    let zero = GradientStop {
        color: exui_core::value::Color::from_argb(0),
        position: 0.0,
    };

    assert_eq!(stops.len(), i32::MAX as usize);
    assert_eq!(
        stops.get(1),
        Some(GradientStop {
            color: exui_core::value::Color::from_argb(0xffff_ffff),
            position: 1.0,
        })
    );
    assert_eq!(stops.get(1000), Some(zero));
    assert_eq!(stops.get(i32::MAX as usize), None);
    // The walk never reaches past the stop at 1.0.
    let two = GradientStops::from_stops(stops.get(0).into_iter().chain(stops.get(1)).collect());
    assert_eq!(gradient_texels(stops), gradient_texels(&two));
}

#[test]
fn large_concave_outlines_are_not_ear_clipped() {
    // A star of straight segments: each control point is its segment start.
    let star = |tips: usize| -> Vec<FigurePoint> {
        (0..tips * 2)
            .map(|index| {
                let angle = index as f32 * core::f32::consts::PI / tips as f32;
                let radius = if index % 2 == 0 { 400.0 } else { 200.0 };
                let (x, y) = (
                    500.0 + radius * libm::cosf(angle),
                    500.0 + radius * libm::sinf(angle),
                );
                FigurePoint {
                    coords: [x, y, x, y, x, y],
                }
            })
            .collect()
    };
    let size = Size {
        width: 1000.0,
        height: 1000.0,
    };
    let mesh = |points: Vec<FigurePoint>| {
        let mut figure = crate::scene::figure::Figure::new();
        figure.points = points;
        figure.design_size = Some(size);
        let mesh = figure.mesh(size);
        (mesh.convex, mesh.points.len(), mesh.fill.len())
    };

    assert_eq!(mesh(star(5)), (false, 11, 8));
    // 1100 points and the closing point: past the limit, the fill is empty
    // and the stroke keeps every point.
    assert_eq!(mesh(star(550)), (false, 1101, 0));
}

#[test]
fn control_point_size_minus_one_selects_the_presenter_size() {
    let registry = registry();
    let scene = sized("XuiCanvas", 100.0, 100.0)
        .child(Obj::new("XuiControl"))
        .child(Obj::new("XuiControl").with(&xui::XUI_CONTROL, "PointSize", V::Float(24.0)));

    let (tree, root) = instantiate_scene(&registry, &scene).expect("scene instantiates");
    let point_size = def(&xui::XUI_CONTROL, "PointSize");
    let sizes: Vec<_> = tree
        .children(root)
        .map(|control| {
            let stored = tree
                .get(control)
                .and_then(|element| element.control.as_ref())
                .and_then(|control| control.point_size);
            (stored, tree.property(control, &[point_size], None))
        })
        .collect();

    assert_eq!(
        sizes,
        [
            (None, Some(PropValue::Float(-1.0))),
            (Some(24.0), Some(PropValue::Float(24.0))),
        ]
    );
}

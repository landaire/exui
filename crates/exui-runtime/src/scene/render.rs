//! Render traversal (doc/xui-render.md sections 5, 6, 8 to 10; text in
//! doc/xui-text.md section 5).

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::num::NonZeroU32;

use xbox360_draw::draw::AddressMode;
use xbox360_draw::draw::BlendMode;
use xbox360_draw::draw::ColorWriteMask;
use xbox360_draw::draw::DrawList;
use xbox360_draw::draw::Filter;
use xbox360_draw::draw::Rgba8;
use xbox360_draw::draw::Sampler;
use xbox360_draw::draw::TextureMapping;
use xbox360_draw::image::Image;
use xbox360_draw::texture::TextureStore;

use crate::error::FrameError;
use crate::math::Matrix4;
use crate::math::unit_to_byte;
use crate::resource::CachedTexture;
use crate::resource::Resources;
use crate::resource::TextureObject;
use crate::scene::BasePathId;
use crate::scene::ContentKind;
use crate::scene::Element;
use crate::scene::ElementFlags;
use crate::scene::ElementId;
use crate::scene::ElementTree;
use crate::scene::PresenterSource;
use crate::scene::Size;
use crate::scene::SourceImage;
use crate::scene::XuiBlendMode;
use crate::scene::content::Content;
use crate::scene::content::is_scene_path;
use crate::scene::effect::EffectTarget;
use crate::scene::effect::STRIP_TRIANGLES;
use crate::scene::effect::TargetSize;
use crate::scene::figure::FillType;
use crate::scene::figure::GRADIENT_TEXELS;
use crate::scene::figure::fill_transform;
use crate::scene::figure::gradient_texels;
use crate::scene::figure::strip_triangles;
use crate::scene::figure::stroke_strip;
use crate::scene::image::CustomImage;
use crate::scene::image::QUAD_TRIANGLES;
use crate::scene::image::image_layout;
use crate::scene::image::nine_grid;
use crate::scene::paint::Brush;
use crate::scene::paint::DeviceState;
use crate::scene::paint::LocalVertex;
use crate::scene::paint::PaintColor;
use crate::scene::paint::Painter;
use crate::scene::paint::TextureFlags;
use crate::scene::paint::modulate;
use crate::scene::paint::sampler_from_flags;
use crate::scene::paint::straight_rgba;
use crate::text::FontLibrary;
use crate::text::GlyphQuad;
use crate::text::TextError;
use crate::text::TextExtent;
use crate::text::TextRenderer;
use crate::text::TextRequest;
use crate::text::TextStyle;

/// Everything a draw needs besides the tree.
pub struct RenderContext<'a, 'p> {
    pub resources: &'a mut Resources<'p>,
    pub fonts: &'a FontLibrary,
    pub text: &'a mut TextRenderer,
    pub textures: &'a mut dyn TextureStore,
    pub presenters: &'a mut dyn PresenterSource,
}

/// Render data passed down the tree (`rd` of the original) with the world
/// matrix of the element that pushed it.
#[derive(Clone, Copy, Debug)]
struct RenderState {
    world: Matrix4,
    /// Straight `0xAARRGGBB`.
    color_factor: u32,
    /// Last non-inherit BlendMode value, including values above 6.
    blend_value: XuiBlendMode,
    device: DeviceState,
}

impl RenderState {
    /// The render data both passes start from (0x9210d300, 0x9210d370) on a
    /// reset DC: opaque white, normal blending, all channels, no clip
    /// planes.
    const RESET: Self = Self {
        world: Matrix4::IDENTITY,
        color_factor: 0xffff_ffff,
        blend_value: XuiBlendMode::Normal,
        device: DeviceState {
            blend: BlendMode::Normal,
            clip: None,
            color_write: ColorWriteMask::ALL,
        },
    };
}

/// Buffers one draw reuses from the previous one.
#[derive(Default)]
pub struct RenderScratch {
    stack: Vec<(ElementId, RenderState)>,
    /// Pass-1 walk: element and whether its children were queued.
    pass1: Vec<(ElementId, bool)>,
    effects: Vec<ElementId>,
    vertices: Vec<LocalVertex>,
    triangles: Vec<[u32; 3]>,
    strip: Vec<[f32; 2]>,
    quads: Vec<GlyphQuad>,
}

/// Draws `root` and its shown descendants in document order. `view` maps
/// the root canvas to the target (the DC view transform). The render
/// targets of EnableEffects elements are drawn first, as target passes.
pub fn render(
    tree: &mut ElementTree,
    root: ElementId,
    view: Matrix4,
    context: &mut RenderContext<'_, '_>,
    scratch: &mut RenderScratch,
    list: &mut DrawList,
) -> Result<(), FrameError> {
    for texture in core::mem::take(&mut tree.released_textures) {
        // The backend may already have dropped it; nothing is left to free.
        let _ = context.textures.destroy_texture(texture);
    }

    // Moved out so the draw functions can borrow the other buffers; they
    // are empty again when the walks end.
    let mut stack = core::mem::take(&mut scratch.stack);
    let mut effects = core::mem::take(&mut scratch.effects);

    // Pass 1 sets the DC view to the inverse of the element's world matrix,
    // so its children are drawn in element units.
    let mut painter = Painter::new(list, Matrix4::IDENTITY);
    effect_elements(tree, root, &mut scratch.pass1, &mut effects);
    for &id in &effects {
        draw_effect_target(tree, id, context, &mut painter, scratch, &mut stack)?;
    }
    effects.clear();
    scratch.effects = effects;

    painter.set_view(view);
    stack.push((root, RenderState::RESET));
    draw_queued(tree, context, &mut painter, scratch, &mut stack)?;
    scratch.stack = stack;

    Ok(())
}

/// Draws the queued elements and their descendants (message 0).
fn draw_queued(
    tree: &mut ElementTree,
    context: &mut RenderContext<'_, '_>,
    painter: &mut Painter<'_>,
    scratch: &mut RenderScratch,
    stack: &mut Vec<(ElementId, RenderState)>,
) -> Result<(), FrameError> {
    while let Some((id, parent_state)) = stack.pop() {
        let Some(element) = tree.get_mut(id) else {
            continue;
        };
        let world = element.local_transform().mul(&parent_state.world);
        let state = push_state(&parent_state, element, world, painter);
        if let Some(custom) = element.custom_image.as_deref() {
            draw_custom_image(custom, &state, context, painter)?;
        }
        let Some(element) = tree.get(id) else {
            continue;
        };

        match element.traits.content {
            ContentKind::None if element.flags.contains(ElementFlags::ENABLE_EFFECTS) => {
                draw_effect(element, &state, painter)?
            }
            ContentKind::None => push_children(tree, id, state, stack),
            ContentKind::Figure => draw_figure(tree, id, &state, context, painter, scratch)?,
            // An image brush's attached scene root is a child of the image.
            ContentKind::Image | ContentKind::ImagePresenter => {
                draw_image(tree, id, &state, context, painter)?;
                push_children(tree, id, state, stack);
            }
            ContentKind::NineGrid => draw_nine_grid(tree, id, &state, context, painter, scratch)?,
            ContentKind::Text | ContentKind::TextPresenter => {
                draw_text(tree, id, &state, context, painter, scratch)?
            }
        }
    }

    Ok(())
}

/// The EnableEffects elements that render pass 1 reaches, in the order it
/// draws their targets: `XuiElement_OnRenderPass1` (0x92164078) renders the
/// children with message 1 first, then the element's own target. Pass 1
/// pushes no render state, so the children filter sees normal blending.
fn effect_elements(
    tree: &ElementTree,
    root: ElementId,
    walk: &mut Vec<(ElementId, bool)>,
    effects: &mut Vec<ElementId>,
) {
    walk.push((root, false));
    while let Some((id, children_queued)) = walk.pop() {
        let Some(element) = tree.get(id) else {
            continue;
        };

        if children_queued {
            if element.flags.contains(ElementFlags::ENABLE_EFFECTS) {
                effects.push(id);
            }
            continue;
        }

        walk.push((id, true));
        let mut child = element.links.last_child;
        while let Some(current) = child {
            let Some(child_element) = tree.get(current) else {
                break;
            };
            if should_render(child_element, XuiBlendMode::Normal) {
                walk.push((current, false));
            }
            child = child_element.links.prev_sibling;
        }
    }
}

/// The effect path of pass 1 (0x92163e28): sizes the element's target to
/// its rounded-up size, then draws its children into it with message 0
/// after pushing the element's own render state.
fn draw_effect_target(
    tree: &mut ElementTree,
    id: ElementId,
    context: &mut RenderContext<'_, '_>,
    painter: &mut Painter<'_>,
    scratch: &mut RenderScratch,
    stack: &mut Vec<(ElementId, RenderState)>,
) -> Result<(), FrameError> {
    let Some(element) = tree.get_mut(id) else {
        return Ok(());
    };
    let used = TargetSize::of_element(element.size);
    let Some(texture) = EffectTarget::prepare(&mut element.effect_target, used, context.textures)?
    else {
        return Ok(());
    };

    let state = push_state(&RenderState::RESET, element, Matrix4::IDENTITY, painter);
    painter.begin_target(texture)?;
    push_children(tree, id, state, stack);
    draw_queued(tree, context, painter, scratch, stack)?;
    painter.end_target();

    Ok(())
}

/// Message 0 of an EnableEffects element (0x92157078): its target drawn
/// with the element's world matrix instead of its children. Nothing is
/// drawn before pass 1 has created a target.
fn draw_effect(
    element: &Element,
    state: &RenderState,
    painter: &mut Painter<'_>,
) -> Result<(), FrameError> {
    let Some(target) = element.effect_target.as_ref() else {
        return Ok(());
    };

    let brush = Brush {
        texture: Some(target.texture),
        sampler: sampler_from_flags(TextureFlags::CLAMP, TextureMapping::Direct),
        color: PaintColor::Argb(state.color_factor),
    };
    painter.draw(
        &state.world,
        state.device,
        &brush,
        &target.quad(),
        &STRIP_TRIANGLES,
    )?;

    Ok(())
}

/// Queues the children of `id` to draw after it, first child on top of the
/// stack.
fn push_children(
    tree: &ElementTree,
    id: ElementId,
    state: RenderState,
    stack: &mut Vec<(ElementId, RenderState)>,
) {
    let Some(element) = tree.get(id) else {
        return;
    };

    let mut child = element.links.last_child;
    while let Some(current) = child {
        let Some(child_element) = tree.get(current) else {
            break;
        };
        if should_render(child_element, state.blend_value) {
            stack.push((current, state));
        }
        child = child_element.links.prev_sibling;
    }
}

/// A [`CustomImage`] with the element's transform and opacity.
fn draw_custom_image(
    custom: &CustomImage,
    state: &RenderState,
    context: &mut RenderContext<'_, '_>,
    painter: &mut Painter<'_>,
) -> Result<(), FrameError> {
    let Some(texture) = custom.texture.texture(context.textures)? else {
        return Ok(());
    };

    let factor = state.color_factor;
    let alpha = unit_to_byte((factor >> 24) as f32 / 255.0 * custom.opacity);
    let brush = Brush {
        texture: Some(texture.id),
        sampler: sampler_from_flags(TextureFlags::CLAMP, TextureMapping::Direct),
        color: PaintColor::Argb((factor & 0x00ff_ffff) | (u32::from(alpha) << 24)),
    };
    let corners = custom.corners.map(|corner| LocalVertex {
        x: corner.x,
        y: corner.y,
        u: corner.u,
        v: corner.v,
    });
    painter.draw(
        &state.world,
        state.device,
        &brush,
        &corners,
        &QUAD_TRIANGLES,
    )?;

    Ok(())
}

/// `XuiElement_RenderChildren` visibility: hidden children are skipped,
/// zero-opacity children only when their effective blend is normal.
fn should_render(child: &Element, blend_value: XuiBlendMode) -> bool {
    if !child.is_shown() {
        return false;
    }
    if child.opacity != 0.0 {
        return true;
    }

    let mode = child.blend_mode;
    mode != XuiBlendMode::Normal
        && (mode != XuiBlendMode::Inherit || blend_value != XuiBlendMode::Normal)
}

/// `XuiElement_PushRenderState`.
fn push_state(
    parent: &RenderState,
    element: &Element,
    world: Matrix4,
    painter: &Painter<'_>,
) -> RenderState {
    let factor = parent.color_factor;
    let alpha = unit_to_byte((factor >> 24) as f32 / 255.0 * element.opacity);
    let mut state = RenderState {
        world,
        color_factor: (factor & 0x00ff_ffff) | (u32::from(alpha) << 24),
        ..*parent
    };

    if element.blend_mode != XuiBlendMode::Inherit {
        state.blend_value = element.blend_mode;
        if let Some(blend) = device_blend(element.blend_mode) {
            state.device.blend = blend;
        }
    }
    // Nested ClipChildren replaces the planes.
    if element.flags.contains(ElementFlags::CLIP_CHILDREN) {
        state.device.clip = Some(painter.clip_planes(&world, element.size));
    }
    // ColorWriteFlags 0 inherits.
    let color_write = element.flags.color_write();
    if color_write != 0 {
        state.device.color_write = ColorWriteMask::from_bits_truncate(color_write as u8);
    }

    state
}

/// `XuiDC_ApplyBlendMode`; values above 6 leave the device unchanged.
fn device_blend(mode: XuiBlendMode) -> Option<BlendMode> {
    match mode {
        XuiBlendMode::Normal => Some(BlendMode::Normal),
        XuiBlendMode::Multiply => Some(BlendMode::Modulate),
        XuiBlendMode::Darken => Some(BlendMode::Min),
        XuiBlendMode::Lighten => Some(BlendMode::Max),
        XuiBlendMode::Additive => Some(BlendMode::Additive),
        XuiBlendMode::Subtract => Some(BlendMode::ReverseSubtract),
        XuiBlendMode::Inherit | XuiBlendMode::Unknown(_) => None,
    }
}

fn base_units(tree: &ElementTree, base: Option<BasePathId>) -> Option<&[u16]> {
    base.map(|base| tree.base_path_units(base))
}

/// Loads an image through the texture loader. A failure leaves the brush
/// empty and is retried on the next draw, as in the original. Scene paths
/// are XuiImage scene content, loaded by the XuiImage level instead.
fn load_texture(
    tree: &ElementTree,
    context: &mut RenderContext<'_, '_>,
    path: &[u16],
    base: Option<BasePathId>,
) -> Option<CachedTexture> {
    if is_scene_path(path) {
        return None;
    }

    context
        .resources
        .texture(path, base_units(tree, base), context.textures)
        .ok()
}

fn texture_size(texture: CachedTexture) -> Size {
    Size {
        width: texture.width.get() as f32,
        height: texture.height.get() as f32,
    }
}

fn draw_figure(
    tree: &mut ElementTree,
    id: ElementId,
    state: &RenderState,
    context: &mut RenderContext<'_, '_>,
    painter: &mut Painter<'_>,
    scratch: &mut RenderScratch,
) -> Result<(), FrameError> {
    let fill_texture = {
        let Some(element) = tree.get(id) else {
            return Ok(());
        };
        let Content::Figure(figure) = &element.content else {
            return Ok(());
        };
        match (&figure.fill.texture_path, figure.fill_texture) {
            (Some(path), None) if figure.fill.fill_type == FillType::Texture => {
                load_texture(tree, context, path, element.base_path)
            }
            (_, texture) => texture,
        }
    };

    let Some(element) = tree.get_mut(id) else {
        return Ok(());
    };
    let size = element.size;
    let Content::Figure(figure) = &mut element.content else {
        return Ok(());
    };
    if figure.points.is_empty() {
        return Ok(());
    }
    figure.fill_texture = fill_texture;

    let gradient = figure.fill.fill_type.is_gradient() && figure.fill.stops.len() >= 2;
    let straight = state.device.blend.uses_straight_source();
    if gradient
        && (figure.gradient_dirty
            || figure.gradient_texture.is_none()
            || figure.gradient_straight != straight)
    {
        let texels = gradient_texels(&figure.fill.stops);
        let width = NonZeroU32::new(GRADIENT_TEXELS as u32).unwrap_or(NonZeroU32::MIN);
        // Straight-source blends read the texel color, which a premultiplied
        // texel of alpha 0 no longer has (a multiply fade drawing black), and
        // in a target pass Modulate also multiplies the target alpha by the
        // texel alpha.
        let image = if straight {
            let pixels = texels.iter().map(|&argb| straight_rgba(argb)).collect();
            Image::new_straight(width, NonZeroU32::MIN, pixels)
        } else {
            let pixels = texels
                .iter()
                .map(|&argb| Rgba8::from_argb_straight(argb))
                .collect();
            Image::new(width, NonZeroU32::MIN, pixels)
        };
        let Ok(image) = image else {
            // 128 pixels always fill a 128x1 image.
            return Ok(());
        };
        match figure.gradient_texture {
            Some(texture) => context.textures.update_texture(texture, image)?,
            None => figure.gradient_texture = Some(context.textures.create_texture(image)?),
        }
        figure.gradient_dirty = false;
        figure.gradient_straight = straight;
    }

    let fill_brush = match figure.fill.fill_type {
        FillType::Solid => Some((
            Brush {
                texture: None,
                sampler: Sampler::default(),
                color: PaintColor::Argb(modulate(figure.fill.color.argb(), state.color_factor)),
            },
            None,
        )),
        FillType::GradientMinSide | FillType::GradientMaxSide if gradient => {
            let mapping = if figure.fill.radial {
                TextureMapping::Radial
            } else {
                TextureMapping::Direct
            };
            let sampler = Sampler {
                filter: Filter::Linear,
                address_u: AddressMode::Clamp,
                address_v: AddressMode::Clamp,
                mapping,
            };
            let transform = fill_transform(&figure.fill, size, None);
            Some((
                Brush {
                    texture: figure.gradient_texture,
                    sampler,
                    color: PaintColor::Argb(state.color_factor),
                },
                Some(transform),
            ))
        }
        FillType::Texture => figure.fill_texture.map(|texture| {
            let sampler = sampler_from_flags(figure.fill.texture_flags(), TextureMapping::Direct);
            let transform = fill_transform(&figure.fill, size, Some(texture_size(texture)));
            (
                Brush {
                    texture: Some(texture.id),
                    sampler,
                    color: PaintColor::Argb(state.color_factor),
                },
                Some(transform),
            )
        }),
        _ => None,
    };
    let stroke_width = figure.stroke_width;
    let stroke_color = figure.stroke_color.argb();
    let closed = figure.closed;
    let mesh = figure.mesh(size);

    if let Some((brush, transform)) = fill_brush {
        scratch.vertices.clear();
        scratch.vertices.extend(mesh.points.iter().map(|&[x, y]| {
            let [u, v] = transform.map_or([0.0, 0.0], |matrix| {
                let [u, v, _, _] = matrix.transform(x, y, 0.0);
                [u, v]
            });
            LocalVertex { x, y, u, v }
        }));
        painter.draw(
            &state.world,
            state.device,
            &brush,
            &scratch.vertices,
            &mesh.fill,
        )?;
    }

    if stroke_width >= 1.0 {
        stroke_strip(
            &mesh.points,
            mesh.open_count,
            stroke_width,
            closed,
            &mut scratch.strip,
        );
        scratch.vertices.clear();
        scratch
            .vertices
            .extend(scratch.strip.iter().map(|&[x, y]| LocalVertex {
                x,
                y,
                u: 0.0,
                v: 0.0,
            }));
        strip_triangles(scratch.vertices.len(), &mut scratch.triangles);
        let brush = Brush {
            texture: None,
            sampler: Sampler::default(),
            color: PaintColor::Argb(modulate(stroke_color, state.color_factor)),
        };
        painter.draw(
            &state.world,
            state.device,
            &brush,
            &scratch.vertices,
            &scratch.triangles,
        )?;
    }

    Ok(())
}

fn draw_image(
    tree: &mut ElementTree,
    id: ElementId,
    state: &RenderState,
    context: &mut RenderContext<'_, '_>,
    painter: &mut Painter<'_>,
) -> Result<(), FrameError> {
    let presenter = tree
        .get(id)
        .is_some_and(|element| element.traits.content == ContentKind::ImagePresenter);
    let ImageTexture { texture, loaded } = if presenter {
        presenter_texture(tree, id, context)
    } else {
        let Some(element) = tree.get(id) else {
            return Ok(());
        };
        let Content::Image(image) = &element.content else {
            return Ok(());
        };
        let texture = match (&image.path, image.texture) {
            (Some(path), None) => load_texture(tree, context, path, element.base_path),
            (_, texture) => texture,
        };
        ImageTexture {
            texture,
            loaded: Loaded::Keep,
        }
    };

    let Some(element) = tree.get_mut(id) else {
        return Ok(());
    };
    let size = element.size;
    let Content::Image(image) = &mut element.content else {
        return Ok(());
    };
    image.texture = texture;
    if let Loaded::Set { path, object } = loaded {
        image.loaded_path = path;
        image.object = object;
    }

    let Some(texture) = image.texture else {
        return Ok(());
    };
    let Some(layout) = image_layout(image.size_mode, size, texture_size(texture)) else {
        return Ok(());
    };
    let sampler = sampler_from_flags(
        image.brush_flags | TextureFlags::CLAMP,
        TextureMapping::Direct,
    );
    let brush = Brush {
        texture: Some(texture.id),
        sampler,
        color: PaintColor::Argb(state.color_factor),
    };
    painter.draw(
        &state.world,
        state.device,
        &brush,
        &layout.quad(),
        &QUAD_TRIANGLES,
    )?;

    Ok(())
}

struct ImageTexture {
    texture: Option<CachedTexture>,
    loaded: Loaded,
}

/// Update of the source an image presenter last loaded.
enum Loaded {
    Keep,
    Set {
        path: Option<Box<[u16]>>,
        object: Option<TextureObject>,
    },
}

/// `XuiImagePresenter_UpdateFromSource`: reloads when the source path or
/// texture object differs from the loaded one.
fn presenter_texture(
    tree: &ElementTree,
    id: ElementId,
    context: &mut RenderContext<'_, '_>,
) -> ImageTexture {
    let unloaded = ImageTexture {
        texture: None,
        loaded: Loaded::Set {
            path: None,
            object: None,
        },
    };
    let Some(Content::Image(image)) = tree.get(id).map(|element| &element.content) else {
        return ImageTexture {
            texture: None,
            loaded: Loaded::Keep,
        };
    };
    let Some(source) = context.presenters.source_image(tree, id) else {
        return unloaded;
    };

    let (path, base) = match source {
        SourceImage::Texture(object) => {
            if image.texture.is_some() && image.object.as_ref() == Some(&object) {
                return ImageTexture {
                    texture: image.texture,
                    loaded: Loaded::Keep,
                };
            }
            // A failed upload draws nothing, like a failed path load.
            let Ok(Some(texture)) = object.texture(context.textures) else {
                return unloaded;
            };
            return ImageTexture {
                texture: Some(texture),
                loaded: Loaded::Set {
                    path: None,
                    object: Some(object),
                },
            };
        }
        SourceImage::Path { path, base } => (path, base),
    };
    if image.texture.is_some()
        && image.object.is_none()
        && image.loaded_path.as_deref() == Some(&*path)
    {
        return ImageTexture {
            texture: image.texture,
            loaded: Loaded::Keep,
        };
    }

    let path: Box<[u16]> = path.into_owned().into_boxed_slice();
    match load_texture(tree, context, &path, base) {
        Some(texture) => ImageTexture {
            texture: Some(texture),
            loaded: Loaded::Set {
                path: Some(path),
                object: None,
            },
        },
        None => unloaded,
    }
}

fn draw_nine_grid(
    tree: &mut ElementTree,
    id: ElementId,
    state: &RenderState,
    context: &mut RenderContext<'_, '_>,
    painter: &mut Painter<'_>,
    scratch: &mut RenderScratch,
) -> Result<(), FrameError> {
    let texture = {
        let Some(element) = tree.get(id) else {
            return Ok(());
        };
        let Content::NineGrid(grid) = &element.content else {
            return Ok(());
        };
        match (&grid.path, grid.texture) {
            (Some(path), None) => load_texture(tree, context, path, element.base_path),
            (_, texture) => texture,
        }
    };

    let Some(element) = tree.get_mut(id) else {
        return Ok(());
    };
    let size = element.size;
    let Content::NineGrid(grid) = &mut element.content else {
        return Ok(());
    };
    grid.texture = texture;
    let Some(texture) = texture else {
        return Ok(());
    };

    let offsets = [grid.left, grid.top, grid.right, grid.bottom];
    nine_grid(
        size,
        texture_size(texture),
        offsets,
        grid.no_center,
        &mut scratch.vertices,
        &mut scratch.triangles,
    );
    let brush = Brush {
        texture: Some(texture.id),
        sampler: sampler_from_flags(TextureFlags::CLAMP, TextureMapping::Direct),
        color: PaintColor::Argb(state.color_factor),
    };
    painter.draw(
        &state.world,
        state.device,
        &brush,
        &scratch.vertices,
        &scratch.triangles,
    )?;

    Ok(())
}

fn draw_text(
    tree: &mut ElementTree,
    id: ElementId,
    state: &RenderState,
    context: &mut RenderContext<'_, '_>,
    painter: &mut Painter<'_>,
    scratch: &mut RenderScratch,
) -> Result<(), FrameError> {
    let tree: &ElementTree = tree;
    let Some(element) = tree.get(id) else {
        return Ok(());
    };
    let Content::Text(props) = &element.content else {
        return Ok(());
    };

    let RenderContext {
        fonts,
        text: renderer,
        textures,
        presenters,
        ..
    } = context;
    let source = if element.traits.content == ContentKind::TextPresenter {
        presenters.source_text(tree, id)
    } else {
        props.text.as_deref().map(|text| crate::scene::SourceText {
            text: text.into(),
            point_size: None,
            style: None,
            y_offset: 0.0,
        })
    };
    let Some(source) = source else {
        return Ok(());
    };
    if source.text.is_empty() {
        return Ok(());
    }

    // XuiText_Draw: color factor' = color factor * TextColor.
    let factor = modulate(state.color_factor, props.color.argb());
    let style = source.style.unwrap_or(props.style);
    // [INFERENCE] The shadow alpha is scaled by the text color factor alpha.
    let shadow_color = (style & TextStyle::DROP_SHADOW != 0).then(|| {
        let shadow = props.shadow_color.argb();
        let alpha = ((shadow >> 24) * (factor >> 24) * 2 + 255) / 510;
        Rgba8::from_argb_straight((shadow & 0x00ff_ffff) | (alpha << 24))
    });
    let request = TextRequest {
        text: &source.text,
        typeface: props.font.as_deref(),
        point_size: source.point_size.unwrap_or(props.point_size),
        style: TextStyle(style),
        color: Rgba8::from_argb_straight(factor),
        shadow_color,
        line_spacing_adjust: props.line_spacing_adjust,
        box_width: element.size.width,
        box_height: element.size.height,
        scroll: source.y_offset,
    };

    scratch.quads.clear();
    match renderer.layout(fonts, &request, &mut **textures, &mut scratch.quads) {
        Ok(()) => {}
        Err(TextError::Texture(error)) => return Err(error.into()),
        // A font that cannot be created draws nothing, as in the original.
        Err(_) => return Ok(()),
    }

    let sampler = Sampler {
        filter: Filter::Linear,
        address_u: AddressMode::Clamp,
        address_v: AddressMode::Clamp,
        mapping: TextureMapping::Direct,
    };
    for quad in &scratch.quads {
        let [u0, v0, u1, v1] = quad.uv;
        let corners = [
            LocalVertex {
                x: quad.left,
                y: quad.top,
                u: u0,
                v: v0,
            },
            LocalVertex {
                x: quad.right,
                y: quad.top,
                u: u1,
                v: v0,
            },
            LocalVertex {
                x: quad.right,
                y: quad.bottom,
                u: u1,
                v: v1,
            },
            LocalVertex {
                x: quad.left,
                y: quad.bottom,
                u: u0,
                v: v1,
            },
        ];
        let brush = Brush {
            texture: Some(quad.texture),
            sampler,
            color: PaintColor::Premultiplied(quad.color),
        };
        painter.draw(
            &state.world,
            state.device,
            &brush,
            &corners,
            &QUAD_TRIANGLES,
        )?;
    }

    Ok(())
}

/// `XuiText` measure (0x9214F550): `text` in the element's own font, point
/// size and style, wrapped at its width. `None` for elements without text
/// properties and fonts that cannot be created.
pub(crate) fn measure_text(
    tree: &ElementTree,
    id: ElementId,
    text: &[u16],
    fonts: &FontLibrary,
    renderer: &mut TextRenderer,
) -> Option<TextExtent> {
    let request = own_text_request(tree, id, text)?;

    renderer.measure(fonts, &request).ok()
}

/// Font and box metrics XuiEdit reads from its `Text` presenter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextLineMetrics {
    /// `XuiFontGetLineHeight` 0x92180208: the font line height rounded up.
    pub(crate) line_height: u32,
    pub(crate) line_spacing_adjust: i32,
    pub(crate) style: TextStyle,
    pub(crate) box_width: f32,
    pub(crate) box_height: f32,
}

/// `XuiFontGetLineStarts` as `XuiEdit_WrapLines` (0x9215ea08) calls it on
/// its `Text` presenter: the start of every line of `text` in the element's
/// own font, point size and style at its width. `None` for elements without
/// text properties and fonts that cannot be created.
pub(crate) fn text_line_starts(
    tree: &ElementTree,
    id: ElementId,
    text: &[u16],
    fonts: &FontLibrary,
    renderer: &mut TextRenderer,
    out: &mut Vec<usize>,
) -> Option<TextLineMetrics> {
    let request = own_text_request(tree, id, text)?;
    let line_height = renderer.line_starts(fonts, &request, out).ok()?;

    Some(TextLineMetrics {
        line_height,
        line_spacing_adjust: request.line_spacing_adjust,
        style: request.style,
        box_width: request.box_width,
        box_height: request.box_height,
    })
}

/// A measure request of XuiEdit: `text` in the font of its `Text`
/// presenter with the style and LineSpacingAdjust read at its last layout,
/// in a rect at the origin.
#[derive(Clone, Copy, Debug)]
pub(crate) struct EditMeasure<'a> {
    pub(crate) text: &'a [u16],
    /// Code units to measure; `None` measures the whole text (count -1).
    pub(crate) count: Option<usize>,
    pub(crate) style: TextStyle,
    pub(crate) line_spacing_adjust: i32,
    pub(crate) width: f32,
    pub(crate) height: f32,
}

/// Right and bottom edges of the measured rect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextBounds {
    pub(crate) right: f32,
    pub(crate) bottom: f32,
}

/// The measure wrapper 0x92183eb8 over `XuiMeasureText` 0x92183be8. An
/// empty text gives an empty rect. A count of 0 and a negative width
/// without style 0x2000 leave the rect as given in integers; the 0x2000
/// measurer gives an empty rect for a width below 1. `None` for elements
/// without text properties and fonts that cannot be created.
pub(crate) fn edit_text_bounds(
    tree: &ElementTree,
    id: ElementId,
    measure: &EditMeasure<'_>,
    fonts: &FontLibrary,
    renderer: &mut TextRenderer,
) -> Option<TextBounds> {
    const EMPTY: TextBounds = TextBounds {
        right: 0.0,
        bottom: 0.0,
    };

    let mut request = own_text_request(tree, id, measure.text)?;
    request.style = measure.style;
    request.line_spacing_adjust = measure.line_spacing_adjust;
    request.box_width = measure.width;
    request.box_height = measure.height;
    let limit = measure.count.unwrap_or(measure.text.len());
    let extent = renderer.measure_within(fonts, &request, limit).ok()?;

    // The wrapper converts the rect to integers by truncation.
    let width = measure.width as i32;
    let given = TextBounds {
        right: width as f32,
        bottom: (measure.height as i32) as f32,
    };
    let bounds = if measure.text.first().is_none_or(|code| *code == 0) {
        EMPTY
    } else if measure.count == Some(0) {
        given
    } else if measure.style.contains(TextStyle::MULTI_LINE_MEASURE) {
        if width > 0 {
            TextBounds {
                right: extent.width,
                bottom: extent.height,
            }
        } else {
            EMPTY
        }
    } else if width < 0 {
        given
    } else {
        TextBounds {
            right: extent.width,
            bottom: extent.height,
        }
    };

    Some(bounds)
}

fn own_text_request<'a>(
    tree: &'a ElementTree,
    id: ElementId,
    text: &'a [u16],
) -> Option<TextRequest<'a>> {
    let element = tree.get(id)?;
    let Content::Text(props) = &element.content else {
        return None;
    };

    Some(TextRequest {
        text,
        typeface: props.font.as_deref(),
        point_size: props.point_size,
        style: TextStyle(props.style),
        color: Rgba8::from_argb_straight(0xffff_ffff),
        shadow_color: None,
        line_spacing_adjust: props.line_spacing_adjust,
        box_width: element.size.width,
        box_height: element.size.height,
        scroll: 0.0,
    })
}

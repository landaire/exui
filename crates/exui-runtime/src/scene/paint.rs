//! Turns element-local primitives into draw list triangles: world, view and
//! projection on the CPU, clip planes, and vertex colors per blend mode.
//!
//! The XUI vertex shader computes `v * World * View * P * InvViewport` with
//! `P` the identity except `m[2][3] = 0.001`. After the hardware divides by
//! w that is `(x / w, y / w)` in target pixels, where `(x, y, z, w0) =
//! v * World * View` and `w = w0 + 0.001 * z`. `draw::Vertex` is 2D, so the
//! division happens here and texture coordinates are interpolated affinely
//! by the backends instead of perspective-correct as on Xenos.

use alloc::vec::Vec;

use xbox360_draw::draw::AddressMode;
use xbox360_draw::draw::BlendMode;
use xbox360_draw::draw::ClipRect;
use xbox360_draw::draw::ColorWriteMask;
use xbox360_draw::draw::DrawList;
use xbox360_draw::draw::DrawState;
use xbox360_draw::draw::Filter;
use xbox360_draw::draw::Rgba8;
use xbox360_draw::draw::Sampler;
use xbox360_draw::draw::TextureId;
use xbox360_draw::draw::TextureMapping;
use xbox360_draw::draw::Vertex;
use xbox360_draw::error::DrawListError;

use crate::math::Matrix4;
use crate::scene::Size;

/// A vertex in element-local units with its texture coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LocalVertex {
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) u: f32,
    pub(crate) v: f32,
}

/// Color applied to every vertex of a draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum PaintColor {
    /// Straight `0xAARRGGBB`, the shader output color.
    Argb(u32),
    /// Already premultiplied (glyph quads).
    Premultiplied(Rgba8),
}

/// What a draw samples and with which color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Brush {
    pub(crate) texture: Option<TextureId>,
    pub(crate) sampler: Sampler,
    pub(crate) color: PaintColor,
}

/// Active clip planes in target pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Clip {
    /// Axis-aligned planes: the rasterizer's pixel-center test against the
    /// plane positions equals this rectangle.
    Rect(ClipRect),
    /// Four planes through the corners of a convex quad, in order.
    Quad([[f32; 2]; 4]),
    /// Degenerate or behind the viewer: nothing passes [INFERENCE].
    Empty,
}

/// Device state of a draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DeviceState {
    pub(crate) blend: BlendMode,
    pub(crate) clip: Option<Clip>,
    pub(crate) color_write: ColorWriteMask,
}

#[derive(Clone, Copy, Debug)]
struct Projected {
    position: [f32; 2],
    uv: [f32; 2],
    /// w > 0; geometry at or behind the eye plane is dropped [INFERENCE].
    visible: bool,
}

/// Draw list writer with reusable buffers.
pub(crate) struct Painter<'l> {
    list: &'l mut DrawList,
    view: Matrix4,
    projected: Vec<Projected>,
    vertices: Vec<Vertex>,
    triangles: Vec<[u32; 3]>,
    polygon: Vec<Projected>,
    scratch: Vec<Projected>,
}

impl<'l> Painter<'l> {
    pub(crate) fn new(list: &'l mut DrawList, view: Matrix4) -> Self {
        Self {
            list,
            view,
            projected: Vec::new(),
            vertices: Vec::new(),
            triangles: Vec::new(),
            polygon: Vec::new(),
            scratch: Vec::new(),
        }
    }

    /// Sets the DC view transform.
    pub(crate) fn set_view(&mut self, view: Matrix4) {
        self.view = view;
    }

    /// Sends the following draws into the render target `texture`.
    pub(crate) fn begin_target(&mut self, texture: TextureId) -> Result<(), DrawListError> {
        self.list.begin_target(texture)
    }

    /// Sends the following draws to the frame.
    pub(crate) fn end_target(&mut self) {
        self.list.end_target();
    }

    fn project(world_view: &Matrix4, x: f32, y: f32) -> ([f32; 2], bool) {
        let [x, y, z, w0] = world_view.transform(x, y, 0.0);
        let w = w0 + 0.001 * z;

        ([x / w, y / w], w > 0.0)
    }

    /// The four ClipChildren planes of an element of `size`.
    pub(crate) fn clip_planes(&self, world: &Matrix4, size: Size) -> Clip {
        let corners = [
            (0.0, 0.0),
            (size.width, 0.0),
            (size.width, size.height),
            (0.0, size.height),
        ];
        let world_view = world.mul(&self.view);
        let mut quad = [[0.0; 2]; 4];
        for (slot, (x, y)) in quad.iter_mut().zip(corners) {
            let (position, visible) = Self::project(&world_view, x, y);
            if !visible {
                return Clip::Empty;
            }
            *slot = position;
        }

        let [a, b, c, d] = quad;
        let aligned = (a[1] == b[1] && c[1] == d[1] && a[0] == d[0] && b[0] == c[0])
            || (a[0] == b[0] && c[0] == d[0] && a[1] == d[1] && b[1] == c[1]);
        if aligned {
            let left = a[0].min(c[0]);
            let right = a[0].max(c[0]);
            let top = a[1].min(c[1]);
            let bottom = a[1].max(c[1]);
            let edge = |value: f32| libm::ceilf(value - 0.5) as i32;
            return Clip::Rect(ClipRect {
                left: edge(left),
                top: edge(top),
                right: edge(right),
                bottom: edge(bottom),
            });
        }
        if signed_area(&quad) == 0.0 {
            return Clip::Empty;
        }

        Clip::Quad(quad)
    }

    /// Appends `triangles` over `local` vertices transformed by `world`.
    pub(crate) fn draw(
        &mut self,
        world: &Matrix4,
        device: DeviceState,
        brush: &Brush,
        local: &[LocalVertex],
        triangles: &[[u32; 3]],
    ) -> Result<(), DrawListError> {
        if matches!(device.clip, Some(Clip::Empty)) {
            return Ok(());
        }

        let world_view = world.mul(&self.view);
        self.projected.clear();
        for vertex in local {
            let (position, visible) = Self::project(&world_view, vertex.x, vertex.y);
            self.projected.push(Projected {
                position,
                uv: [vertex.u, vertex.v],
                visible,
            });
        }

        let color = vertex_color(brush.color, device.blend);
        self.vertices.clear();
        self.triangles.clear();
        let quad = match device.clip {
            Some(Clip::Quad(quad)) => Some(quad),
            _ => None,
        };
        for triangle in triangles {
            let corners = triangle.map(|index| self.projected.get(index as usize).copied());
            let [Some(a), Some(b), Some(c)] = corners else {
                continue;
            };
            if !(a.visible && b.visible && c.visible) {
                continue;
            }

            match quad {
                Some(quad) => self.clip_triangle([a, b, c], &quad, color),
                None => {
                    let base = self.vertices.len() as u32;
                    for corner in [a, b, c] {
                        self.vertices.push(Vertex {
                            position: corner.position,
                            uv: corner.uv,
                            color,
                        });
                    }
                    self.triangles.push([base, base + 1, base + 2]);
                }
            }
        }

        let clip = match device.clip {
            Some(Clip::Rect(rect)) => Some(rect),
            _ => None,
        };
        let state = DrawState {
            texture: brush.texture,
            sampler: brush.sampler,
            blend: device.blend,
            clip,
            color_write: device.color_write,
        };

        self.list.push(state, &self.vertices, &self.triangles)
    }

    /// Sutherland-Hodgman against the four planes, then a fan.
    fn clip_triangle(&mut self, triangle: [Projected; 3], quad: &[[f32; 2]; 4], color: Rgba8) {
        let orientation = signed_area(quad).signum();
        self.polygon.clear();
        self.polygon.extend(triangle);

        for edge in 0..4 {
            let from = quad[edge];
            let to = quad[(edge + 1) % 4];
            let distance = |point: [f32; 2]| {
                orientation
                    * ((to[0] - from[0]) * (point[1] - from[1])
                        - (to[1] - from[1]) * (point[0] - from[0]))
            };

            self.scratch.clear();
            for index in 0..self.polygon.len() {
                let current = self.polygon[index];
                let next = self.polygon[(index + 1) % self.polygon.len()];
                let (d_current, d_next) = (distance(current.position), distance(next.position));
                if d_current >= 0.0 {
                    self.scratch.push(current);
                }
                if (d_current >= 0.0) != (d_next >= 0.0) {
                    let t = d_current / (d_current - d_next);
                    let lerp = |a: [f32; 2], b: [f32; 2]| {
                        [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
                    };
                    self.scratch.push(Projected {
                        position: lerp(current.position, next.position),
                        uv: lerp(current.uv, next.uv),
                        visible: true,
                    });
                }
            }
            core::mem::swap(&mut self.polygon, &mut self.scratch);
            if self.polygon.len() < 3 {
                return;
            }
        }

        let base = self.vertices.len() as u32;
        for corner in &self.polygon {
            self.vertices.push(Vertex {
                position: corner.position,
                uv: corner.uv,
                color,
            });
        }
        for index in 1..self.polygon.len() as u32 - 1 {
            self.triangles.push([base, base + index, base + index + 1]);
        }
    }
}

/// Twice the signed area of a quad.
fn signed_area(quad: &[[f32; 2]; 4]) -> f32 {
    let mut area = 0.0;
    for index in 0..4 {
        let a = quad[index];
        let b = quad[(index + 1) % 4];
        area += a[0] * b[1] - b[0] * a[1];
    }

    area
}

/// Vertex color for `blend`: premultiplied, or straight for the modes that
/// keep the color at zero alpha.
pub(crate) fn vertex_color(color: PaintColor, blend: BlendMode) -> Rgba8 {
    match (color, blend.uses_straight_source()) {
        (PaintColor::Argb(argb), false) => Rgba8::from_argb_straight(argb),
        (PaintColor::Argb(argb), true) => {
            let channel = |shift: u32| ((argb >> shift) & 0xff) as u8;
            Rgba8::new(channel(16), channel(8), channel(0), channel(24))
        }
        (PaintColor::Premultiplied(color), false) => color,
        (PaintColor::Premultiplied(color), true) => {
            let alpha = u32::from(color.alpha());
            let straight = |channel: u8| {
                (u32::from(channel) * 255 + alpha / 2)
                    .checked_div(alpha)
                    .map_or(0, |value| value.min(255) as u8)
            };
            Rgba8::new(
                straight(color.red()),
                straight(color.green()),
                straight(color.blue()),
                color.alpha(),
            )
        }
    }
}

/// Per-channel product of two straight ARGB colors, rounded.
pub(crate) fn modulate(a: u32, b: u32) -> u32 {
    let mut out = 0;
    for shift in [0, 8, 16, 24] {
        let product = ((a >> shift) & 0xff) * ((b >> shift) & 0xff);
        out |= ((2 * product + 255) / 510) << shift;
    }

    out
}

/// A straight `0xAARRGGBB` color as a straight texel.
pub(crate) fn straight_rgba(argb: u32) -> Rgba8 {
    let [alpha, red, green, blue] = argb.to_be_bytes();

    Rgba8::new(red, green, blue, alpha)
}

/// 6-bit DC texture flags value (`XuiDC_SetTextureFlags`): bits 0-1 address
/// U, 2-3 address V, 0x10 point min filter, 0x20 point mag filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) struct TextureFlags(pub(crate) u32);

impl TextureFlags {
    const ADDRESS_MASK: u32 = 0x3;
    const ADDRESS_V_SHIFT: u32 = 2;
    /// Clamp U and V, the flags XuiImage and XuiNineGrid add.
    pub(crate) const CLAMP: Self = Self(0xa);
    pub(crate) const POINT_MAG: Self = Self(0x20);

    pub(crate) fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    fn address_u(self) -> DcAddress {
        DcAddress::from_bits(self.0 & Self::ADDRESS_MASK)
    }

    fn address_v(self) -> DcAddress {
        DcAddress::from_bits((self.0 >> Self::ADDRESS_V_SHIFT) & Self::ADDRESS_MASK)
    }
}

impl core::ops::BitOr for TextureFlags {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Two-bit DC address mode field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
enum DcAddress {
    Wrap = 0,
    Mirror = 1,
    Clamp = 2,
    MirrorOnce = 3,
}

impl DcAddress {
    /// Total over the two-bit field.
    fn from_bits(bits: u32) -> Self {
        match bits {
            0 => Self::Wrap,
            1 => Self::Mirror,
            2 => Self::Clamp,
            _ => Self::MirrorOnce,
        }
    }
}

/// Sampler of a DC texture flags value. The draw list has one filter, taken
/// from the mag bit; mirror-once maps to mirror.
pub(crate) fn sampler_from_flags(flags: TextureFlags, mapping: TextureMapping) -> Sampler {
    let address = |mode: DcAddress| match mode {
        DcAddress::Wrap => AddressMode::Wrap,
        DcAddress::Clamp => AddressMode::Clamp,
        DcAddress::Mirror | DcAddress::MirrorOnce => AddressMode::Mirror,
    };
    let filter = if flags.contains(TextureFlags::POINT_MAG) {
        Filter::Nearest
    } else {
        Filter::Linear
    };

    Sampler {
        filter,
        address_u: address(flags.address_u()),
        address_v: address(flags.address_v()),
        mapping,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modulate_rounds_to_nearest() {
        assert_eq!(modulate(0xffff_ffff, 0x80c0_4001), 0x80c0_4001);
        assert_eq!(modulate(0x8000_0000, 0x8000_0000), 0x4000_0000);
    }

    #[test]
    fn straight_modes_keep_color_at_zero_alpha() {
        let color = PaintColor::Argb(0x00ff_8000);

        assert_eq!(
            vertex_color(color, BlendMode::Modulate),
            Rgba8::new(0xff, 0x80, 0x00, 0)
        );
        assert_eq!(vertex_color(color, BlendMode::Normal), Rgba8::TRANSPARENT);
    }
}

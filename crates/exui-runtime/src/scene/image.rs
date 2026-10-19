//! XuiImage size modes and XuiNineGrid geometry (xui-render.md sections 9
//! and 10).

use alloc::vec::Vec;

use crate::resource::TextureObject;
use crate::scene::Size;
use crate::scene::paint::LocalVertex;

/// Where an image is drawn and how positions map to texture coordinates:
/// `uv = (position - origin) / extent`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ImageLayout {
    /// `[x0, y0, x1, y1]` in element units.
    pub rect: [f32; 4],
    pub origin: [f32; 2],
    pub extent: [f32; 2],
}

impl ImageLayout {
    /// The rectangle as `XuiDC_DrawRect` draws it: two triangles.
    pub(crate) fn quad(&self) -> [LocalVertex; 4] {
        let [x0, y0, x1, y1] = self.rect;
        let vertex = |x: f32, y: f32| LocalVertex {
            x,
            y,
            u: (x - self.origin[0]) / self.extent[0],
            v: (y - self.origin[1]) / self.extent[1],
        };

        [
            vertex(x0, y0),
            vertex(x1, y0),
            vertex(x1, y1),
            vertex(x0, y1),
        ]
    }
}

/// Triangles of [`ImageLayout::quad`].
pub(crate) const QUAD_TRIANGLES: [[u32; 3]; 2] = [[0, 1, 2], [0, 2, 3]];

/// A corner of a [`CustomImage`]: position in element units and texture
/// coordinate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CustomVertex {
    pub x: f32,
    pub y: f32,
    pub u: f32,
    pub v: f32,
}

/// An image a class draws for its element, as a control does from its
/// render message handler with the DC. It is drawn before the element's
/// own content and children, with the element's transform and opacity.
#[derive(Clone, Debug, PartialEq)]
pub struct CustomImage {
    pub texture: TextureObject,
    /// Corners in order around the quad (two triangles 0-1-2, 0-2-3).
    pub corners: [CustomVertex; 4],
    /// Multiplies the element's opacity.
    pub opacity: f32,
}

/// SizeMode property bits of XuiImage and XuiImagePresenter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SizeMode(pub u32);

impl SizeMode {
    pub const NONE: Self = Self(0);
    pub const RESIZE_TO_TEXTURE: Self = Self(0x01);
    pub const CENTER: Self = Self(0x02);
    pub const STRETCH: Self = Self(0x04);
    pub const FIT: Self = Self(0x08);
    pub const FIT_CENTERED: Self = Self(0x10);

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// The fit size of SizeMode 0x08 and 0x10: `content` scaled to the full
/// width of `element` when it is relatively wider, else to the full height.
pub fn fit_size(element: Size, content: Size) -> Size {
    if content.width / content.height > element.width / element.height {
        Size {
            width: element.width,
            height: element.width * content.height / content.width,
        }
    } else {
        Size {
            width: element.height * content.width / content.height,
            height: element.height,
        }
    }
}

/// `XuiImage_ComputeLayout`: SizeMode bits are tested in declaration order
/// of the [`SizeMode`] constants. `None` when the element has no area.
pub fn image_layout(size_mode: SizeMode, element: Size, texture: Size) -> Option<ImageLayout> {
    let Size { width, height } = element;
    let Size {
        width: texture_width,
        height: texture_height,
    } = texture;
    if width == 0.0 || height == 0.0 {
        return None;
    }

    let fitted = || {
        let Size { width, height } = fit_size(element, texture);
        (width, height)
    };

    let layout = if size_mode.contains(SizeMode::RESIZE_TO_TEXTURE) {
        ImageLayout {
            rect: [0.0, 0.0, texture_width, texture_height],
            origin: [0.0, 0.0],
            extent: [texture_width, texture_height],
        }
    } else if size_mode.contains(SizeMode::CENTER) {
        let x0 = width / 2.0 - texture_width / 2.0;
        let y0 = height / 2.0 - texture_height / 2.0;
        let rect = [
            x0.clamp(0.0, width),
            y0.clamp(0.0, height),
            (x0 + texture_width).clamp(0.0, width),
            (y0 + texture_height).clamp(0.0, height),
        ];
        ImageLayout {
            rect,
            origin: [x0, y0],
            extent: [texture_width, texture_height],
        }
    } else if size_mode.contains(SizeMode::STRETCH) {
        ImageLayout {
            rect: [0.0, 0.0, width, height],
            origin: [0.0, 0.0],
            extent: [width, height],
        }
    } else if size_mode.contains(SizeMode::FIT) {
        let (fit_width, fit_height) = fitted();
        ImageLayout {
            rect: [0.0, 0.0, fit_width, fit_height],
            origin: [0.0, 0.0],
            extent: [fit_width, fit_height],
        }
    } else if size_mode.contains(SizeMode::FIT_CENTERED) {
        let (fit_width, fit_height) = fitted();
        let x0 = (width - fit_width) / 2.0;
        let y0 = (height - fit_height) / 2.0;
        ImageLayout {
            rect: [x0, y0, x0 + fit_width, y0 + fit_height],
            origin: [x0, y0],
            extent: [fit_width, fit_height],
        }
    } else {
        let rect = [
            0.0,
            0.0,
            width.min(texture_width),
            height.min(texture_height),
        ];
        ImageLayout {
            rect,
            origin: [0.0, 0.0],
            extent: [texture_width, texture_height],
        }
    };

    Some(layout)
}

/// XuiNineGrid cells. Columns `{0, L, W-R, W}` and rows `{0, T, H-T, H}`
/// (the third row uses TopOffset, as the original), texture coordinates
/// `{0, L/tw, 1-R/tw, 1}` and `{0, T/th, 1-B/th, 1}`. Each cell maps
/// position to texture coordinates affinely, so drawing the cells as quads
/// covers and samples the same as the original 25-vertex strip.
pub(crate) fn nine_grid(
    size: Size,
    texture: Size,
    offsets: [u32; 4],
    no_center: bool,
    vertices: &mut Vec<LocalVertex>,
    triangles: &mut Vec<[u32; 3]>,
) {
    let [left, top, right, bottom] = offsets.map(|offset| offset as f32);
    let columns = [0.0, left, size.width - right, size.width];
    let rows = [0.0, top, size.height - top, size.height];
    let us = [0.0, left / texture.width, 1.0 - right / texture.width, 1.0];
    let vs = [
        0.0,
        top / texture.height,
        1.0 - bottom / texture.height,
        1.0,
    ];

    vertices.clear();
    triangles.clear();
    for row in 0..4 {
        for column in 0..4 {
            vertices.push(LocalVertex {
                x: columns[column],
                y: rows[row],
                u: us[column],
                v: vs[row],
            });
        }
    }
    for row in 0..3u32 {
        for column in 0..3u32 {
            if no_center && row == 1 && column == 1 {
                continue;
            }
            let corner = row * 4 + column;
            triangles.push([corner, corner + 1, corner + 5]);
            triangles.push([corner, corner + 5, corner + 4]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ELEMENT: Size = Size {
        width: 200.0,
        height: 100.0,
    };
    const TEXTURE: Size = Size {
        width: 50.0,
        height: 50.0,
    };

    #[test]
    fn size_modes() {
        let layouts: Vec<_> = [0x00, 0x01, 0x02, 0x04, 0x08, 0x10, 0x03]
            .into_iter()
            .map(|mode| (mode, image_layout(SizeMode(mode), ELEMENT, TEXTURE)))
            .collect();

        insta::assert_debug_snapshot!(layouts);
    }

    #[test]
    fn center_mode_clamps_rect_but_not_origin() {
        let layout = image_layout(
            SizeMode::CENTER,
            Size {
                width: 20.0,
                height: 20.0,
            },
            TEXTURE,
        )
        .unwrap();

        assert_eq!(layout.rect, [0.0, 0.0, 20.0, 20.0]);
        assert_eq!(layout.origin, [-15.0, -15.0]);
        assert_eq!(layout.quad()[0].u, 0.3);
    }

    #[test]
    fn fit_uses_the_relatively_wider_axis() {
        let tall = image_layout(
            SizeMode::FIT,
            ELEMENT,
            Size {
                width: 10.0,
                height: 100.0,
            },
        )
        .unwrap();
        let wide = image_layout(
            SizeMode::FIT,
            ELEMENT,
            Size {
                width: 400.0,
                height: 100.0,
            },
        )
        .unwrap();

        assert_eq!(tall.rect, [0.0, 0.0, 10.0, 100.0]);
        assert_eq!(wide.rect, [0.0, 0.0, 200.0, 50.0]);
    }

    #[test]
    fn empty_element_draws_nothing() {
        assert_eq!(
            image_layout(
                SizeMode::STRETCH,
                Size {
                    width: 0.0,
                    height: 10.0
                },
                TEXTURE
            ),
            None
        );
    }

    #[test]
    fn nine_grid_lower_row_uses_top_offset() {
        let mut vertices = Vec::new();
        let mut triangles = Vec::new();
        nine_grid(
            Size {
                width: 100.0,
                height: 80.0,
            },
            Size {
                width: 32.0,
                height: 32.0,
            },
            [4, 8, 6, 2],
            true,
            &mut vertices,
            &mut triangles,
        );

        assert_eq!(triangles.len(), 16);
        assert_eq!(vertices[8].y, 72.0);
        assert_eq!(vertices[8].v, 1.0 - 2.0 / 32.0);
        assert_eq!(vertices[2].x, 94.0);
    }
}

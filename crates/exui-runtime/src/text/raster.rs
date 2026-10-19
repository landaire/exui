//! Glyph scan conversion: outlines are sampled at 8x8 points per pixel and
//! each pixel's alpha is `min(255, 4 * covered samples)`.

use alloc::vec::Vec;

use crate::text::sfnt::OutlineSink;

/// Subsamples per pixel along each axis.
pub(crate) const OVERSAMPLE: i32 = 8;

/// Most edges of one glyph outline. Not 1:1: it bounds the fill work
/// hostile font data can cause; the original's limit is not known.
pub(crate) const MAX_EDGES: usize = 0x4000;

/// Converts font units to oversampled pixel units, y down from the cell top.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Placement {
    /// Oversampled pixels per font unit.
    pub scale: f32,
    /// Distance from the cell top to the baseline in oversampled pixels.
    pub baseline: f32,
}

/// Collects outline edges and fills them with the nonzero rule.
#[derive(Debug, Default)]
pub(crate) struct Rasterizer {
    edges: Vec<Edge>,
    crossings: Vec<Crossing>,
    counts: Vec<u8>,
}

impl Rasterizer {
    /// Starts a glyph; its outline is emitted into the returned sink.
    pub(crate) fn outline_sink(&mut self, placement: Placement) -> EdgeSink<'_> {
        self.edges.clear();

        EdgeSink {
            edges: &mut self.edges,
            placement,
            contour_start: (0.0, 0.0),
            pen: (0.0, 0.0),
        }
    }

    /// The last outline had more than [`MAX_EDGES`] edges; the excess was
    /// dropped.
    pub(crate) fn overflowed(&self) -> bool {
        self.edges.len() > MAX_EDGES
    }

    /// Fills the collected outline into a `width` x `height` pixel region of
    /// `target` starting at `origin` with `stride` bytes per row. `left` is
    /// the region's left edge relative to the pen in pixels.
    pub(crate) fn fill(
        &mut self,
        left: i32,
        width: usize,
        height: usize,
        target: &mut [u8],
        origin: usize,
        stride: usize,
    ) {
        let samples_per_row = width * OVERSAMPLE as usize;
        let shift = (left * OVERSAMPLE) as f32;

        for row in 0..height {
            self.counts.clear();
            self.counts.resize(width, 0);

            for subrow in 0..OVERSAMPLE as usize {
                let center = (row * OVERSAMPLE as usize + subrow) as f32 + 0.5;

                self.crossings.clear();
                for edge in &self.edges {
                    if edge.top <= center && center < edge.bottom {
                        let x = edge.top_x + (center - edge.top) * edge.slope - shift;
                        self.crossings.push(Crossing {
                            x,
                            winding: edge.winding,
                        });
                    }
                }

                self.crossings.sort_unstable_by(|a, b| a.x.total_cmp(&b.x));

                let mut winding = 0i32;
                let mut span_start = 0.0f32;
                for crossing in &self.crossings {
                    let before = winding;
                    winding += crossing.winding;

                    if before == 0 && winding != 0 {
                        span_start = crossing.x;
                    } else if before != 0 && winding == 0 {
                        // Sample columns whose centers lie in [span_start, x).
                        let first = sample_index(span_start, samples_per_row);
                        let end = sample_index(crossing.x, samples_per_row);
                        for sample in first..end {
                            let count = &mut self.counts[sample / OVERSAMPLE as usize];
                            *count = count.saturating_add(1);
                        }
                    }
                }
            }

            let start = origin + row * stride;
            for (texel, count) in target[start..start + width].iter_mut().zip(&self.counts) {
                *texel = count.saturating_mul(4);
            }
        }
    }
}

/// Outline receiver that converts segments to edges of a [`Rasterizer`].
#[derive(Debug)]
pub(crate) struct EdgeSink<'a> {
    edges: &'a mut Vec<Edge>,
    placement: Placement,
    contour_start: (f32, f32),
    pen: (f32, f32),
}

impl EdgeSink<'_> {
    fn map(&self, x: f32, y: f32) -> (f32, f32) {
        (
            x * self.placement.scale,
            self.placement.baseline - y * self.placement.scale,
        )
    }

    fn push_edge(&mut self, from: (f32, f32), to: (f32, f32)) {
        // One edge past the limit marks the overflow.
        if from.1 == to.1 || self.edges.len() > MAX_EDGES {
            return;
        }

        let (top, bottom, winding) = if from.1 < to.1 {
            (from, to, 1)
        } else {
            (to, from, -1)
        };
        let slope = (bottom.0 - top.0) / (bottom.1 - top.1);
        self.edges.push(Edge {
            top_x: top.0,
            top: top.1,
            bottom: bottom.1,
            slope,
            winding,
        });
    }
}

impl OutlineSink for EdgeSink<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let point = self.map(x, y);
        self.contour_start = point;
        self.pen = point;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let point = self.map(x, y);
        self.push_edge(self.pen, point);
        self.pen = point;
    }

    fn quad_to(&mut self, control_x: f32, control_y: f32, x: f32, y: f32) {
        // Segment count bounding the flattening error to 1/4 subsample.
        const MAX_SEGMENTS: f32 = 64.0;

        let start = self.pen;
        let control = self.map(control_x, control_y);
        let end = self.map(x, y);

        let deviation_x = start.0 - 2.0 * control.0 + end.0;
        let deviation_y = start.1 - 2.0 * control.1 + end.1;
        let deviation = libm::sqrtf(deviation_x * deviation_x + deviation_y * deviation_y);
        let segments = libm::ceilf(libm::sqrtf(deviation * 0.5)).clamp(1.0, MAX_SEGMENTS) as u32;

        let mut previous = start;
        for step in 1..=segments {
            let t = step as f32 / segments as f32;
            let inverse = 1.0 - t;
            let point = if step == segments {
                end
            } else {
                (
                    inverse * inverse * start.0 + 2.0 * inverse * t * control.0 + t * t * end.0,
                    inverse * inverse * start.1 + 2.0 * inverse * t * control.1 + t * t * end.1,
                )
            };

            self.push_edge(previous, point);
            previous = point;
        }

        self.pen = end;
    }

    fn close(&mut self) {
        self.push_edge(self.pen, self.contour_start);
        self.pen = self.contour_start;
    }
}

/// Non-horizontal outline edge in oversampled pixels, `top < bottom`.
#[derive(Clone, Copy, Debug)]
struct Edge {
    top_x: f32,
    top: f32,
    bottom: f32,
    /// x change per unit of y.
    slope: f32,
    /// +1 for edges running down, -1 for edges running up.
    winding: i32,
}

#[derive(Clone, Copy, Debug)]
struct Crossing {
    x: f32,
    winding: i32,
}

/// First sample column whose center `index + 0.5` is at or right of `x`.
fn sample_index(x: f32, samples_per_row: usize) -> usize {
    let index = libm::ceilf(x - 0.5);
    if index <= 0.0 {
        return 0;
    }

    (index as usize).min(samples_per_row)
}

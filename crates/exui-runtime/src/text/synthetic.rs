//! Synthetic oblique and bold outlines of TextStyle bits 0x2 and 0x4
//! (doc/xui-text.md section 4). The font loader (0x9218dd48) keeps points
//! as 16-bit font units with y negated and applies the oblique shear, then
//! the emboldening, to each simple glyph or component before its component
//! transform.

/// A point in the loader's convention: font units, y down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LoaderPoint {
    pub(crate) x: i16,
    pub(crate) y: i16,
}

/// Synthetic style of a font instance.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) struct SyntheticStyle {
    pub(crate) oblique: bool,
    /// Outline offset `k` in font units.
    pub(crate) bold: Option<BoldOffset>,
}

impl SyntheticStyle {
    const OBLIQUE: u32 = 0x2;
    const BOLD: u32 = 0x4;

    /// The style of TextStyle `bits` for a font of `units_per_em`.
    pub(crate) fn new(bits: u32, units_per_em: u16) -> Self {
        Self {
            oblique: bits & Self::OBLIQUE != 0,
            bold: (bits & Self::BOLD != 0).then(|| BoldOffset::new(units_per_em)),
        }
    }

    /// Font units the emboldening adds to advances, line height, ascender
    /// and the maximum glyph bounds: `ceil(2 * k)` (0x9218b6a8, 0x9218b238,
    /// 0x9218b3a8, 0x9218ae28).
    pub(crate) fn extra_units(self) -> i32 {
        self.bold.map_or(0, |bold| libm::ceilf(bold.0 * 2.0) as i32)
    }

    /// Applies the style to the decoded points of one simple glyph.
    pub(crate) fn apply(self, points: &mut [LoaderPoint], ends: &[usize]) {
        if self.oblique {
            oblique(points);
        }
        if let Some(bold) = self.bold {
            embolden(points, ends, bold);
        }
    }

    /// Glyph bounds (glyf header, y up) as 0x9218ae28 styles them.
    pub(crate) fn bounds(self, x_min: i16, y_min: i16, x_max: i16, y_max: i16) -> [i16; 4] {
        let (mut x_min, mut x_max, mut y_max) = (x_min, x_max, y_max);
        if self.oblique {
            let third = 1.0f32 / OBLIQUE_DIVISOR;
            x_min = libm::fmaf(y_min as f32, third, x_min as f32) as i32 as i16;
            x_max = libm::fmaf(y_max as f32, third, x_max as f32) as i32 as i16;
        }
        let extra = self.extra_units() as i16;
        x_max = x_max.wrapping_add(extra);
        y_max = y_max.wrapping_add(extra);

        [x_min, y_min, x_max, y_max]
    }
}

/// `units_per_em * 0.0122` (font instance +0x14 and +0x18, 0x9218aa70).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct BoldOffset(f32);

impl Eq for BoldOffset {}

impl BoldOffset {
    fn new(units_per_em: u16) -> Self {
        const FACTOR: f32 = 0.0122;

        Self(units_per_em as i16 as f32 * FACTOR)
    }
}

/// Font instance +0x10.
const OBLIQUE_DIVISOR: f32 = 3.0;

/// 0x9218cc80: `x -= (int)(y / 3 + 0.5)` in the loader's convention.
fn oblique(points: &mut [LoaderPoint]) {
    for point in points {
        let shift = (point.y as f32 / OBLIQUE_DIVISOR + 0.5) as i32;
        point.x = point.x.wrapping_sub(shift as i16);
    }
}

/// 0x9218cce8: moves each point along the bisector of the outward offsets
/// of its two edges, offset by `k`, then right and up by `k`. Neighbors are
/// read before they are moved.
fn embolden(points: &mut [LoaderPoint], ends: &[usize], k: BoldOffset) {
    const COMPARE_SCALE: f32 = 100.0;
    const HALF: f32 = 0.5;

    let k = k.0;
    let mut start = 0usize;
    for &end in ends {
        if end <= start || end > points.len() {
            break;
        }

        let first = points[start];
        let mut previous = points[end - 1];
        for index in start..end {
            let current = points[index];
            let next = if index + 1 == end {
                first
            } else {
                points[index + 1]
            };

            let to_previous = normalize(difference(previous, current));
            let to_next = normalize(difference(current, next));
            let offset_a = [k * -to_previous[1], k * to_previous[0]];
            let offset_b = [k * -to_next[1], k * to_next[0]];
            let compare = |value: f32| (value * COMPARE_SCALE) as i32;

            let shift = if compare(offset_b[0]) == compare(offset_a[0])
                && compare(offset_b[1]) != compare(offset_a[1])
            {
                offset_a
            } else {
                let cur = [current.x as f32, current.y as f32];
                let line_a_start = [
                    previous.x as f32 + offset_a[0],
                    previous.y as f32 + offset_a[1],
                ];
                let line_a_end = [cur[0] + offset_a[0], cur[1] + offset_a[1]];
                let line_b_start = [cur[0] + offset_b[0], cur[1] + offset_b[1]];
                let line_b_end = [next.x as f32 + offset_b[0], next.y as f32 + offset_b[1]];
                let intersection = intersect(line_a_start, line_a_end, line_b_start, line_b_end);
                let point = if intersection[0].is_finite() {
                    intersection
                } else {
                    [
                        (line_b_start[0] + line_a_end[0]) * HALF,
                        (line_b_start[1] + line_a_end[1]) * HALF,
                    ]
                };

                [point[0] - cur[0], point[1] - cur[1]]
            };

            let x = (shift[0] + k + HALF) as i32;
            let y = (shift[1] - k + HALF) as i32;
            points[index] = LoaderPoint {
                x: current.x.wrapping_add(x as i16),
                y: current.y.wrapping_add(y as i16),
            };
            previous = current;
        }

        start = end;
    }
}

fn difference(a: LoaderPoint, b: LoaderPoint) -> [f32; 2] {
    [
        (a.x as i32 - b.x as i32) as f32,
        (a.y as i32 - b.y as i32) as f32,
    ]
}

/// Unit vector; the zero vector stays zero. The original refines a
/// reciprocal square root estimate (`vrsqrtefp`) with one Newton step.
fn normalize(vector: [f32; 2]) -> [f32; 2] {
    let length_squared = vector[0] * vector[0] + vector[1] * vector[1];
    if length_squared == 0.0 {
        return [0.0, 0.0];
    }

    let reciprocal = 1.0 / libm::sqrtf(length_squared);

    [vector[0] * reciprocal, vector[1] * reciprocal]
}

/// Intersection of the line through `a_start`, `a_end` with the line
/// through `b_start`, `b_end`: +inf for coincident lines, NaN for parallel
/// ones (0x9218d0a4 to 0x9218d134). The original refines a reciprocal
/// estimate (`vrefp`) with two Newton steps.
fn intersect(a_start: [f32; 2], a_end: [f32; 2], b_start: [f32; 2], b_end: [f32; 2]) -> [f32; 2] {
    /// 0x34000000, built as 13 << 26.
    const EPSILON: f32 = 1.192_092_9e-7;

    let direction_a = [a_end[0] - a_start[0], a_end[1] - a_start[1]];
    let direction_b = [b_end[0] - b_start[0], b_end[1] - b_start[1]];
    let between = [a_start[0] - b_start[0], a_start[1] - b_start[1]];
    let numerator = direction_b[0] * between[1] - direction_b[1] * between[0];
    let denominator = direction_a[0] * direction_b[1] - direction_a[1] * direction_b[0];

    if denominator.abs() <= EPSILON {
        let value = if numerator.abs() <= EPSILON {
            f32::INFINITY
        } else {
            f32::NAN
        };
        return [value, value];
    }

    let t = numerator * (1.0 / denominator);

    [
        libm::fmaf(direction_a[0], t, a_start[0]),
        libm::fmaf(direction_a[1], t, a_start[1]),
    ]
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;

    fn points(coordinates: &[(i16, i16)]) -> Vec<LoaderPoint> {
        coordinates
            .iter()
            .map(|&(x, y)| LoaderPoint { x, y })
            .collect()
    }

    #[test]
    fn oblique_shifts_by_a_third_of_y_rounded_toward_zero() {
        let mut shifted = points(&[(0, -100), (0, 100), (10, 0)]);

        oblique(&mut shifted);

        assert_eq!(shifted, points(&[(32, -100), (-33, 100), (10, 0)]));
    }

    #[test]
    fn embolden_grows_right_and_top_edges() {
        let mut square = points(&[(0, 0), (0, -100), (100, -100), (100, 0)]);

        embolden(&mut square, &[4], BoldOffset::new(1000));

        assert_eq!(square, points(&[(0, 0), (0, -123), (124, -123), (124, 0)]));
    }

    #[test]
    fn embolden_moves_collinear_points_to_the_offset_midpoint() {
        let mut triangle = points(&[(0, 0), (50, 0), (100, 0), (50, -80)]);

        embolden(&mut triangle, &[4], BoldOffset::new(1000));

        assert_eq!(triangle[1], LoaderPoint { x: 62, y: -23 });
    }

    #[test]
    fn bold_extra_units_and_styled_bounds() {
        let style = SyntheticStyle::new(0x6, 1000);

        assert_eq!(style.extra_units(), 25);
        assert_eq!(style.bounds(10, -90, 500, 600), [-20, -90, 725, 625]);
    }
}

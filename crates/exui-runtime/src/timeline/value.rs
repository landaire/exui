//! Keyframe value interpolation and ease curves (xui-timelines.md 2, 4.2,
//! 4.3).

use exui_core::value::Color;
use exui_core::value::Quaternion;
use exui_core::value::Vector3;

use crate::scene::PropValue;

/// Interpolation of the segment that starts at a keyframe.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Segment {
    Linear,
    Hold,
    /// Cubic Bezier on the y axis only. The x components of the control
    /// points are never read.
    Ease {
        p1_y: f32,
        p2_y: f32,
    },
}

impl Segment {
    /// Converts the XUR interpolation byte and ease bytes. The loader tests
    /// the hold bit before the ease bit, so XUR interpolation 3 holds.
    pub fn from_xur(interpolation: u8, ease_in: u8, ease_out: u8, ease_scale: u8) -> Self {
        const HOLD: u8 = 0x1;
        const EASE: u8 = 0x2;

        if interpolation & HOLD != 0 {
            return Self::Hold;
        }

        if interpolation & EASE == 0 {
            return Self::Linear;
        }

        // The ease bytes are signed.
        let (p1_y, p2_y) = ease_control_y(ease_in as i8, ease_out as i8, ease_scale as i8);

        Self::Ease { p1_y, p2_y }
    }

    /// Maps the linear segment fraction through the segment's curve.
    pub fn shape(self, t: f32) -> f32 {
        let Self::Ease { p1_y, p2_y } = self else {
            return t;
        };

        let u = 1.0 - t;

        (p1_y * u + p2_y * t) * u * t * 3.0 + t * t * t
    }

    /// Value of one path at shaped progress `t` through the segment, from
    /// `first` toward `second`. `None` keeps `first`: the segment holds or
    /// the type steps.
    pub fn value(self, first: &PropValue, second: &PropValue, t: f32) -> Option<PropValue> {
        match self {
            Self::Hold => None,
            Self::Linear | Self::Ease { .. } => interpolate(first, second, t),
        }
    }
}

/// `XuiEaseToBezierPoints` 0x9214fa28, y components only.
///
/// [INFERENCE] The rotation is counterclockwise (see the timeline notes).
fn ease_control_y(ease_in: i8, ease_out: i8, ease_scale: i8) -> (f32, f32) {
    const QUARTER_PI: f32 = core::f32::consts::FRAC_PI_4;

    let scale = f32::from(ease_scale) * 0.01;
    let a1 = (1.0 - f32::from(ease_in) * 0.01) * QUARTER_PI;
    let a2 = (f32::from(ease_out) * 0.01 - 1.0) * -QUARTER_PI;

    let p1_y = scale * libm::sinf(a1);
    let p2_y = 1.0 - scale * libm::sinf(a2);

    (p1_y, p2_y)
}

/// `XuiValueInterpolate` 0x921547f0. `None` means the type steps, so the
/// first keyframe's value is used.
pub fn interpolate(a: &PropValue, b: &PropValue, t: f32) -> Option<PropValue> {
    let value = match (a, b) {
        (PropValue::Integer(a), PropValue::Integer(b)) => {
            // fctiwz truncates toward zero; the difference wraps in 32 bits.
            let delta = b.wrapping_sub(*a) as f32;
            PropValue::Integer((*a as f32 + delta * t) as i32)
        }
        (PropValue::Float(a), PropValue::Float(b)) => PropValue::Float(fused(*b - *a, t, *a)),
        (PropValue::Color(a), PropValue::Color(b)) => PropValue::Color(lerp_color(*a, *b, t)),
        (PropValue::Vector(a), PropValue::Vector(b)) => PropValue::Vector(Vector3::new(
            fused(b.x - a.x, t, a.x),
            fused(b.y - a.y, t, a.y),
            fused(b.z - a.z, t, a.z),
        )),
        (PropValue::Quaternion(a), PropValue::Quaternion(b)) => {
            PropValue::Quaternion(slerp(*a, *b, t))
        }
        _ => return None,
    };

    Some(value)
}

/// `x * y + z` with one rounding, as the PowerPC and VMX multiply-add
/// instructions compute it. The f64 product of two f32 values is exact.
fn fused(x: f32, y: f32, z: f32) -> f32 {
    (f64::from(x) * f64::from(y) + f64::from(z)) as f32
}

/// Per-channel lerp in [0, 1] units, then the D3DCOLOR pack: the value is
/// added to 3.0 scaled so that one channel step is one mantissa unit, and
/// the pack keeps the low 8 mantissa bits.
///
/// [INFERENCE] The pack takes the low mantissa bits without saturation.
fn lerp_color(a: Color, b: Color, t: f32) -> Color {
    const SHIFTS: [u32; 4] = [24, 16, 8, 0];
    const INV_255: f32 = 0.003_921_569;
    const PACK_SCALE: f64 = 255.0 / 4_194_304.0;

    let mut argb = 0u32;
    for shift in SHIFTS {
        let from = ((a.argb() >> shift) & 0xff) as f32 * INV_255;
        let to = ((b.argb() >> shift) & 0xff) as f32 * INV_255;
        let value = fused(to - from, t, from);

        let biased = (3.0 + f64::from(value) * PACK_SCALE) as f32;
        argb |= (biased.to_bits() & 0xff) << shift;
    }

    Color::from_argb(argb)
}

/// Spherical interpolation along the shorter arc.
///
/// [INFERENCE] The VMX routine at 0x9217e4d0 was only partly decoded. This
/// follows its observed structure (dot product, sign flip toward the shorter
/// arc, square root of 1 - cos^2) with a linear blend when the inputs are
/// nearly parallel.
fn slerp(a: Quaternion, b: Quaternion, t: f32) -> Quaternion {
    const PARALLEL: f32 = 1.0 - 0.000_01;

    let dot = a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w;
    let sign = if dot < 0.0 { -1.0 } else { 1.0 };
    let cos = dot * sign;

    let (wa, wb) = if cos < PARALLEL {
        let sin = libm::sqrtf(1.0 - cos * cos);
        let omega = libm::atan2f(sin, cos);
        let inv_sin = 1.0 / sin;
        (
            libm::sinf((1.0 - t) * omega) * inv_sin,
            libm::sinf(t * omega) * inv_sin,
        )
    } else {
        (1.0 - t, t)
    };
    let wb = wb * sign;

    Quaternion::new(
        a.x * wa + b.x * wb,
        a.y * wa + b.y * wb,
        a.z * wa + b.z * wb,
        a.w * wa + b.w * wb,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ease_control_points_and_endpoints() {
        // Full scale with no ease rotates the points by 45 degrees.
        let segment = Segment::from_xur(2, 0, 0, 100);
        let Segment::Ease { p1_y, p2_y } = segment else {
            panic!("expected ease");
        };
        let half = libm::sinf(core::f32::consts::FRAC_PI_4);
        assert!((p1_y - half).abs() < 1e-6);
        assert!((p2_y - (1.0 - half)).abs() < 1e-6);

        // Full ease in and out lays both points flat: an S curve.
        let segment = Segment::from_xur(2, 100, 100, 100);
        assert_eq!(
            segment,
            Segment::Ease {
                p1_y: 0.0,
                p2_y: 1.0
            }
        );

        for segment in [
            Segment::from_xur(2, 0, 0, 100),
            Segment::from_xur(2, 100, 100, 100),
            Segment::from_xur(2, -50i8 as u8, 30, 80),
        ] {
            assert_eq!(segment.shape(0.0), 0.0);
            assert!((segment.shape(1.0) - 1.0).abs() < 1e-6);
            let mut previous = 0.0;
            for step in 1..=20 {
                let value = segment.shape(step as f32 / 20.0);
                assert!(value >= previous);
                previous = value;
            }
        }
    }

    #[test]
    fn interpolation_three_holds() {
        assert_eq!(Segment::from_xur(3, 50, 50, 50), Segment::Hold);
        assert_eq!(Segment::from_xur(0, 50, 50, 50), Segment::Linear);
    }

    #[test]
    fn integer_truncates_toward_zero() {
        let a = PropValue::Integer(0);
        let b = PropValue::Integer(-10);
        assert_eq!(interpolate(&a, &b, 0.25), Some(PropValue::Integer(-2)));

        let b = PropValue::Integer(10);
        assert_eq!(interpolate(&a, &b, 0.29), Some(PropValue::Integer(2)));
    }

    #[test]
    fn steps_for_unsigned_bool_string() {
        let a = PropValue::Unsigned(0);
        let b = PropValue::Unsigned(10);
        assert_eq!(interpolate(&a, &b, 0.5), None);
        assert_eq!(
            interpolate(&PropValue::Bool(false), &PropValue::Bool(true), 0.5),
            None
        );
    }

    #[test]
    fn color_rounds_per_channel() {
        let a = Color::from_argb(0x00_00_00_00);
        let b = Color::from_argb(0xff_80_01_ff);
        let mid = lerp_color(a, b, 0.5);

        // 127.5 rounds to even and 64.0 is exact; 1/255 * 0.5 lands just
        // above half a step and rounds up.
        assert_eq!(mid.argb(), 0x80_40_01_80);
        assert_eq!(lerp_color(a, b, 1.0).argb(), 0xff_80_01_ff);
        assert_eq!(lerp_color(a, b, 0.0).argb(), 0);
    }

    #[test]
    fn slerp_takes_short_arc() {
        let half = core::f32::consts::FRAC_1_SQRT_2;
        let a = Quaternion::new(0.0, 0.0, 0.0, 1.0);
        let b = Quaternion::new(0.0, 0.0, half, half);
        let mid = slerp(a, b, 0.5);
        let angle = 2.0 * libm::atan2f(mid.z, mid.w);
        assert!((angle - core::f32::consts::FRAC_PI_4).abs() < 1e-5);

        let negated = Quaternion::new(0.0, 0.0, -half, -half);
        let mid = slerp(a, negated, 0.5);
        let angle = 2.0 * libm::atan2f(mid.z, mid.w);
        assert!((angle - core::f32::consts::FRAC_PI_4).abs() < 1e-5);
    }
}

//! Row-vector 4x4 matrices as used by XUI (`v' = v * M`, translation in
//! row 3). See doc/xui-render.md.

use exui_core::value::Quaternion;
use exui_core::value::Vector3;

/// Row-major 4x4 matrix applied to row vectors.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix4(pub [[f32; 4]; 4]);

impl Matrix4 {
    pub const IDENTITY: Self = Self([
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]);

    pub const fn scaling(x: f32, y: f32, z: f32) -> Self {
        Self([
            [x, 0.0, 0.0, 0.0],
            [0.0, y, 0.0, 0.0],
            [0.0, 0.0, z, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    pub const fn translation(x: f32, y: f32, z: f32) -> Self {
        Self([
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [x, y, z, 1.0],
        ])
    }

    /// Rotation of a unit quaternion.
    pub fn rotation_quaternion(rotation: Quaternion) -> Self {
        let Quaternion { x, y, z, w } = rotation;

        Self([
            [
                1.0 - 2.0 * (y * y + z * z),
                2.0 * (x * y + z * w),
                2.0 * (x * z - y * w),
                0.0,
            ],
            [
                2.0 * (x * y - z * w),
                1.0 - 2.0 * (x * x + z * z),
                2.0 * (y * z + x * w),
                0.0,
            ],
            [
                2.0 * (x * z + y * w),
                2.0 * (y * z - x * w),
                1.0 - 2.0 * (x * x + y * y),
                0.0,
            ],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    /// Rotation about Z by `degrees`; positive turns +X towards +Y.
    pub fn rotation_z_degrees(degrees: f32) -> Self {
        let radians = degrees * (core::f32::consts::PI / 180.0);
        let (sin, cos) = (libm::sinf(radians), libm::cosf(radians));

        Self([
            [cos, sin, 0.0, 0.0],
            [-sin, cos, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ])
    }

    /// `self * other`: applies `self` first.
    pub fn mul(&self, other: &Self) -> Self {
        let mut out = [[0.0; 4]; 4];
        for (row, out_row) in out.iter_mut().enumerate() {
            for (column, cell) in out_row.iter_mut().enumerate() {
                *cell = self.0[row][0] * other.0[0][column]
                    + self.0[row][1] * other.0[1][column]
                    + self.0[row][2] * other.0[2][column]
                    + self.0[row][3] * other.0[3][column];
            }
        }

        Self(out)
    }

    /// `(x, y, z, 1) * self`.
    pub fn transform(&self, x: f32, y: f32, z: f32) -> [f32; 4] {
        let m = &self.0;

        [
            x * m[0][0] + y * m[1][0] + z * m[2][0] + m[3][0],
            x * m[0][1] + y * m[1][1] + z * m[2][1] + m[3][1],
            x * m[0][2] + y * m[1][2] + z * m[2][2] + m[3][2],
            x * m[0][3] + y * m[1][3] + z * m[2][3] + m[3][3],
        ]
    }

    /// `transform` followed by the division by w (`D3DXVec3TransformCoord`).
    pub fn transform_coord(&self, x: f32, y: f32, z: f32) -> [f32; 3] {
        let [tx, ty, tz, tw] = self.transform(x, y, z);

        [tx / tw, ty / tw, tz / tw]
    }
}

/// Element local transform (`XuiElement_UpdateTransform`):
/// `Translate(-Pivot) * Scale * Rotation * Translate(Pivot) * Translate(Position)`.
pub fn local_transform(
    position: Vector3,
    scale: Vector3,
    rotation: Quaternion,
    pivot: Vector3,
) -> Matrix4 {
    Matrix4::translation(-pivot.x, -pivot.y, -pivot.z)
        .mul(&Matrix4::scaling(scale.x, scale.y, scale.z))
        .mul(&Matrix4::rotation_quaternion(rotation))
        .mul(&Matrix4::translation(
            pivot.x + position.x,
            pivot.y + position.y,
            pivot.z + position.z,
        ))
}

/// Rounds half to even, as the VMX color packing does.
pub fn round_ties_even(value: f32) -> f32 {
    libm::rintf(value)
}

/// Packs a unit-range channel into a byte with round half to even. Values
/// outside [0, 1] saturate.
pub fn unit_to_byte(value: f32) -> u8 {
    round_ties_even(value * 255.0).clamp(0.0, 255.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(actual: [f32; 4], expected: [f32; 4]) {
        for (a, e) in actual.iter().zip(expected) {
            assert!((a - e).abs() < 1e-4, "{actual:?} != {expected:?}");
        }
    }

    #[test]
    fn local_transform_rotates_about_the_pivot_before_translating() {
        // 90 degrees about Z, pivot at (10, 0), placed at (100, 50).
        let half = core::f32::consts::FRAC_1_SQRT_2;
        let local = local_transform(
            Vector3::new(100.0, 50.0, 0.0),
            Vector3::new(2.0, 1.0, 1.0),
            Quaternion::new(0.0, 0.0, half, half),
            Vector3::new(10.0, 0.0, 0.0),
        );

        // The pivot stays at Position + Pivot.
        assert_close(local.transform(10.0, 0.0, 0.0), [110.0, 50.0, 0.0, 1.0]);
        // (11, 0): 1 unit right of the pivot, scaled by 2, turned to +Y.
        assert_close(local.transform(11.0, 0.0, 0.0), [110.0, 52.0, 0.0, 1.0]);
    }

    #[test]
    fn quaternion_matches_z_rotation() {
        let half_angle = 0.3f32;
        let quaternion = Matrix4::rotation_quaternion(Quaternion::new(
            0.0,
            0.0,
            libm::sinf(half_angle),
            libm::cosf(half_angle),
        ));
        let euler = Matrix4::rotation_z_degrees(2.0 * half_angle * 180.0 / core::f32::consts::PI);

        assert_close(
            quaternion.transform(3.0, 4.0, 0.0),
            euler.transform(3.0, 4.0, 0.0),
        );
    }

    #[test]
    fn byte_packing_rounds_half_to_even() {
        assert_eq!(round_ties_even(0.5), 0.0);
        assert_eq!(unit_to_byte(0.5), 128);
        assert_eq!(round_ties_even(1.5), 2.0);
        assert_eq!(unit_to_byte(1.5), 255);
        assert_eq!(unit_to_byte(-0.25), 0);
    }
}

//! Value types shared by the schema and the scene document.

/// 32-bit ARGB color.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Color(u32);

impl Color {
    pub const fn from_argb(argb: u32) -> Self {
        Self(argb)
    }

    pub const fn argb(self) -> u32 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vector3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vector3 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quaternion {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Quaternion {
    pub const fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }
}

//! Decoders for custom property blobs.

use alloc::vec::Vec;

use crate::cursor::Cursor;

/// One point record of a figure path: six floats in file order. The loader
/// copies them without interpretation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FigurePoint {
    pub coords: [f32; 6],
}

/// `XuiFigure.Points` blob, as decoded by the binary load handler of its
/// custom type (0x92164a68).
#[derive(Clone, Debug, PartialEq)]
pub struct FigurePoints {
    pub width: f32,
    pub height: f32,
    /// `None` when the blob ends before a complete point list, which the
    /// handler then ignores.
    pub points: Option<Vec<FigurePoint>>,
}

impl FigurePoints {
    /// `None` when the blob is shorter than the eight-byte size header; the
    /// handler then applies nothing.
    pub fn parse(blob: &[u8]) -> Option<Self> {
        let mut cursor = Cursor::new(blob, 0);
        let width = cursor.f32().ok()?;
        let height = cursor.f32().ok()?;

        let points = cursor.u32().ok().and_then(|count| {
            let size = (count as usize).checked_mul(24)?;
            if cursor.remaining() < size {
                return None;
            }

            let mut points = Vec::with_capacity(count as usize);
            for _ in 0..count {
                let mut coords = [0.0; 6];
                for coord in &mut coords {
                    *coord = cursor.f32().ok()?;
                }
                points.push(FigurePoint { coords });
            }

            Some(points)
        });

        Some(Self {
            width,
            height,
            points,
        })
    }
}

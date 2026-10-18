//! Big-endian reads over a byte window with absolute offsets.

use winnow::Parser;
use winnow::binary::be_f32;
use winnow::binary::be_u8;
use winnow::binary::be_u16;
use winnow::binary::be_u32;
use winnow::token::take;

/// A read ran past the end of the window.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Short {
    pub offset: usize,
    pub needed: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct Cursor<'a> {
    input: &'a [u8],
    window_len: usize,
    origin: usize,
}

impl<'a> Cursor<'a> {
    /// `origin` is the absolute offset of `window[0]`.
    pub fn new(window: &'a [u8], origin: usize) -> Self {
        Self {
            input: window,
            window_len: window.len(),
            origin,
        }
    }

    pub fn offset(&self) -> usize {
        self.origin + (self.window_len - self.input.len())
    }

    pub fn remaining(&self) -> usize {
        self.input.len()
    }

    pub fn u8(&mut self) -> Result<u8, Short> {
        self.read(1, be_u8)
    }

    pub fn u16(&mut self) -> Result<u16, Short> {
        self.read(2, be_u16)
    }

    pub fn u32(&mut self) -> Result<u32, Short> {
        self.read(4, be_u32)
    }

    pub fn f32(&mut self) -> Result<f32, Short> {
        self.read(4, be_f32)
    }

    pub fn bytes(&mut self, count: usize) -> Result<&'a [u8], Short> {
        let offset = self.offset();

        take::<_, _, ()>(count)
            .parse_next(&mut self.input)
            .map_err(|()| Short {
                offset,
                needed: count,
            })
    }

    fn read<T>(
        &mut self,
        size: usize,
        mut parser: impl Parser<&'a [u8], T, ()>,
    ) -> Result<T, Short> {
        let offset = self.offset();

        parser.parse_next(&mut self.input).map_err(|()| Short {
            offset,
            needed: size,
        })
    }
}

//! Big-endian reads over font data.

use winnow::Parser;
use winnow::binary::be_i8;
use winnow::binary::be_i16;
use winnow::binary::be_u8;
use winnow::binary::be_u16;
use winnow::binary::be_u32;
use winnow::token::take;

use crate::text::error::ShortRead;

/// Sequential reader over a byte slice.
#[derive(Clone, Debug)]
pub(crate) struct Reader<'a> {
    data: &'a [u8],
    input: &'a [u8],
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self {
            data: bytes,
            input: bytes,
        }
    }

    /// Reader starting `offset` bytes into `bytes`.
    pub(crate) fn at(bytes: &'a [u8], offset: usize) -> Result<Self, ShortRead> {
        let input = bytes.get(offset..).ok_or(ShortRead {
            offset,
            needed: 0,
            len: bytes.len(),
        })?;

        Ok(Self { data: bytes, input })
    }

    pub(crate) fn u8(&mut self) -> Result<u8, ShortRead> {
        self.read(1, be_u8::<_, ()>)
    }

    pub(crate) fn i8(&mut self) -> Result<i8, ShortRead> {
        self.read(1, be_i8::<_, ()>)
    }

    pub(crate) fn u16(&mut self) -> Result<u16, ShortRead> {
        self.read(2, be_u16::<_, ()>)
    }

    pub(crate) fn i16(&mut self) -> Result<i16, ShortRead> {
        self.read(2, be_i16::<_, ()>)
    }

    pub(crate) fn u32(&mut self) -> Result<u32, ShortRead> {
        self.read(4, be_u32::<_, ()>)
    }

    pub(crate) fn bytes(&mut self, count: usize) -> Result<&'a [u8], ShortRead> {
        self.read(count, take::<_, _, ()>(count))
    }

    pub(crate) fn skip(&mut self, count: usize) -> Result<(), ShortRead> {
        self.bytes(count).map(|_| ())
    }

    fn read<O>(
        &mut self,
        needed: usize,
        mut parser: impl Parser<&'a [u8], O, ()>,
    ) -> Result<O, ShortRead> {
        let short = ShortRead {
            offset: self.data.len() - self.input.len(),
            needed,
            len: self.data.len(),
        };

        parser.parse_next(&mut self.input).map_err(|()| short)
    }
}

/// Big-endian u16 at `offset`.
pub(crate) fn u16_at(bytes: &[u8], offset: usize) -> Result<u16, ShortRead> {
    Reader::at(bytes, offset)?.u16()
}

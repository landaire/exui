//! `xttf` font container: an RSA-signed header followed by a zlib stream
//! holding a TrueType file. The signature is not verified.

use alloc::vec::Vec;

use zune_inflate::DeflateDecoder;
use zune_inflate::DeflateOptions;

use crate::text::error::FontError;
use crate::text::error::InflateError;
use crate::text::error::ShortRead;
use crate::text::reader::Reader;

pub(crate) const MAGIC: [u8; 4] = *b"xttf";

/// Inflates the TrueType file inside an `xttf` container.
pub(crate) fn decompress(file: &[u8]) -> Result<Vec<u8>, FontError> {
    const SIGNATURE_LEN: usize = 0x100;
    const VERSION: u32 = 0x0001_0000;

    let truncated = |_: ShortRead| FontError::XttTruncated { len: file.len() };
    let mut header = Reader::new(file);
    header
        .skip(MAGIC.len() + SIGNATURE_LEN)
        .map_err(truncated)?;
    let signed_len = header.u32().map_err(truncated)?;
    let recorded_size = header.u32().map_err(truncated)?;
    let compressed_len = header.u32().map_err(truncated)?;
    let uncompressed_len = header.u32().map_err(truncated)?;
    let version = header.u32().map_err(truncated)?;

    if signed_len as usize > file.len() - SIGNATURE_LEN {
        return Err(FontError::XttSignedLength {
            signed_len,
            file_len: file.len(),
        });
    }

    if recorded_size as usize != file.len() {
        return Err(FontError::XttFileSize {
            recorded: recorded_size,
            actual: file.len(),
        });
    }

    if version != VERSION {
        return Err(FontError::XttVersion { version });
    }

    let compressed =
        header
            .bytes(compressed_len as usize)
            .map_err(|_| FontError::XttCompressedLength {
                compressed_len,
                file_len: file.len(),
            })?;

    // zlib `uncompress` fails when the output exceeds the destination size
    // and succeeds with a shorter output. No size hint: the header value is
    // untrusted and the decoder would allocate it up front.
    let options = DeflateOptions::default().set_limit(uncompressed_len as usize);

    DeflateDecoder::new_with_options(compressed, options)
        .decode_zlib()
        .map_err(|error| FontError::XttInflate(InflateError(error.error)))
}

use alloc::boxed::Box;
use alloc::vec::Vec;

use crate::extension::ExtensionName;
use crate::resource::ModuleHandle;
use crate::resource::ResourceError;
use crate::resource::SectionName;
use crate::resource::UnsupportedScheme;
use crate::resource::WidePath;

/// Where a resource lives, after resolution.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Locator {
    /// A file inside the XUIZ archive stored as section `section` of the
    /// image `module`.
    Section {
        module: ModuleHandle,
        section: SectionName,
        path: Box<[u16]>,
    },
    /// A `file://` path, such as `media:/font.xtt`.
    File { path: Box<[u16]> },
    /// A byte block the application registered with
    /// [`crate::resource::Resources::insert_memory`]:
    /// `memory://<block hex>,<size hex>`. The original names a buffer
    /// address; the block id takes its place.
    Memory { block: MemoryBlockId, size: u32 },
    /// A file of an enabled extension's archive: `ext://<name>/<file>`.
    /// Not an XUI scheme.
    Extension {
        name: ExtensionName,
        path: Box<[u16]>,
    },
}

/// Id of a registered memory block, the address field of a `memory://`
/// locator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MemoryBlockId(pub u32);

impl Locator {
    /// `memory://<block hex>,<size hex>`, as the dashboard formats it with
    /// `memory://%X,%X`.
    pub fn memory_path(block: MemoryBlockId, size: u32) -> Box<[u16]> {
        let mut path: Vec<u16> = "memory://".encode_utf16().collect();
        push_hex(&mut path, block.0);
        path.push(u16::from(b','));
        push_hex(&mut path, size);
        path.into_boxed_slice()
    }

    /// Parses an absolute XUI locator: `section://<module hex>,<name>#<file>`,
    /// `file://<path>` or `memory://<block hex>,<size hex>`; in-memory
    /// archives (`memory://...#<file>`) are not supported. Scheme names
    /// compare without ASCII case [INFERENCE].
    pub fn parse(path: &[u16]) -> Result<Self, ResourceError> {
        if let Some(rest) = strip_prefix_ignore_case(path, "section://") {
            return parse_section(path, rest);
        }
        if let Some(rest) = strip_prefix_ignore_case(path, "file://") {
            return Ok(Self::File { path: rest.into() });
        }
        if let Some(rest) = strip_prefix_ignore_case(path, "memory://") {
            return parse_memory(path, rest);
        }
        if let Some(rest) = strip_prefix_ignore_case(path, EXTENSION_SCHEME) {
            return parse_extension(path, rest);
        }
        if is_absolute(path) {
            return Err(ResourceError::UnknownScheme {
                path: WidePath::from_units(path),
            });
        }

        Err(ResourceError::NoBasePath {
            path: WidePath::from_units(path),
        })
    }

    /// Cache key: the module, the section and the [`archive_key`] of the
    /// path, so spellings of one entry share a key.
    pub fn cache_key(&self) -> Box<[u16]> {
        let mut key = Vec::new();
        match self {
            Self::Section {
                module,
                section,
                path,
            } => {
                push_hex(&mut key, module.0);
                key.push(u16::from(b','));
                key.extend(section.as_str().bytes().map(u16::from));
                key.push(u16::from(b'#'));
                key.extend(archive_key(path));
            }
            Self::File { path } => {
                key.extend("file://".bytes().map(u16::from));
                key.extend_from_slice(path);
            }
            Self::Memory { block, size } => {
                key.extend_from_slice(&Self::memory_path(*block, *size))
            }
            Self::Extension { name, path } => {
                key.extend(EXTENSION_SCHEME.bytes().map(u16::from));
                key.extend(name.as_str().bytes().map(u16::from));
                key.push(u16::from(b'/'));
                key.extend(archive_key(path));
            }
        }

        key.into_boxed_slice()
    }
}

/// Scheme of [`Locator::Extension`].
const EXTENSION_SCHEME: &str = "ext://";

/// Base path of scenes created from the archive of extension `name`:
/// `ext://<name>/`.
pub fn extension_base_path(name: ExtensionName) -> Box<[u16]> {
    let mut path: Vec<u16> = EXTENSION_SCHEME.encode_utf16().collect();
    path.extend(name.as_str().bytes().map(u16::from));
    path.push(u16::from(b'/'));

    path.into_boxed_slice()
}

/// Base path of scenes created from section `section` of `module` with no
/// file: `section://<module hex>,<section>#`.
pub fn section_base_path(module: ModuleHandle, section: SectionName) -> Box<[u16]> {
    let mut path: Vec<u16> = "section://".encode_utf16().collect();
    push_hex(&mut path, module.0);
    path.push(u16::from(b','));
    path.extend(section.as_str().bytes().map(u16::from));
    path.push(u16::from(b'#'));

    path.into_boxed_slice()
}

/// `value` in uppercase hexadecimal without leading zeros, as `%X` writes it.
fn push_hex(out: &mut Vec<u16>, value: u32) {
    let digits = (32 - value.leading_zeros()).div_ceil(4).max(1);
    for index in (0..digits).rev() {
        let digit = (value >> (index * 4)) & 0xF;
        let unit = match digit {
            0..=9 => u32::from(b'0') + digit,
            _ => u32::from(b'A') + digit - 10,
        };
        out.push(unit as u16);
    }
}

/// Whether `path` names its own scheme or device (`XuiResolveElementPath`
/// copies those unchanged). [INFERENCE] The test (0x9216ba10) was not
/// decoded; a `:` marks an absolute path, which covers `<scheme>://` and
/// loader prefixes such as `live:`.
pub fn is_absolute(path: &[u16]) -> bool {
    path.contains(&u16::from(b':'))
}

/// `path` itself when absolute, else `base` followed by `path`
/// [INFERENCE: joined by concatenation; base paths end in `#` or `/`].
pub(crate) fn join(path: &[u16], base: Option<&[u16]>) -> Result<Vec<u16>, ResourceError> {
    if is_absolute(path) {
        return Ok(path.to_vec());
    }
    let Some(base) = base else {
        return Err(ResourceError::NoBasePath {
            path: WidePath::from_units(path),
        });
    };

    let mut joined = Vec::with_capacity(base.len() + path.len());
    joined.extend_from_slice(base);
    joined.extend_from_slice(path);

    Ok(joined)
}

/// `path` without `prefix`, compared without ASCII case.
pub fn strip_prefix_ignore_case<'a>(path: &'a [u16], prefix: &str) -> Option<&'a [u16]> {
    let head = path.get(..prefix.len())?;
    let matches = head
        .iter()
        .zip(prefix.bytes())
        .all(|(&unit, byte)| u8::try_from(unit).is_ok_and(|unit| unit.eq_ignore_ascii_case(&byte)));

    matches.then(|| &path[prefix.len()..])
}

/// `path` as archive lookup compares it: up to the first NUL, ASCII
/// lowercase, `\` as `/`.
pub(crate) fn archive_key(path: &[u16]) -> impl Iterator<Item = u16> + '_ {
    path.iter()
        .take_while(|&&unit| unit != 0)
        .map(|&unit| match u8::try_from(unit) {
            Ok(b'\\') => u16::from(b'/'),
            Ok(byte) => u16::from(byte.to_ascii_lowercase()),
            Err(_) => unit,
        })
}

/// `<module hex>,<section name>#<file>`. [INFERENCE] A module value that
/// does not fit 32 bits is malformed; the original's hex parser (0x92174c60)
/// was not decoded.
fn parse_section(path: &[u16], rest: &[u16]) -> Result<Locator, ResourceError> {
    let malformed = || ResourceError::MalformedSection {
        path: WidePath::from_units(path),
    };

    let comma = rest
        .iter()
        .position(|&unit| unit == u16::from(b','))
        .ok_or_else(malformed)?;
    let digits = &rest[..comma];
    if digits.is_empty() {
        return Err(malformed());
    }
    let mut module: u32 = 0;
    for &unit in digits {
        let digit = u8::try_from(unit)
            .ok()
            .and_then(|byte| char::from(byte).to_digit(16))
            .ok_or_else(malformed)?;
        module = module
            .checked_mul(16)
            .and_then(|value| value.checked_add(digit))
            .ok_or_else(malformed)?;
    }

    let after_module = &rest[comma + 1..];
    let hash = after_module
        .iter()
        .position(|&unit| unit == u16::from(b'#'))
        .ok_or_else(malformed)?;
    let section =
        SectionName::from_units(after_module[..hash].iter().copied()).ok_or_else(malformed)?;

    Ok(Locator::Section {
        module: ModuleHandle(module),
        section,
        path: after_module[hash + 1..].into(),
    })
}

/// `<name>/<file>`; the name is an [`ExtensionName`] in any ASCII case, as
/// the rest of a locator is.
fn parse_extension(path: &[u16], rest: &[u16]) -> Result<Locator, ResourceError> {
    let malformed = || ResourceError::MalformedExtension {
        path: WidePath::from_units(path),
    };
    let slash = rest
        .iter()
        .position(|&unit| unit == u16::from(b'/'))
        .ok_or_else(malformed)?;
    let name: Option<alloc::string::String> = rest[..slash]
        .iter()
        .map(|&unit| {
            u8::try_from(unit)
                .ok()
                .map(|byte| char::from(byte.to_ascii_lowercase()))
        })
        .collect();
    let name = name
        .as_deref()
        .and_then(ExtensionName::new)
        .ok_or_else(malformed)?;

    Ok(Locator::Extension {
        name,
        path: rest[slash + 1..].into(),
    })
}

/// `<block hex>,<size hex>`; a `#` names a file of an in-memory archive.
fn parse_memory(path: &[u16], rest: &[u16]) -> Result<Locator, ResourceError> {
    let unsupported = || ResourceError::UnsupportedScheme {
        scheme: UnsupportedScheme::Memory,
        path: WidePath::from_units(path),
    };
    if rest.contains(&u16::from(b'#')) {
        return Err(unsupported());
    }
    let malformed = || ResourceError::MalformedMemory {
        path: WidePath::from_units(path),
    };
    let comma = rest
        .iter()
        .position(|&unit| unit == u16::from(b','))
        .ok_or_else(malformed)?;
    let block = parse_hex(&rest[..comma]).ok_or_else(malformed)?;
    let size = parse_hex(&rest[comma + 1..]).ok_or_else(malformed)?;

    Ok(Locator::Memory {
        block: MemoryBlockId(block),
        size,
    })
}

/// Hexadecimal digits that fit 32 bits; `None` when empty or malformed.
fn parse_hex(digits: &[u16]) -> Option<u32> {
    if digits.is_empty() {
        return None;
    }
    digits.iter().try_fold(0u32, |value, &unit| {
        let digit = char::from(u8::try_from(unit).ok()?).to_digit(16)?;
        value.checked_mul(16)?.checked_add(digit)
    })
}

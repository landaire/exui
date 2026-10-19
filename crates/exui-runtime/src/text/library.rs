use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use core::ops::Range;

use crate::text::cache::FontSlot;
use crate::text::error::FontError;
use crate::text::error::TextError;
use crate::text::sfnt::Font;
use crate::text::xtt;

/// Contents of a font file: an `xttf` container or a plain TrueType file.
pub enum FontData {
    Static(&'static [u8]),
    Owned(Vec<u8>),
}

impl FontData {
    pub(crate) fn bytes(&self) -> &[u8] {
        match self {
            Self::Static(bytes) => bytes,
            Self::Owned(bytes) => bytes,
        }
    }

    /// The bytes in `range`, which must lie inside the data, without the
    /// rest.
    pub(crate) fn into_range(self, range: Range<usize>) -> Self {
        match self {
            Self::Static(bytes) => Self::Static(&bytes[range]),
            Self::Owned(mut bytes) => {
                bytes.truncate(range.end);
                bytes.drain(..range.start);
                bytes.shrink_to_fit();
                Self::Owned(bytes)
            }
        }
    }
}

impl fmt::Debug for FontData {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self {
            Self::Static(_) => "Static",
            Self::Owned(_) => "Owned",
        };

        write!(formatter, "FontData::{kind}({} bytes)", self.bytes().len())
    }
}

/// A parsed font file. Clones share the parsed font, so one file can back
/// several typefaces.
#[derive(Clone, Debug)]
pub struct LoadedFont(Rc<Font>);

impl LoadedFont {
    /// Parses a font file; an `xttf` container is inflated.
    pub fn load(data: FontData) -> Result<Self, FontError> {
        let font = if data.bytes().starts_with(&xtt::MAGIC) {
            let sfnt = xtt::decompress(data.bytes())?;
            Font::parse(FontData::Owned(sfnt), Some(data))?
        } else {
            Font::parse(data, None)?
        };

        Ok(Self(Rc::new(font)))
    }
}

/// `XuiRegisterTypeface` descriptor with the fonts its file URLs name,
/// which the caller resolves and loads.
#[derive(Debug)]
pub struct Typeface {
    pub primary: LoadedFont,
    /// Font asked for characters the primary font lacks.
    pub fallback: Option<LoadedFont>,
    /// Fraction of the font line height removed from the line pitch.
    pub line_height_reduction: f32,
}

/// Registered typefaces by name.
#[derive(Debug)]
pub struct FontLibrary {
    typefaces: Vec<TypefaceEntry>,
    /// Stored by name, as XUI does, so re-registering the name replaces it.
    default: Option<Vec<u16>>,
    next_generation: u32,
}

impl FontLibrary {
    pub fn new() -> Self {
        Self {
            typefaces: Vec::new(),
            default: None,
            next_generation: 0,
        }
    }

    /// Registers the fonts under `name`, replacing an earlier registration
    /// of that name.
    pub fn register(
        &mut self,
        name: &str,
        typeface: Typeface,
        is_default: bool,
    ) -> Result<(), FontError> {
        if name.contains(';') {
            return Err(FontError::TypefaceName);
        }

        let name: Vec<u16> = name.encode_utf16().collect();

        let generation = TypefaceGeneration(self.next_generation);
        self.next_generation = self.next_generation.wrapping_add(1);

        if is_default {
            self.default = Some(name.clone());
        }

        let entry = TypefaceEntry {
            name,
            primary: typeface.primary.0,
            fallback: typeface.fallback.map(|font| font.0),
            line_height_reduction: typeface.line_height_reduction,
            generation,
        };
        match self
            .typefaces
            .iter_mut()
            .find(|existing| existing.name == entry.name)
        {
            Some(existing) => *existing = entry,
            None => self.typefaces.push(entry),
        }

        Ok(())
    }

    /// Resolves an element's typeface name. Only a missing or empty name
    /// selects the default typeface; an unknown name fails.
    pub(crate) fn resolve(&self, name: Option<&[u16]>) -> Result<&TypefaceEntry, TextError> {
        let name = match name {
            Some(name) if !name.is_empty() => name,
            _ => self
                .default
                .as_deref()
                .ok_or(TextError::NoDefaultTypeface)?,
        };

        self.typefaces
            .iter()
            .find(|entry| entry.name == name)
            .ok_or_else(|| TextError::UnknownTypeface {
                name: String::from_utf16_lossy(name),
            })
    }

    pub(crate) fn by_generation(&self, generation: TypefaceGeneration) -> Option<&TypefaceEntry> {
        self.typefaces
            .iter()
            .find(|entry| entry.generation == generation)
    }
}

impl Default for FontLibrary {
    fn default() -> Self {
        Self::new()
    }
}

/// One registration of a typeface; a re-registered name gets a new value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct TypefaceGeneration(u32);

#[derive(Debug)]
pub(crate) struct TypefaceEntry {
    name: Vec<u16>,
    pub(crate) primary: Rc<Font>,
    pub(crate) fallback: Option<Rc<Font>>,
    pub(crate) line_height_reduction: f32,
    pub(crate) generation: TypefaceGeneration,
}

impl TypefaceEntry {
    /// Font of a slot. Glyph records are made from this registration, so a
    /// fallback slot only occurs when the fallback font exists.
    pub(crate) fn font(&self, slot: FontSlot) -> &Font {
        match (slot, &self.fallback) {
            (FontSlot::Fallback, Some(fallback)) => fallback,
            _ => &self.primary,
        }
    }
}

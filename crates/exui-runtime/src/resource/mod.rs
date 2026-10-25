//! Resource access: archive sections, locators, the application resolver
//! and texture loader hooks, textures and string tables.

mod error;
mod locator;
mod object;
mod strings;
mod texture;

#[cfg(test)]
mod tests;

use alloc::borrow::Cow;
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::RefCell;
use core::fmt;

use exui_core::AcceptedVersions;
use exui_core::Archive;
use xbox360_draw::image::Image;

use crate::extension::ExtensionName;
use crate::extension::ExtensionSet;

pub use self::error::OverlayError;
pub use self::error::ResourceError;
pub use self::error::StringTableError;
pub use self::error::TextureLoadError;
pub use self::error::UnsupportedScheme;
pub use self::locator::Locator;
pub use self::locator::MemoryBlockId;
pub(crate) use self::locator::archive_key;
pub use self::locator::extension_base_path;
pub use self::locator::is_absolute;
pub(crate) use self::locator::join;
pub use self::locator::section_base_path;
pub use self::locator::strip_prefix_ignore_case;
pub use self::object::TextureObject;
pub use self::object::TextureReleases;
pub use self::object::WeakTextureObject;
pub use self::strings::EntryKeys;
pub use self::strings::StringIndex;
pub use self::strings::StringTable;
pub use self::texture::CachedTexture;
pub use self::texture::SignatureLoader;
pub use self::texture::TextureCache;

/// Supplies the bytes behind `section://` and `file://` locators.
pub trait ResourceProvider {
    /// The XUIZ archive stored as image section `name` of `module`
    /// (`XexGetModuleSection`), or `None` when it does not exist or cannot be
    /// read. The bytes must stay available for the lifetime of the provider
    /// borrow.
    fn section(&self, module: ModuleHandle, name: SectionName) -> Option<&[u8]>;

    /// A `file://` resource; `path` is the text after the scheme, such as
    /// `media:/font.xtt`. `None` when it does not exist or cannot be read.
    fn file(&self, path: &[u16]) -> Option<Cow<'_, [u8]>>;
}

/// Application locator resolver (`XuiSetResolver`, method 0). XUI asks it
/// first for every path and handles its own schemes when it declines.
pub trait Resolver {
    /// The locator `path` stands for, or `None` to leave `path` to XUI.
    fn resolve(&self, path: &[u16]) -> Option<Vec<u16>>;
}

/// Application texture loader (the texture-loader callback of the XUI
/// render init). Receives image paths after base path resolution.
pub trait TextureLoader {
    fn load(&self, path: &[u16], resources: &Resources<'_>) -> Result<Image, TextureLoadError>;
}

/// Module handle written in a `section://` locator, which XUI passes to
/// `XexGetModuleSection` with the section name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModuleHandle(pub u32);

impl ModuleHandle {
    /// `0xFFFFFFFF`, which `XexGetModuleSection` takes as the module of its
    /// caller: for the title's own code, the title image.
    pub const TITLE: Self = Self(0xFFFF_FFFF);
}

/// Image section name: 1 to 8 ASCII characters.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SectionName {
    bytes: [u8; 8],
    len: u8,
}

impl SectionName {
    /// `None` unless `name` is 1 to 8 ASCII characters.
    pub fn new(name: &str) -> Option<Self> {
        Self::from_units(name.bytes().map(u16::from))
    }

    pub fn from_units(units: impl IntoIterator<Item = u16>) -> Option<Self> {
        let mut bytes = [0; 8];
        let mut len = 0;
        for unit in units {
            let byte = u8::try_from(unit).ok().filter(u8::is_ascii)?;
            *bytes.get_mut(len)? = byte;
            len += 1;
        }
        if len == 0 {
            return None;
        }

        Some(Self {
            bytes,
            len: len as u8,
        })
    }

    pub fn as_str(&self) -> &str {
        // Only ASCII bytes are stored.
        core::str::from_utf8(&self.bytes[..usize::from(self.len)]).unwrap_or_default()
    }
}

impl fmt::Display for SectionName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl fmt::Debug for SectionName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{:?}", self.as_str())
    }
}

/// Owned UTF-16 path, displayed with non-ASCII units escaped.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WidePath(pub Box<[u16]>);

impl WidePath {
    pub fn from_units(units: &[u16]) -> Self {
        Self(units.into())
    }
}

impl fmt::Display for WidePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for decoded in char::decode_utf16(self.0.iter().copied()) {
            match decoded {
                Ok(character) if character.is_ascii_graphic() || character == ' ' => {
                    write!(formatter, "{character}")?
                }
                Ok(character) => write!(formatter, "\\u{{{:04X}}}", u32::from(character))?,
                Err(error) => write!(formatter, "\\u{{{:04X}}}", error.unpaired_surrogate())?,
            }
        }

        Ok(())
    }
}

impl fmt::Debug for WidePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "\"{self}\"")
    }
}

/// Resource access of one UI: the provider, the application hooks, the
/// locale for string tables, the texture cache and the memory blocks that
/// `memory://` locators name.
pub struct Resources<'p> {
    provider: &'p dyn ResourceProvider,
    extensions: Option<&'p ExtensionSet<'p>>,
    resolver: Option<&'p dyn Resolver>,
    texture_loader: Option<&'p dyn TextureLoader>,
    locale: Option<Box<str>>,

    textures: TextureCache,
    memory: RefCell<MemoryBlocks>,
    /// Textures of released memory blocks.
    texture_releases: TextureReleases,
}

/// Registered memory blocks. Ids are not reused, so a texture cached for a
/// released block never stands for a later one.
#[derive(Debug, Default)]
struct MemoryBlocks {
    last: u32,
    blocks: BTreeMap<MemoryBlockId, Rc<[u8]>>,
}

impl<'p> Resources<'p> {
    /// Without a resolver, and with [`SignatureLoader`] as texture loader.
    pub fn new(provider: &'p dyn ResourceProvider) -> Self {
        Self {
            provider,
            extensions: None,
            resolver: None,
            texture_loader: None,
            locale: None,
            textures: TextureCache::new(),
            memory: RefCell::new(MemoryBlocks::default()),
            texture_releases: TextureReleases::new(),
        }
    }

    /// Registers `bytes` and returns the `memory://` locator that names
    /// them, the way the original formats a buffer address and size.
    /// `None` when the block is larger than the 32-bit size field or every
    /// block id is used.
    pub fn insert_memory(&self, bytes: Rc<[u8]>) -> Option<Box<[u16]>> {
        let size = u32::try_from(bytes.len()).ok()?;
        let mut memory = self.memory.borrow_mut();
        memory.last = memory.last.checked_add(1)?;
        let block = MemoryBlockId(memory.last);
        memory.blocks.insert(block, bytes);
        Some(Locator::memory_path(block, size))
    }

    /// Releases the block a `memory://` locator from
    /// [`Self::insert_memory`] names, with the texture loaded from it.
    /// Other paths are ignored.
    pub fn remove_memory(&self, path: &[u16]) {
        let Ok(Locator::Memory { block, .. }) = Locator::parse(path) else {
            return;
        };
        let Some(bytes) = self.memory.borrow_mut().blocks.remove(&block) else {
            return;
        };

        // insert_memory checked that the length fits.
        let Ok(size) = u32::try_from(bytes.len()) else {
            return;
        };
        let key = Locator::Memory { block, size }.cache_key();
        if let Some(texture) = self.textures.remove(&key) {
            self.texture_releases.release(texture.id);
        }
    }

    /// Bytes of a block from [`Self::insert_memory`], shared with the
    /// block instead of copied.
    pub fn read_memory(&self, block: MemoryBlockId, size: u32) -> Result<Rc<[u8]>, ResourceError> {
        self.memory
            .borrow()
            .blocks
            .get(&block)
            .filter(|bytes| u32::try_from(bytes.len()).ok() == Some(size))
            .map(Rc::clone)
            .ok_or(ResourceError::MemoryBlockNotFound { block, size })
    }

    /// Textures of released memory blocks, to destroy before the next draw.
    pub fn texture_releases(&self) -> &TextureReleases {
        &self.texture_releases
    }

    pub fn provider(&self) -> &'p dyn ResourceProvider {
        self.provider
    }

    /// The extensions whose files `ext://` names and whose patches and
    /// string overlays apply; `None` reads the archives as they are.
    pub fn set_extensions(&mut self, extensions: Option<&'p ExtensionSet<'p>>) {
        self.extensions = extensions;
    }

    pub fn extensions(&self) -> Option<&'p ExtensionSet<'p>> {
        self.extensions
    }

    pub fn set_resolver(&mut self, resolver: Option<&'p dyn Resolver>) {
        self.resolver = resolver;
    }

    /// `None` selects [`SignatureLoader`].
    pub fn set_texture_loader(&mut self, loader: Option<&'p dyn TextureLoader>) {
        self.texture_loader = loader;
    }

    /// Locale folder name (`XuiSetLocale`), such as `de-de`.
    pub fn locale(&self) -> Option<&str> {
        self.locale.as_deref()
    }

    /// `None` selects the base strings.
    pub fn set_locale(&mut self, locale: Option<&str>) {
        self.locale = locale.map(Into::into);
    }

    pub fn textures(&self) -> &TextureCache {
        &self.textures
    }

    pub fn textures_mut(&mut self) -> &mut TextureCache {
        &mut self.textures
    }

    /// Resolves `path` against `base` like XUI: a relative path is
    /// appended to the base path, then the resolver is asked, then the XUI
    /// schemes apply.
    pub fn resolve(&self, path: &[u16], base: Option<&[u16]>) -> Result<Locator, ResourceError> {
        let full = locator::join(path, base)?;
        if let Some(resolved) = self.resolver.and_then(|resolver| resolver.resolve(&full)) {
            return Locator::parse(&resolved);
        }

        Locator::parse(&full)
    }

    /// Bytes of the resource `locator` names. Archive files borrow the
    /// provider's section; memory blocks are copied, see
    /// [`Self::read_memory`].
    pub fn read(&self, locator: &Locator) -> Result<Cow<'p, [u8]>, ResourceError> {
        match locator {
            Locator::Section {
                module,
                section,
                path,
            } => self
                .open_in_section(*module, *section, path)
                .map(Cow::Borrowed),
            Locator::File { path } => {
                self.provider
                    .file(path)
                    .ok_or_else(|| ResourceError::FileNotFound {
                        path: WidePath::from_units(path),
                    })
            }
            Locator::Memory { block, size } => {
                Ok(Cow::Owned(self.read_memory(*block, *size)?.to_vec()))
            }
            Locator::Extension { name, path } => {
                self.open_in_extension(*name, path).map(Cow::Borrowed)
            }
        }
    }

    /// A file of the archive of enabled extension `name`.
    pub fn open_in_extension(
        &self,
        name: ExtensionName,
        path: &[u16],
    ) -> Result<&'p [u8], ResourceError> {
        let extension = self
            .extensions
            .and_then(|extensions| extensions.extension(name))
            .ok_or(ResourceError::ExtensionNotEnabled { name })?;

        extension
            .open(path)
            .map_err(|source| ResourceError::NotInExtension {
                name,
                path: WidePath::from_units(path),
                source,
            })
    }

    /// [`Self::resolve`] followed by [`Self::read`].
    pub fn read_path(
        &self,
        path: &[u16],
        base: Option<&[u16]>,
    ) -> Result<Cow<'p, [u8]>, ResourceError> {
        self.read(&self.resolve(path, base)?)
    }

    /// A file inside the XUIZ archive of `section` of `module`.
    pub fn open_in_section(
        &self,
        module: ModuleHandle,
        section: SectionName,
        path: &[u16],
    ) -> Result<&'p [u8], ResourceError> {
        let data = self
            .provider
            .section(module, section)
            .ok_or(ResourceError::SectionNotFound { module, section })?;
        // The dash 6717 XUI reads version 1 only. Version 3 is also read
        // because the shared system resources of system software 2.0.21256.0
        // are a version 3 archive.
        let archive = Archive::parse(data, AcceptedVersions::ALL)
            .map_err(|source| ResourceError::Archive { section, source })?;

        let bytes = archive
            .open_utf16(path)
            .map_err(|source| ResourceError::NotInArchive {
                section,
                path: WidePath::from_units(path),
                source,
            })?;

        // Extensions patch the title's own archives only.
        let patched = self
            .extensions
            .filter(|_| module == ModuleHandle::TITLE)
            .and_then(|extensions| extensions.patched(section, path, bytes));
        match patched {
            None => Ok(bytes),
            Some(result) => result.map_err(|source| ResourceError::Patch {
                section,
                path: WidePath::from_units(path),
                source,
            }),
        }
    }
}

impl fmt::Debug for Resources<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Resources")
            .field("resolver", &self.resolver.is_some())
            .field("texture_loader", &self.texture_loader.is_some())
            .field("locale", &self.locale)
            .field("textures", &self.textures)
            .finish_non_exhaustive()
    }
}

//! Layout, big-endian: magic `OBXT`, u16 version 1, the name, the scene
//! patches, the string overlays and the blade tab. A string is a u16 byte
//! length and UTF-8 bytes, a list a u16 count and its items, an optional
//! string a u8 0 or 1 and the string. The blade tab is a u8, 0 for none.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use core::num::NonZeroU8;

use winnow::Parser;
use winnow::binary::be_u16;
use winnow::binary::be_u32;
use winnow::binary::u8 as be_u8;
use winnow::error::ContextError;
use winnow::token::take;

use crate::DecodeError;
use crate::EncodeError;
use crate::ExtensionName;

/// Archive entry that holds the descriptor.
pub const DESCRIPTOR_PATH: &str = "extension.bin";

const MAGIC: &[u8; 4] = b"OBXT";
const VERSION: u16 = 1;

const KEY_INDEX: u8 = 0;
const KEY_ID: u8 = 1;
const KEY_NAME: u8 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Descriptor {
    pub name: ExtensionName,
    pub patches: Vec<ScenePatch>,
    pub strings: Vec<StringOverlay>,
    /// Blade the extension adds to the dashboard's tab scene.
    pub blade: Option<BladeTab>,
}

impl Descriptor {
    pub fn to_bytes(&self) -> Result<Vec<u8>, EncodeError> {
        let mut out = Encoder(Vec::new());
        out.0.extend_from_slice(MAGIC);
        out.u16(VERSION);
        out.string(self.name.as_str())?;

        out.count(self.patches.len())?;
        for patch in &self.patches {
            out.target(&patch.target)?;
            out.count(patch.remove.len())?;
            for path in &patch.remove {
                out.path(path)?;
            }
            out.option(patch.overlay.as_deref())?;
            out.count(patch.holds.len())?;
            for hold in &patch.holds {
                out.path(&hold.element)?;
                out.string(&hold.source)?;
                out.count(hold.frames.len())?;
                for frame in &hold.frames {
                    out.string(frame)?;
                }
            }
        }

        out.count(self.strings.len())?;
        for overlay in &self.strings {
            out.target(&overlay.target)?;
            out.option(overlay.locale.as_deref())?;
            out.count(overlay.entries.len())?;
            for entry in &overlay.entries {
                match &entry.key {
                    StringKey::Index(index) => {
                        out.0.push(KEY_INDEX);
                        out.u32(*index);
                    }
                    StringKey::Id(id) => {
                        out.0.push(KEY_ID);
                        out.u32(*id);
                    }
                    StringKey::Name(name) => {
                        out.0.push(KEY_NAME);
                        out.string(name)?;
                    }
                }
                out.string(&entry.value)?;
            }
        }

        out.0.push(self.blade.map_or(0, |blade| blade.0.get()));

        Ok(out.0)
    }

    pub fn parse(data: &[u8]) -> Result<Self, DecodeError> {
        let mut input = data;
        let malformed = |input: &[u8]| DecodeError::Malformed {
            offset: data.len() - input.len(),
        };

        let magic: &[u8] = take(4usize)
            .parse_next(&mut input)
            .map_err(|_: ContextError| malformed(input))?;
        if magic != MAGIC {
            return Err(DecodeError::Magic);
        }
        let version = be_u16
            .parse_next(&mut input)
            .map_err(|_: ContextError| malformed(input))?;
        if version != VERSION {
            return Err(DecodeError::Version { version });
        }

        let mut reader = Decoder { input, data };
        let name = ExtensionName::new(&reader.string()?).ok_or(DecodeError::Name)?;

        // Counts come from the file, so lists grow with the items actually
        // read instead of reserving the count up front.
        let patch_count = reader.u16()?;
        let mut patches = Vec::new();
        for _ in 0..patch_count {
            let target = reader.target()?;
            let remove_count = reader.u16()?;
            let mut remove = Vec::new();
            for _ in 0..remove_count {
                remove.push(reader.path()?);
            }
            let overlay = reader.option()?;
            let hold_count = reader.u16()?;
            let mut holds = Vec::new();
            for _ in 0..hold_count {
                let element = reader.path()?;
                let source = reader.string()?;
                let frame_count = reader.u16()?;
                let mut frames = Vec::new();
                for _ in 0..frame_count {
                    frames.push(reader.string()?);
                }
                holds.push(PoseHold {
                    element,
                    source,
                    frames,
                });
            }
            patches.push(ScenePatch {
                target,
                remove,
                overlay,
                holds,
            });
        }

        let overlay_count = reader.u16()?;
        let mut strings = Vec::new();
        for _ in 0..overlay_count {
            let target = reader.target()?;
            let locale = reader.option()?;
            let entry_count = reader.u16()?;
            let mut entries = Vec::new();
            for _ in 0..entry_count {
                let key = match reader.u8()? {
                    KEY_INDEX => StringKey::Index(reader.u32()?),
                    KEY_ID => StringKey::Id(reader.u32()?),
                    KEY_NAME => StringKey::Name(reader.string()?),
                    _ => return Err(reader.malformed()),
                };
                let value = reader.string()?;
                entries.push(StringEntry { key, value });
            }
            strings.push(StringOverlay {
                target,
                locale,
                entries,
            });
        }

        let blade = NonZeroU8::new(reader.u8()?).map(BladeTab);
        if !reader.input.is_empty() {
            return Err(DecodeError::Trailing {
                trailing: reader.input.len(),
            });
        }

        Ok(Self {
            name,
            patches,
            strings,
            blade,
        })
    }
}

/// A file of a dashboard archive: section `section` of the title image
/// (such as `dashmain`) and the path inside its archive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntryTarget {
    pub section: String,
    pub path: String,
}

/// Changes to one scene file, applied in order: removals, the overlay,
/// then the pose holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScenePatch {
    pub target: EntryTarget,
    pub remove: Vec<ElementPath>,
    /// Archive entry of the extension holding the compiled overlay scene.
    pub overlay: Option<String>,
    pub holds: Vec<PoseHold>,
}

/// Keys every timeline of `element` at the named frames `frames` with the
/// value it has at the named frame `source`, so a new frame range shows the
/// pose of an existing one. Keyframes already at those frames are kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PoseHold {
    pub element: ElementPath,
    pub source: String,
    pub frames: Vec<String>,
}

/// Element of a scene named by the `Id`s of its ancestors below the root,
/// then its own. Empty names the root.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ElementPath(pub Vec<String>);

impl ElementPath {
    /// `RootScene/Tab5`; an empty string is the root.
    pub fn parse(text: &str) -> Option<Self> {
        if text.is_empty() {
            return Some(Self::default());
        }
        let ids: Vec<String> = text.split('/').map(String::from).collect();
        if ids.iter().any(String::is_empty) {
            return None;
        }

        Some(Self(ids))
    }
}

impl fmt::Display for ElementPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, id) in self.0.iter().enumerate() {
            if index > 0 {
                formatter.write_str("/")?;
            }
            formatter.write_str(id)?;
        }

        Ok(())
    }
}

/// Entries added to or replaced in a string table file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringOverlay {
    pub target: EntryTarget,
    /// Applies only while this locale (such as `de-de`) is set; `None`
    /// applies always.
    pub locale: Option<String>,
    pub entries: Vec<StringEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StringEntry {
    pub key: StringKey,
    pub value: String,
}

/// Key of a string table entry, matching the table's key kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StringKey {
    /// Position in an index-keyed table. The position one past the last
    /// entry appends.
    Index(u32),
    Id(u32),
    Name(String),
}

/// One-based tab number of a blade, as in the tab scene's `Tab<n>`
/// elements and `<n>To<m>` frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BladeTab(pub NonZeroU8);

struct Encoder(Vec<u8>);

impl Encoder {
    fn u16(&mut self, value: u16) {
        self.0.extend_from_slice(&value.to_be_bytes());
    }

    fn u32(&mut self, value: u32) {
        self.0.extend_from_slice(&value.to_be_bytes());
    }

    fn count(&mut self, count: usize) -> Result<(), EncodeError> {
        let count = u16::try_from(count).map_err(|_| EncodeError::ListTooLong)?;
        self.u16(count);
        Ok(())
    }

    fn string(&mut self, text: &str) -> Result<(), EncodeError> {
        let length = u16::try_from(text.len()).map_err(|_| EncodeError::StringTooLong)?;
        self.u16(length);
        self.0.extend_from_slice(text.as_bytes());
        Ok(())
    }

    fn option(&mut self, text: Option<&str>) -> Result<(), EncodeError> {
        match text {
            Some(text) => {
                self.0.push(1);
                self.string(text)
            }
            None => {
                self.0.push(0);
                Ok(())
            }
        }
    }

    fn target(&mut self, target: &EntryTarget) -> Result<(), EncodeError> {
        self.string(&target.section)?;
        self.string(&target.path)
    }

    fn path(&mut self, path: &ElementPath) -> Result<(), EncodeError> {
        self.count(path.0.len())?;
        for id in &path.0 {
            self.string(id)?;
        }

        Ok(())
    }
}

struct Decoder<'a> {
    input: &'a [u8],
    data: &'a [u8],
}

impl Decoder<'_> {
    fn malformed(&self) -> DecodeError {
        DecodeError::Malformed {
            offset: self.data.len() - self.input.len(),
        }
    }

    fn u8(&mut self) -> Result<u8, DecodeError> {
        be_u8
            .parse_next(&mut self.input)
            .map_err(|_: ContextError| self.malformed())
    }

    fn u16(&mut self) -> Result<u16, DecodeError> {
        be_u16
            .parse_next(&mut self.input)
            .map_err(|_: ContextError| self.malformed())
    }

    fn u32(&mut self) -> Result<u32, DecodeError> {
        be_u32
            .parse_next(&mut self.input)
            .map_err(|_: ContextError| self.malformed())
    }

    fn string(&mut self) -> Result<String, DecodeError> {
        let length = self.u16()?;
        let bytes: &[u8] = take(usize::from(length))
            .parse_next(&mut self.input)
            .map_err(|_: ContextError| self.malformed())?;
        let text = core::str::from_utf8(bytes).map_err(|_| self.malformed())?;

        Ok(String::from(text))
    }

    fn option(&mut self) -> Result<Option<String>, DecodeError> {
        match self.u8()? {
            0 => Ok(None),
            1 => self.string().map(Some),
            _ => Err(self.malformed()),
        }
    }

    fn path(&mut self) -> Result<ElementPath, DecodeError> {
        let count = self.u16()?;
        let mut ids = Vec::new();
        for _ in 0..count {
            let id = self.string()?;
            if id.is_empty() {
                return Err(self.malformed());
            }
            ids.push(id);
        }

        Ok(ElementPath(ids))
    }

    fn target(&mut self) -> Result<EntryTarget, DecodeError> {
        Ok(EntryTarget {
            section: self.string()?,
            path: self.string()?,
        })
    }
}

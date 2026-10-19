//! Extensions: archives of authored content that the runtime layers over
//! the base archives without changing them.
//!
//! A scene file that a patch targets is patched when it is first read: the
//! file is parsed, converted to an owned tree, patched, and written back to
//! XUR bytes, which every later read returns. Instantiation then runs on
//! those bytes exactly as on an unpatched file. The writer reproduces every
//! 6717 scene byte for byte from its tree, so the round trip itself changes
//! nothing. Files no patch targets are read as they are.

mod error;
mod overlay;

#[cfg(test)]
mod tests;

use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cell::OnceCell;

use exui_core::AcceptedVersions;
use exui_core::Archive;
use exui_core::ArchiveError;
use exui_core::Document;
use exui_core::Registry;
use exui_core::tree::Tree;
use exui_core::writer::WriteOptions;
use exui_core::writer::write;
use exui_extension::EntryTarget;
use exui_extension::StringOverlay;

use crate::resource::SectionName;
use crate::resource::archive_key;

pub use self::error::ExtensionError;
pub use self::error::PatchError;
pub use self::error::ScenePatchError;
pub use self::error::SharedPatchError;
pub use exui_extension::DESCRIPTOR_PATH;
pub use exui_extension::Descriptor;
pub use exui_extension::ExtensionName;

/// Position of a patch among the `[[patch]]` entries of its extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PatchIndex(pub usize);

/// One loaded extension.
#[derive(Debug)]
pub struct Extension<'p> {
    archive: Archive<'p>,
    descriptor: Descriptor,
}

impl<'p> Extension<'p> {
    fn parse(
        name: ExtensionName,
        bytes: &'p [u8],
        versions: AcceptedVersions,
    ) -> Result<Self, ExtensionError> {
        let archive = Archive::parse(bytes, versions)
            .map_err(|source| ExtensionError::Archive { name, source })?;
        let descriptor_bytes = archive
            .open(DESCRIPTOR_PATH)
            .map_err(|source| match source {
                ArchiveError::NotFound => ExtensionError::NoDescriptor { name },
                source => ExtensionError::DescriptorEntry { name, source },
            })?;
        let descriptor = Descriptor::parse(descriptor_bytes)
            .map_err(|source| ExtensionError::Descriptor { name, source })?;
        if descriptor.name != name {
            return Err(ExtensionError::NameMismatch {
                name,
                described: descriptor.name,
            });
        }

        Ok(Self {
            archive,
            descriptor,
        })
    }

    pub fn name(&self) -> ExtensionName {
        self.descriptor.name
    }

    pub fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }

    /// A file of the extension's archive.
    pub fn open(&self, path: &[u16]) -> Result<&'p [u8], ArchiveError> {
        self.archive.open_utf16(path)
    }
}

/// The enabled extensions, in the order they apply.
#[derive(Debug)]
pub struct ExtensionSet<'p> {
    /// Classes of the scenes, as the UI registers them; patching parses and
    /// writes scene files with it.
    registry: Registry,
    extensions: Vec<Extension<'p>>,
    patched: Vec<PatchedEntry>,
    /// In extension order; within one extension, overlays without a locale
    /// come first, so a locale's entries replace them.
    string_overlays: Vec<OverlayRef>,
}

impl<'p> ExtensionSet<'p> {
    /// Loads each `(name, archive bytes)` pair. `registry` must hold the
    /// classes of the UI that reads the patched scenes; `versions` are the
    /// archive versions that UI reads.
    pub fn new(
        registry: Registry,
        versions: AcceptedVersions,
        archives: impl IntoIterator<Item = (ExtensionName, &'p [u8])>,
    ) -> Result<Self, ExtensionError> {
        let mut extensions: Vec<Extension<'p>> = Vec::new();
        for (name, bytes) in archives {
            if extensions.iter().any(|extension| extension.name() == name) {
                return Err(ExtensionError::Duplicate { name });
            }
            extensions.push(Extension::parse(name, bytes, versions)?);
        }

        let mut patched: Vec<PatchedEntry> = Vec::new();
        let mut string_overlays: Vec<OverlayRef> = Vec::new();
        for (extension_index, extension) in extensions.iter().enumerate() {
            let extension_index = ExtensionIndex(extension_index);
            for (patch_index, patch) in extension.descriptor.patches.iter().enumerate() {
                let (section, path) = target_key(extension.name(), &patch.target)?;
                let position = patched
                    .iter()
                    .position(|entry| entry.section == section && entry.path == path);
                let entry = match position {
                    Some(position) => &mut patched[position],
                    None => {
                        patched.push(PatchedEntry {
                            section,
                            path,
                            patches: Vec::new(),
                            bytes: OnceCell::new(),
                        });
                        let last = patched.len() - 1;
                        &mut patched[last]
                    }
                };
                entry.patches.push(PatchRef {
                    extension: extension_index,
                    patch: PatchIndex(patch_index),
                });
            }

            let mut overlays: Vec<OverlayRef> = Vec::new();
            for (overlay_index, overlay) in extension.descriptor.strings.iter().enumerate() {
                let (section, path) = target_key(extension.name(), &overlay.target)?;
                overlays.push(OverlayRef {
                    extension: extension_index,
                    overlay: OverlayIndex(overlay_index),
                    section,
                    path,
                });
            }
            let strings = &extension.descriptor.strings;
            overlays.sort_by_key(|overlay| strings[overlay.overlay.0].locale.is_some());
            string_overlays.append(&mut overlays);
        }

        Ok(Self {
            registry,
            extensions,
            patched,
            string_overlays,
        })
    }

    pub fn extensions(&self) -> &[Extension<'p>] {
        &self.extensions
    }

    pub fn extension(&self, name: ExtensionName) -> Option<&Extension<'p>> {
        self.extensions
            .iter()
            .find(|extension| extension.name() == name)
    }

    /// The patched bytes of file `path` of title section `section`, or
    /// `None` when no patch targets it. `original` is the file's bytes.
    pub(crate) fn patched(
        &'p self,
        section: SectionName,
        path: &[u16],
        original: &[u8],
    ) -> Option<Result<&'p [u8], SharedPatchError>> {
        let entry = self.patched.iter().find(|entry| {
            entry.section == section && archive_key(path).eq(entry.path.iter().copied())
        })?;
        let bytes = entry.bytes.get_or_init(|| {
            self.patch(entry, original)
                .map_err(|error| SharedPatchError(Arc::new(error)))
        });

        Some(bytes.as_ref().map(|bytes| &bytes[..]).map_err(Clone::clone))
    }

    /// String overlays for file `path` of title section `section` that
    /// apply under `locale`, in the order they apply, with their
    /// extension.
    pub(crate) fn string_overlays<'s>(
        &'s self,
        section: SectionName,
        path: &'s [u16],
        locale: Option<&'s str>,
    ) -> impl Iterator<Item = (ExtensionName, &'s StringOverlay)> + 's {
        self.string_overlays
            .iter()
            .filter(move |overlay| {
                overlay.section == section && archive_key(path).eq(overlay.path.iter().copied())
            })
            .map(|overlay| {
                let extension = &self.extensions[overlay.extension.0];
                (
                    extension.name(),
                    &extension.descriptor.strings[overlay.overlay.0],
                )
            })
            .filter(move |(_, overlay)| {
                overlay.locale.as_deref().is_none_or(|wanted| {
                    locale.is_some_and(|locale| locale.eq_ignore_ascii_case(wanted))
                })
            })
    }

    fn patch(&self, entry: &PatchedEntry, original: &[u8]) -> Result<Box<[u8]>, PatchError> {
        let document = Document::parse(original, &self.registry).map_err(PatchError::Parse)?;
        let allocation_hints = document.allocation_hints().is_some();
        let mut tree = Tree::from_document(&document, &self.registry).map_err(PatchError::Tree)?;

        for patch in &entry.patches {
            let extension = &self.extensions[patch.extension.0];
            self.apply(&mut tree, extension, patch.patch)
                .map_err(|source| PatchError::Extension {
                    name: extension.name(),
                    patch: patch.patch,
                    source,
                })?;
        }

        let bytes = write(&tree, &self.registry, WriteOptions { allocation_hints })
            .map_err(PatchError::Write)?;

        Ok(bytes.into_boxed_slice())
    }

    fn apply(
        &self,
        tree: &mut Tree,
        extension: &Extension<'p>,
        index: PatchIndex,
    ) -> Result<(), ScenePatchError> {
        let patch = &extension.descriptor.patches[index.0];
        for path in &patch.remove {
            overlay::remove(&mut tree.root, path)?;
        }
        if let Some(overlay_path) = &patch.overlay {
            let overlay = self.overlay_tree(extension, overlay_path)?;
            overlay::merge(&mut tree.root, overlay.root)?;
        }
        for hold in &patch.holds {
            overlay::hold(&mut tree.root, hold)?;
        }

        Ok(())
    }

    fn overlay_tree(
        &self,
        extension: &Extension<'p>,
        overlay_path: &str,
    ) -> Result<Tree, ScenePatchError> {
        let units: Vec<u16> = overlay_path.encode_utf16().collect();
        let bytes = extension
            .open(&units)
            .map_err(|source| ScenePatchError::OverlayNotFound {
                path: String::from(overlay_path),
                source,
            })?;
        let document = Document::parse(bytes, &self.registry).map_err(|source| {
            ScenePatchError::OverlayParse {
                path: String::from(overlay_path),
                source,
            }
        })?;

        Tree::from_document(&document, &self.registry).map_err(|source| {
            ScenePatchError::OverlayTree {
                path: String::from(overlay_path),
                source,
            }
        })
    }
}

/// Index of an extension in its [`ExtensionSet`], in load order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ExtensionIndex(usize);

/// Position of a string overlay among the `[[strings]]` entries of its
/// extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OverlayIndex(usize);

#[derive(Clone, Copy, Debug)]
struct PatchRef {
    extension: ExtensionIndex,
    patch: PatchIndex,
}

/// A scene file some patch targets, and its patched bytes once read.
#[derive(Debug)]
struct PatchedEntry {
    section: SectionName,
    /// As [`archive_key`] gives it.
    path: Vec<u16>,
    /// In load order.
    patches: Vec<PatchRef>,
    bytes: OnceCell<Result<Box<[u8]>, SharedPatchError>>,
}

#[derive(Debug)]
struct OverlayRef {
    extension: ExtensionIndex,
    overlay: OverlayIndex,
    section: SectionName,
    /// As [`archive_key`] gives it.
    path: Vec<u16>,
}

/// The section and [`archive_key`] of `target`.
fn target_key(
    name: ExtensionName,
    target: &EntryTarget,
) -> Result<(SectionName, Vec<u16>), ExtensionError> {
    let Some(section) = SectionName::new(&target.section) else {
        return Err(ExtensionError::InvalidSection {
            name,
            section: target.section.clone(),
        });
    };
    // The archive lookup ends a path at its first NUL.
    let units: Vec<u16> = target.path.encode_utf16().collect();
    if units.is_empty() || units.contains(&0) {
        return Err(ExtensionError::InvalidPath {
            name,
            path: target.path.clone(),
        });
    }

    Ok((section, archive_key(&units).collect()))
}

use std::format;
use std::string::String;
use std::vec::Vec;

use xbox360_draw::image::Image;
use xbox360_raster::SoftwareRenderer;

use super::*;
use crate::test_support::Bytes;
use crate::test_support::MapProvider;
use crate::test_support::build_archive;
use crate::test_support::encode_png;

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

fn describe(result: Result<Locator, ResourceError>) -> String {
    match result {
        Ok(Locator::Section {
            module,
            section,
            path,
        }) => format!("section {:X} {section} {}", module.0, WidePath(path)),
        Ok(Locator::File { path }) => format!("file {}", WidePath(path)),
        Ok(Locator::Memory { block, size }) => format!("memory {:X} {size:X}", block.0),
        Ok(Locator::Extension { name, path }) => format!("extension {name} {}", WidePath(path)),
        Err(error) => format!("error {error}"),
    }
}

#[test]
fn memory_blocks_are_read_through_their_locator_until_removed() {
    let provider = MapProvider::default();
    let resources = Resources::new(&provider);
    let first = resources.insert_memory(Rc::from(&b"tile"[..])).unwrap();
    let second = resources.insert_memory(Rc::from(&[0u8; 0x1F][..])).unwrap();

    let lines = [
        format!(
            "{} -> {}",
            WidePath::from_units(&first),
            describe(resources.resolve(&first, None))
        ),
        format!(
            "{} -> {}",
            WidePath::from_units(&second),
            describe(resources.resolve(&second, None))
        ),
        format!(
            "mismatched size -> {}",
            describe(resources.resolve(&units("memory://1,5"), None))
        ),
        format!(
            "malformed -> {}",
            describe(resources.resolve(&units("memory://1"), None))
        ),
    ];
    assert_eq!(resources.read_path(&first, None).unwrap().as_ref(), b"tile");
    assert!(resources.read_path(&units("memory://1,5"), None).is_err());
    resources.remove_memory(&first);
    assert!(matches!(
        resources.read_path(&first, None),
        Err(ResourceError::MemoryBlockNotFound { .. })
    ));
    assert_eq!(resources.read_path(&second, None).unwrap().len(), 0x1F);

    insta::assert_snapshot!(lines.join("\n"), @r"
    memory://1,4 -> memory 1 4
    memory://2,1F -> memory 2 1F
    mismatched size -> memory 1 5
    malformed -> error memory locator memory://1 is not `memory://<block>,<size>`
    ");
}

#[test]
fn removing_a_memory_block_releases_its_texture() {
    let png = encode_png(1, 1, &[[255, 0, 0, 255]]);
    let provider = MapProvider::default();
    let mut resources = Resources::new(&provider);
    let mut store = SoftwareRenderer::new();
    let kept = resources.insert_memory(Rc::from(&png[..])).unwrap();
    let removed = resources.insert_memory(Rc::from(&png[..])).unwrap();

    let kept_texture = resources
        .texture(&kept, None, &mut store)
        .expect("decodes the block");
    let removed_texture = resources
        .texture(&removed, None, &mut store)
        .expect("decodes the block");
    resources.remove_memory(&removed);
    resources.remove_memory(&removed);

    assert_eq!(resources.textures().len(), 1);
    assert_eq!(resources.texture_releases().take(), [removed_texture.id]);
    assert_eq!(
        resources.texture(&kept, None, &mut store).expect("cached"),
        kept_texture
    );
    assert!(matches!(
        resources.texture(&removed, None, &mut store),
        Err(TextureLoadError::Resource(
            ResourceError::MemoryBlockNotFound { .. }
        ))
    ));
}

/// Maps `alias://<rest>` into the `shared` section.
struct AliasResolver;

impl Resolver for AliasResolver {
    fn resolve(&self, path: &[u16]) -> Option<Vec<u16>> {
        let rest = strip_prefix_ignore_case(path, "alias://")?;
        let mut resolved = units("section://0,shared#");
        resolved.extend_from_slice(rest);

        Some(resolved)
    }
}

#[test]
fn locators_resolve_relative_paths_then_the_resolver_then_xui_schemes() {
    let provider = MapProvider::default();
    let mut resources = Resources::new(&provider);
    let resolver = AliasResolver;
    resources.set_resolver(Some(&resolver));
    let base = units("section://92000000,scenes#");
    let cases = [
        "section://1A,skin#skin.xur",
        "SECTION://1a,skin#Upper.png",
        "section://1A,toolongname#skin.xur",
        "section://zz,skin#skin.xur",
        "section://100000000,skin#skin.xur",
        "file://media:/font.xtt",
        "memory://1,2#x",
        "http://example/x.png",
        "alias://button.png",
        "ALIAS://Button.png",
        "icon.png",
        "sub\\Image.PNG",
    ];

    let lines: Vec<String> = cases
        .iter()
        .map(|case| {
            format!(
                "{case} -> {}",
                describe(resources.resolve(&units(case), Some(&base)))
            )
        })
        .collect();
    let without_base = describe(resources.resolve(&units("scene.xur"), None));

    insta::assert_snapshot!(lines.join("\n") + "\nno base: " + &without_base);
}

#[test]
fn section_names_are_short_ascii() {
    assert_eq!(
        SectionName::new("scenes").map(|name| String::from(name.as_str())),
        Some(String::from("scenes"))
    );
    assert_eq!(
        &*section_base_path(
            ModuleHandle(0x9200_0000),
            SectionName::new("scenes").unwrap()
        ),
        units("section://92000000,scenes#").as_slice()
    );
    assert_eq!(
        &*section_base_path(ModuleHandle(0), SectionName::new("scenes").unwrap()),
        units("section://0,scenes#").as_slice()
    );
    assert_eq!(
        &*section_base_path(ModuleHandle::TITLE, SectionName::new("scenes").unwrap()),
        units("section://FFFFFFFF,scenes#").as_slice()
    );
    assert_eq!(SectionName::new("ninechars"), None);
    assert_eq!(SectionName::new(""), None);
    assert_eq!(SectionName::from_units([0x100]), None);
}

#[test]
fn cache_keys_normalize_archive_paths() {
    let a = Locator::parse(&units("section://0,scenes#Images\\Logo.PNG")).unwrap();
    let b = Locator::parse(&units("section://00,scenes#images/logo.png")).unwrap();
    let other_module = Locator::parse(&units("section://5,scenes#images/logo.png")).unwrap();

    assert_eq!(a.cache_key(), b.cache_key());
    assert_ne!(a.cache_key(), other_module.cache_key());
}

#[test]
fn textures_load_once_per_resolved_path() {
    let png = encode_png(2, 1, &[[0, 0, 255, 255], [0, 255, 0, 255]]);
    let provider =
        MapProvider::default().with_section("scenes", build_archive(&[("img/a.png", &png)]));
    let mut resources = Resources::new(&provider);
    let mut store = SoftwareRenderer::new();
    let base = units("section://0,scenes#");

    let first = resources
        .texture(&units("img/a.png"), Some(&base), &mut store)
        .expect("loads");
    let second = resources
        .texture(&units("section://0,scenes#IMG\\A.png"), None, &mut store)
        .expect("cached");
    let missing = resources.texture(&units("img/b.png"), Some(&base), &mut store);

    assert_eq!(first, second);
    assert_eq!((first.width.get(), first.height.get()), (2, 1));
    assert_eq!(resources.textures().len(), 1);
    assert!(matches!(
        missing,
        Err(TextureLoadError::Resource(
            ResourceError::NotInArchive { .. }
        ))
    ));
}

/// Declines `skip:` paths and decodes everything else by signature.
struct SkippingLoader;

impl TextureLoader for SkippingLoader {
    fn load(&self, path: &[u16], resources: &Resources<'_>) -> Result<Image, TextureLoadError> {
        if strip_prefix_ignore_case(path, "skip:").is_some() {
            return Err(TextureLoadError::UnsupportedPath {
                path: WidePath::from_units(path),
            });
        }

        SignatureLoader.load(path, resources)
    }
}

#[test]
fn texture_loader_receives_absolute_paths_unchanged() {
    let png = encode_png(1, 1, &[[255, 0, 0, 128]]);
    let provider = MapProvider::default().with_section("scenes", build_archive(&[("a.png", &png)]));
    let loader = SkippingLoader;
    let mut resources = Resources::new(&provider);
    resources.set_texture_loader(Some(&loader));
    let mut store = SoftwareRenderer::new();
    let base = units("section://0,scenes#");

    let skipped = resources.texture(&units("skip:tile"), Some(&base), &mut store);
    let loaded = resources.texture(&units("a.png"), Some(&base), &mut store);

    assert!(matches!(
        skipped,
        Err(TextureLoadError::UnsupportedPath { .. })
    ));
    assert!(loaded.is_ok());
}

fn string_table(flags: u8, entries: &[&str]) -> Vec<u8> {
    let mut body = Bytes::default();
    for entry in entries {
        body.u16(entry.encode_utf16().count() as u16).wide(entry);
    }
    let mut out = Bytes::default();
    out.raw(b"XUIS")
        .u8(1)
        .u8(flags)
        .u32((12 + body.0.len()) as u32)
        .u16(entries.len() as u16)
        .raw(&body.0);

    out.0
}

#[test]
fn localized_string_tables_fall_back_to_the_base_for_empty_entries() {
    let base = string_table(2, &["Yes", "No", "Maybe"]);
    let german = string_table(2, &["Ja", "", "Vielleicht"]);
    let french = string_table(2, &["Oui", "Non"]);
    let provider = MapProvider::default().with_section(
        "strings",
        build_archive(&[
            ("table.xus", &base),
            ("de-de\\table.xus", &german),
            ("fr-fr\\table.xus", &french),
        ]),
    );
    let mut resources = Resources::new(&provider);
    let locator = Locator::parse(&units("section://0,strings#table.xus")).unwrap();

    let english = resources.string_table(&locator).expect("base table");
    resources.set_locale(Some("de-de"));
    let localized = resources.string_table(&locator).expect("localized table");
    resources.set_locale(Some("ja-jp"));
    let japanese_missing = resources.string_table(&locator).expect("base strings stay");
    resources.set_locale(Some("fr-fr"));
    let french_short = resources.string_table(&locator);

    let get =
        |table: &StringTable, index| table.get(StringIndex(index)).map(String::from_utf16_lossy);
    assert_eq!(
        [get(&english, 0), get(&english, 2), get(&english, 3)],
        [Some("Yes".into()), Some("Maybe".into()), None]
    );
    assert_eq!(
        [get(&localized, 0), get(&localized, 1), get(&localized, 2)],
        [
            Some("Ja".into()),
            Some("No".into()),
            Some("Vielleicht".into())
        ]
    );
    assert_eq!(get(&japanese_missing, 1), Some("No".into()));
    assert!(matches!(
        french_short,
        Err(StringTableError::FallbackCount {
            localized: 2,
            base: 3
        })
    ));
}

#[test]
fn string_table_entry_forms() {
    let mut by_id = Bytes::default();
    by_id
        .raw(b"XUIS")
        .u8(1)
        .u8(1)
        .u32(0)
        .u16(1)
        .u16(2)
        .wide("Hi")
        .u16(0x010a)
        .u16(3);
    let mut by_name = Bytes::default();
    by_name
        .raw(b"XUIS")
        .u8(1)
        .u8(0)
        .u32(0)
        .u16(1)
        .u16(1)
        .wide("A")
        .u16(3)
        .wide("key");

    let id_table = StringTable::parse(&by_id.0).expect("id table");
    let name_table = StringTable::parse(&by_name.0).expect("name table");

    assert_eq!(id_table.keys(), EntryKeys::Id);
    assert_eq!(id_table.by_id(0x010a_0003), Some(units("Hi").as_slice()));
    assert_eq!(
        name_table.by_name(&units("key")),
        Some(units("A").as_slice())
    );
    assert!(matches!(
        StringTable::parse(b"XUIS\x02\x02\0\0\0\0\0\0"),
        Err(StringTableError::Format { format: 2 })
    ));
    assert!(matches!(
        StringTable::parse(&string_table(2, &["abc"])[..15]),
        Err(StringTableError::Truncated { entry: 0, .. })
    ));
    assert!(matches!(
        StringTable::parse(b"XUIX"),
        Err(StringTableError::Magic)
    ));
}

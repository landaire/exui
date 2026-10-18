use std::vec::Vec;

use super::builder::ArchiveEntry;
use super::builder::Bytes;
use super::builder::build_archive;
use crate::archive::Archive;
use crate::archive::Entry;
use crate::error::ArchiveError;

fn sample() -> Vec<u8> {
    build_archive(
        false,
        &[
            ArchiveEntry {
                name: "Scenes\\Main.XUR",
                data: b"first",
                trailer: None,
            },
            ArchiveEntry {
                name: "scenes/main.xur",
                data: b"shadowed",
                trailer: None,
            },
            ArchiveEntry {
                name: "Img_\u{C4}.png",
                data: b"",
                trailer: None,
            },
            ArchiveEntry {
                name: "nul\u{0}tail",
                data: b"c string",
                trailer: None,
            },
        ],
    )
}

#[test]
fn entries_snapshot() {
    let bytes = sample();
    let archive = Archive::parse(&bytes).expect("archive parses");
    let entries: Vec<Entry<'_>> = archive.entries().collect();

    insta::assert_debug_snapshot!((archive.flags(), archive.entry_count(), entries));
}

#[test]
fn lookup_normalizes_and_first_match_wins() {
    let bytes = sample();
    let archive = Archive::parse(&bytes).expect("archive parses");
    let wide: Vec<u16> = "SCENES\\MAIN.XUR".encode_utf16().collect();

    insta::assert_debug_snapshot!([
        archive.open("SCENES/MAIN.xur"),
        archive.open("scenes\\main.xur"),
        archive.open_utf16(&wide),
        // Only ASCII letters fold.
        archive.open("img_\u{E4}.png"),
        archive.open("img_\u{C4}.PNG"),
        // Names and paths end at the first NUL.
        archive.open("NUL"),
        archive.open("nul\u{0}other"),
        archive.open("missing.xur"),
        archive.open(""),
        archive.open("\u{0}x"),
    ]);
}

#[test]
fn entry_trailers() {
    let bytes = build_archive(
        true,
        &[
            ArchiveEntry {
                name: "a",
                data: b"one",
                trailer: Some([0x1111_1111, 0x2222_2222]),
            },
            ArchiveEntry {
                name: "b",
                data: b"two!",
                trailer: Some([3, 4]),
            },
        ],
    );
    let archive = Archive::parse(&bytes).expect("archive parses");
    let entries: Vec<Entry<'_>> = archive.entries().collect();

    insta::assert_debug_snapshot!((
        archive.flags(),
        entries,
        archive.open("a"),
        archive.open("b"),
    ));
}

#[test]
fn header_and_table_errors() {
    let valid = sample();

    let mut bad_magic = valid.clone();
    bad_magic[3] = b'B';

    let mut bad_version = valid.clone();
    bad_version[7] = 2;

    let mut bad_size = valid.clone();
    bad_size.push(0);

    let truncated = valid[..0x10].to_vec();

    // One entry whose name runs past a 12-byte table.
    let mut overrun = Bytes::default();
    overrun
        .raw(b"XUIZ")
        .u32(1)
        .u32(0x16 + 12)
        .u32(0)
        .u32(12)
        .u16(1)
        .u32(0)
        .u32(0)
        .u8(2)
        .u16(0x61)
        .raw(&[0]);

    // Entry header needs nine bytes.
    let mut short_entry = Bytes::default();
    short_entry
        .raw(b"XUIZ")
        .u32(1)
        .u32(0x16 + 8)
        .u32(0)
        .u32(8)
        .u16(1)
        .u32(0)
        .u32(0);

    // The flag asks for a trailer that the data does not hold.
    let missing_trailer = build_archive(
        true,
        &[ArchiveEntry {
            name: "a",
            data: b"one",
            trailer: None,
        }],
    );

    // 0xFFFF entries declared over an empty table.
    let mut hostile_count = Bytes::default();
    hostile_count
        .raw(b"XUIZ")
        .u32(1)
        .u32(0x16)
        .u32(0)
        .u32(0)
        .u16(0xFFFF);

    let errors: Vec<Option<ArchiveError>> = [
        &bad_magic,
        &bad_version,
        &bad_size,
        &truncated,
        &overrun.0,
        &short_entry.0,
        &missing_trailer,
        &hostile_count.0,
    ]
    .into_iter()
    .map(|bytes| Archive::parse(bytes).err())
    .collect();

    insta::assert_debug_snapshot!(errors);
}

#[test]
fn entry_data_out_of_bounds() {
    let mut bytes = Bytes::default();
    bytes
        .raw(b"XUIZ")
        .u32(1)
        .u32(0x16 + 2 * 9 + 2)
        .u32(0)
        .u32(2 * 9)
        .u16(2)
        .u32(4)
        .u32(0)
        .u8(0)
        .u32(1)
        .u32(0xFFFF_FFFF)
        .u8(0)
        .raw(&[1, 2]);
    let archive = Archive::parse(&bytes.0).expect("archive parses");
    let results: Vec<_> = archive
        .entries()
        .map(|entry| archive.entry_data(&entry))
        .collect();

    insta::assert_debug_snapshot!(results);
}

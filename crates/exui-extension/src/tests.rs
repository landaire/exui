use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::alloc::GlobalAlloc;
use core::alloc::Layout;
use core::cell::Cell;
use core::fmt::Write;
use core::num::NonZeroU8;
use std::alloc::System;

use crate::BladeTab;
use crate::DecodeError;
use crate::Descriptor;
use crate::ElementPath;
use crate::EntryTarget;
use crate::ExtensionName;
use crate::PoseHold;
use crate::ScenePatch;
use crate::StringEntry;
use crate::StringKey;
use crate::StringOverlay;

/// Records the largest allocation each thread requests, so a test can
/// bound what a parse reserves.
struct LargestAllocation;

std::thread_local! {
    static LARGEST: Cell<usize> = const { Cell::new(0) };
}

// SAFETY: every call forwards to `System` with the caller's arguments.
unsafe impl GlobalAlloc for LargestAllocation {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record(new_size);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: LargestAllocation = LargestAllocation;

fn record(size: usize) {
    // The slot is gone while the thread exits; those allocations are not
    // part of a test.
    let _ = LARGEST.try_with(|largest| largest.set(largest.get().max(size)));
}

fn sample() -> Descriptor {
    Descriptor {
        name: ExtensionName::new("sample").expect("valid name"),
        patches: vec![ScenePatch {
            target: EntryTarget {
                section: String::from("scenes"),
                path: String::from("main.xur"),
            },
            remove: vec![ElementPath::parse("Root/Old").expect("valid path")],
            overlay: Some(String::from("patches/main.xur")),
            holds: vec![PoseHold {
                element: ElementPath::parse("Root").expect("valid path"),
                source: String::from("Rest"),
                frames: vec![String::from("Extra"), String::from("ExtraEnd")],
            }],
        }],
        strings: vec![StringOverlay {
            target: EntryTarget {
                section: String::from("common"),
                path: String::from("strings.xus"),
            },
            locale: Some(String::from("de-de")),
            entries: vec![
                StringEntry {
                    key: StringKey::Index(3),
                    value: String::from("Drei"),
                },
                StringEntry {
                    key: StringKey::Id(0x10),
                    value: String::from("Zehn"),
                },
                StringEntry {
                    key: StringKey::Name(String::from("title")),
                    value: String::from("Titel"),
                },
            ],
        }],
        blade: NonZeroU8::new(7).map(BladeTab),
    }
}

#[test]
fn descriptor_round_trips() {
    let descriptor = sample();
    let bytes = descriptor.to_bytes().expect("descriptor encodes");

    assert_eq!(Descriptor::parse(&bytes), Ok(descriptor));

    let mut hex = String::new();
    for line in bytes.chunks(16) {
        for byte in line {
            write!(hex, "{byte:02x} ").unwrap();
        }
        hex.push('\n');
    }
    insta::assert_snapshot!(hex);
}

#[test]
fn malformed_descriptors_are_rejected() {
    let bytes = sample().to_bytes().expect("descriptor encodes");
    let mut trailing = bytes.clone();
    trailing.push(0);
    let mut bad_name = bytes.clone();
    bad_name[8] = b'S';
    let mut empty_id = sample();
    empty_id.patches[0].remove = vec![ElementPath(vec![String::new()])];
    let empty_id = empty_id.to_bytes().expect("descriptor encodes");

    let results: Vec<Result<Descriptor, DecodeError>> = vec![
        Descriptor::parse(b"OBXX\0\x01"),
        Descriptor::parse(b"OBXT\0\x02"),
        Descriptor::parse(&bytes[..bytes.len() - 1]),
        Descriptor::parse(&trailing),
        Descriptor::parse(&bad_name),
        Descriptor::parse(&empty_id),
    ];
    let errors: Vec<DecodeError> = results
        .into_iter()
        .map(|result| result.expect_err("rejected"))
        .collect();
    insta::assert_debug_snapshot!(errors);
}

#[test]
fn names_and_paths() {
    assert!(ExtensionName::new("homebrew").is_some());
    assert!(ExtensionName::new("toolong12").is_none());
    assert!(ExtensionName::new("Upper").is_none());
    assert!(ExtensionName::new("").is_none());
    assert_eq!(ElementPath::parse(""), Some(ElementPath::default()));
    assert_eq!(ElementPath::parse("a//b"), None);
    assert_eq!(
        ElementPath::parse("RootScene/Tab5")
            .map(|path| alloc::format!("{path}"))
            .as_deref(),
        Some("RootScene/Tab5")
    );
}

#[test]
fn parse_does_not_reserve_declared_counts() {
    // 65535 patches; the first targets "s" "p" and has 65535 removed
    // paths, the first of 65535 ids. The descriptor ends there.
    let mut bytes = Vec::from(*b"OBXT\0\x01\0\x06sample");
    bytes.extend_from_slice(b"\xFF\xFF\0\x01s\0\x01p\xFF\xFF\xFF\xFF");

    LARGEST.with(|largest| largest.set(0));
    let result = Descriptor::parse(&bytes);
    let largest = LARGEST.with(Cell::get);

    assert_eq!(
        result,
        Err(DecodeError::Malformed {
            offset: bytes.len()
        })
    );
    // One reservation of a declared count would exceed this.
    assert!(
        largest < usize::from(u16::MAX),
        "largest allocation {largest}"
    );
}

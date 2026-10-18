# XUR binary format, version 8

Version 5 XUR files and version 1 XUIZ archives are described in [xui-format.md](xui-format.md), and the classes and property tables in [xui-classes.md](xui-classes.md). Both were verified against the loader of dash 6717.

This file covers XUR version 8 and XUIZ version 3, which later dashboards use. These notes come from the `xam.xex` loader (`sub_817e0c10`, `sub_817e0970`, `sub_817dfc00`, `sub_817df530`) and from the files in `test-data/hud`. They have not been checked against a loader as completely as version 5. Where a rule is only observed, it says so.

All multi-byte integers are big-endian.

## XUIZ version 3

Same layout as version 1, except each entry name is stored as `name length` single bytes instead of UTF-16 units. The observed archive has flags 0, so the version 1 trailer flag is not known to apply.

## Packed integers

Most version 8 integers are packed:

```
byte < 0xF0:          value = byte                               (1 byte)
0xF0 <= byte < 0xFF:  value = ((byte & 0x0F) << 8) | next_byte   (2 bytes)
byte == 0xFF:         value = next 4 bytes as u32                (5 bytes)
```

## Header

| Offset | Type | Meaning |
| --- | --- | --- |
| 0x00 | u32 | Magic `XUIB`. |
| 0x04 | u32 | Version, 8. |
| 0x08 | u32 | Flags. 0 in every observed file. |
| 0x0C | u16 | XUI schema version, 0xE in every observed file. |
| 0x0E | u32 | File size. |
| 0x12 | u16 | Section count. |
| 0x14 | packed[12] | Twelve packed integers of unknown meaning (observed). |

The section table follows: per section a 4-byte tag, a u32 offset from file start and a u32 size.

## Sections

| Tag | Content |
| --- | --- |
| `STRN` | u32 byte length, u16 count, then that many NUL-terminated 8-bit strings. References are one-based; 0 is the null string. |
| `VECT` | f32 triples. |
| `QUAT` | f32 quadruples. |
| `CUST` | Custom blobs, each a u32 length and that many bytes. |
| `FLOT` | f32 values. |
| `COLR` | u32 ARGB values. |
| `NAME` | Named frames: packed name, packed time, u8 command, and a packed target name when the command is 2 or more. |
| `KEYD` | Keyframes: packed time, a flag byte, interpolation data, and a packed start index into `KEYP`. |
| `KEYP` | Packed keyframe value references. |
| `DATA` | The object tree. |

## Objects

| Type | Meaning |
| --- | --- |
| packed | Class name string index. |
| u8 | Flags. Bit 0 properties, bit 1 children, bit 2 timelines, bit 3 id. |

Properties (bit 0): a packed count of set mask bits over all class levels, then one block per class level, root first. A block is a packed mask followed by one value per set bit. An empty block is a single 0 byte.

Id (bit 3): one packed integer, use unknown.

Children (bit 1): a packed count, then the child objects.

Timelines (bit 2) follow the children.

### Values

| Type | Encoding |
| --- | --- |
| Bool | u8 |
| Integer, Unsigned | packed value |
| Float | packed `FLOT` index |
| String | packed `STRN` index |
| Color | packed `COLR` index |
| Vector | packed `VECT` index |
| Quaternion | packed `QUAT` index |
| Custom | packed `CUST` offset |
| Compound | packed slot. On the first use of a slot, a packed value count and a block over the sub-definitions follow. A later use of the slot has no body and refers to the first value. |

Indexed properties read a packed element count before the values.

## Timelines

- Packed named frame count, and when it is not 0, a packed start index into `NAME`.
- If the object has no children, nothing more is read.
- Packed timeline count. Each timeline: packed element name, packed property count, the property paths, packed keyframe count and packed start index into `KEYD`.

A property path is a byte with the depth in bits 0-6 and an element index flag in bit 7. A non-zero depth is followed by the levels up, the property index and `depth - 1` sub-indices, one byte each. With bit 7 set, a packed element index follows. The levels up and indices have the version 5 meaning.

`KEYD` flag bits 0-5 select the interpolation: 2 is followed by three ease bytes (in, out, scale), 0xA by a packed integer and 0xB by one byte; 1 is none and the other modes are linear. Keyframe `k` of a timeline holds one `KEYP` entry per property path, from its start index on. Each entry is resolved against the type of the path's final definition: the value itself for Bool, Integer and Unsigned, otherwise an index into the matching table.

## Class tables

No class registry of a version 8 host is known. exui types version 8 values with the version 5 class definitions and reads one block per class level of a known class. A class those lack keeps `XuiElement` for its root level, and its other levels are read as untyped packed integers, which is exact for every type except compounds and indexed properties. Its blocks are read until the set bit count is reached, and the 0 bytes that follow are taken as empty levels. When the field after them could also be 0 (an id, a child count, a named frame count), the level count is ambiguous and exui reports an error.

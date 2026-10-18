# XUI file formats

XUIZ archives and the XUR version 5 binary scene format, as read by the XUI library statically linked into dash 6717. All integers are big-endian.

## Components

| Address | Name | Role | Status |
| --- | --- | --- | --- |
| 0x921737a8 | `XuiZipArchiveOpen` | Reads the XUIZ header and entry table into a name map. | complete |
| 0x92172af8 | `XuiZipReadEntry` | Reads one entry record. | complete |
| 0x92172ca8 | `XuiZipNormalizePath` | ASCII-lowercases a path and turns `\` into `/`. | complete |
| 0x92173498 | `XuiZipOpenFile` | Looks a path up and opens a substream over the entry data. | complete |
| 0x9216bd98 | `XuiStringMapFind` | Linear search with `wcscmp`. | complete |
| 0x9216ffb8 | `XuiStringMapInsert` | Appends to the name map. | complete |
| 0x9216f180 | `XuiLoadBinaryScene` | Validates the XUR header, loads the tables, reads the root object. | complete |
| 0x9216d690 | `XuiBinaryStringTableInit` | Builds the STRN string table. | complete |
| 0x9216d7e8 | `XuiBinaryFloatTableInit` | Copies VECT (3 floats) and QUAT (4 floats). | complete |
| 0x9216d878 | `XuiBinaryFloatTableGet` | Bounds-checked table element copy. | complete |
| 0x9216eef0 | `XuiBinaryReadObject` | Reads one object and, recursively, its children and timelines. | complete |
| 0x9216ed88 | `XuiBinaryReadClassProps` | Reads one property block per class level, root first. | complete |
| 0x9216ea20 | `XuiBinaryReadPropBlock` | Reads a property block (mask and values). | complete |
| 0x9216d940 | `XuiBinaryMaskTestBit` | Tests a mask bit. | complete |
| 0x9216d998 | `XuiBinaryReadArrayCount` | Decodes the element count of an indexed property. | complete |
| 0x9216e460 | `XuiBinaryReadTimelines` | Reads named frames and timelines of an object. | complete |
| 0x9216e020 | `XuiBinaryReadTimelinePropPath` | Reads and resolves a timeline property path. | complete |
| 0x9216e288 | `XuiBinaryReadKeyframe` | Reads one keyframe. | complete |
| 0x9216df10 | `XuiBinaryReadKeyframeValue` | Reads one keyframe value. | complete |
| 0x921410e0 | `XuiValueDeserialize` | Generic typed value reader used for keyframes. | complete |
| 0x9216d9e8 | `XuiClassResolvePropDef` | Maps (levels up, index) to a property definition. | complete |
| 0x921720e8 | `XuiPropEntryGetString` | String of a loaded property entry. | complete |
| 0x9213d8c8 | `XuiSetResolver` | Sets `g_XuiResolver` (0x92809cd4). | complete |
| 0x9214f7f8 | `XuiApplyLoadedProperty` | Applies loaded entries to the element; passes custom blobs to their handler. | partial |
| 0x92164a68 | `XuiFigurePoints_LoadBinary` | Decodes the `XuiFigure.Points` custom blob. | complete |

## XUIZ archive

Header, 0x16 bytes. The reader loads all of it before checking any field; every header failure returns 0x8030001C.

| Offset | Type | Meaning |
| --- | --- | --- |
| 0x00 | u32 | Magic `XUIZ`. |
| 0x04 | u32 | Version, must be 1. |
| 0x08 | u32 | File size, must equal the stream size. |
| 0x0C | u32 | Flags. Bit 0: each entry has an 8-byte trailer. |
| 0x10 | u32 | Entry table size in bytes. |
| 0x14 | u16 | Entry count. |

The entry table follows at 0x16. Entry data starts at `0x16 + table size`. Each entry:

| Type | Meaning |
| --- | --- |
| u32 | Data size. |
| u32 | Data offset from the end of the entry table. |
| u8 | Name length in UTF-16 units. |
| u16[length] | Name, UTF-16BE. |

- An entry needs 9 table bytes for its fixed part, then `2 * length` more; otherwise 0x8030001C.
- Bytes left in the table after the last entry are ignored.
- With flag bit 0, the reader seeks to `data base + offset + size` and reads two u32 words into the entry record (+0x8, +0xC). Their consumer is unknown. None of the 6717 archives set the flag.
- Stored names are normalized: `_wcslwr_s` in the C locale (only A-Z fold) and `\` replaced by `/`.

Lookup (`XuiZipOpenFile`): an empty path fails with 0x80300012. The path is normalized the same way and compared with `wcscmp` against each entry in table order; the first match wins. No match fails with 0x80300013. The entry data range is not checked at open time.

## XUR header

| Offset | Type | Meaning |
| --- | --- | --- |
| 0x00 | u32 | Magic `XUIB`. |
| 0x04 | u32 | Version, must be 5. |
| 0x08 | u32 | Flags. Bit 0: ten u32 allocation hints follow the header. |
| 0x0C | u16 | XUI schema version, must be 0xC. |
| 0x0E | u32 | File size, must equal the stream size. |
| 0x12 | u16 | Section count, must be at least 2. |

Magic, version and schema failures return 0x8030000D. With flag bit 0, ten u32 hints h0..h9 (0x28 bytes) follow and the loader sizes its allocator as `4 * (2 * (2 * (2*h7 + h8 + h6 + h5 + h1) + h9 + h2) + 5*h3 + 11*h0 + h4)` bytes; without them it uses 0x4000 bytes. The hints have no other use.

The section table follows: per section a 4-byte tag, u32 offset from file start, u32 size. Tags STRN, VECT, QUAT, CUST and DATA are recognized; a later entry with the same tag replaces an earlier one and other tags are ignored. STRN and DATA are required. Offsets are not bounds-checked by the original.

### STRN

Consecutive strings, each a u16 length in UTF-16 units followed by the units. String references are one-based; 0 is the null string. The counting pass masks the length with 0x7FFF but the copy pass uses all 16 bits, so a set bit 15 corrupts the table. Strings are terminated in place by overwriting the next length word.

### VECT, QUAT

Packed f32 triples and quadruples. The size must be a multiple of 12 (16) bytes, otherwise 0x8000FFFF. References are zero-based.

### CUST

Raw bytes, copied. A custom property value is an offset into this section where a blob starts: u32 length followed by that many bytes. `XuiApplyLoadedProperty` passes `blob + 4` and the length to the custom type's binary load handler (vtable +0x14). Offsets are not checked.

## Objects (DATA)

The DATA section holds one root object.

| Type | Meaning |
| --- | --- |
| u16 | Class name string index. 0 or past the table fails with 0x8030000E. |
| u8 | Flags. Bit 0 properties, bit 1 children, bit 2 timelines. |

Class names are resolved with `XuiFindClass` (case-sensitive). If no class matches, the loader calls method 1 (vtable +4) of the resolver interface `g_XuiResolver` (0x92809cd4), set from `XUIInitParams` +8 by `XuiInit` or later by `XuiSetResolver` (0x9213d8c8). Without a resolver or on its failure the whole load fails with 0x80300004. The dashboard calls `XuiInit` with default parameters (`{0xC, 0, 0}` built at 0x9210d0a0) and then installs the object at 0x927975c4 (vtable 0x92001d34). Its method 0 resolves `common://` and `sharedres://` locators; its method 1 (0x9272b668) returns E_NOTIMPL, so in the dashboard an unknown class always fails the load.

Properties (bit 0): a u16 with the number of value entries, then one property block per class level from the root base class down to the object's class. Every level reads a block header, including levels that declare no properties. The total entry count must equal the u16, otherwise 0x8030000D.

Children (bit 1): at least 4 bytes must remain (else 0x8030000D), then a u32 count and that many objects.

Timelines (bit 2) follow the children.

### Property block

The first byte's low three bits give the mask size n; the upper five bits are ignored (writers fill them, meaning unknown). n = 1, 2 or 4 reads a u8, u16 or u32 mask. Other values skip n bytes and act as an empty mask. n = 0 ends the block immediately.

Definitions are visited in table order. Bit i of the mask selects definition i; definitions at position 32 or later can never be selected. A selected definition with flag bit 0 (indexed) reads an element count:

- byte b < 0x80: count b.
- otherwise k = b & 0x7F; k = 1, 2, 4 reads a u8, u16, u32 count; other k skips k bytes and gives count 0.

A count of 0 records nothing. Every element is one value entry. Value encodings:

| Type | Code | Encoding |
| --- | --- | --- |
| Bool | 1 | u8 |
| Integer | 2 | u32 |
| Unsigned | 3 | u32 |
| Float | 4 | f32 |
| String | 5 | u16 string index, checked only when used |
| Color | 6 | u32 ARGB |
| Vector | 7 | u32 VECT index, checked only when used |
| Quaternion | 8 | u32 QUAT index, checked only when used |
| Compound | 9 | u16 entry count, then a property block over the compound's sub-definitions |
| Custom | 10 | u32 CUST offset |

The compound u16 sizes the sub-entry array; the loader does not compare it with the entries read, so a smaller value overflows the allocation. A compound counts as one entry of its parent block.

## Timelines

Read after the object's children.

- If fewer than 4 bytes remain in DATA, timeline data is skipped and the load succeeds.
- u32 named frame count. Each named frame needs 9 bytes (else 0x8030000D): u16 name, u32 frame, u8 command, u16 target name. String indices must be 0 or within the table. The command byte is stored unchanged; its values are decoded by the timeline runtime.
- If the object has no children, nothing more is read.
- u32 timeline count. Each timeline: u16 element name (checked), u32 property count, the property paths, u32 keyframe count, the keyframes.

### Property path

| Type | Meaning |
| --- | --- |
| u8 | Bits 0-6: depth. Bit 7: an element index follows. |
| u8 | Levels up (only when depth > 0). |
| u8 | Property index within that class level (only when depth > 0). |
| u8[depth - 1] | Sub-definition indices through compounds. |
| u32 | Element index (only with bit 7). |

For every path the loader searches the object's direct children, in order, for one with any loaded property named `Id` (an `XuiEffect` child has two, both are compared) whose string equals the timeline's element name (`wcscmp`); not found gives 0x8030000D. A timeline with zero properties never searches. The first definition is found by walking `levels up` base classes from the child's class (`XuiClassResolvePropDef`); a missing level or index gives 0x80300007. Sub indices must be below the compound's definition count (0x8030000D).

### Keyframe

| Type | Meaning |
| --- | --- |
| u32 | Frame. |
| u8 | Interpolation, stored shifted left by one into a flag byte. |
| u8 | Ease in. |
| u8 | Ease out. |
| u8 | Ease scale. |

Then one value per property path whose final definition is not Compound or Custom (those have none): String u16 (index checked), Vector u32 and Quaternion u32 (table index checked, value copied), Bool u8 (nonzero is true), Integer, Unsigned, Float and Color as 4 bytes. A path of depth 0 makes the loader read its type from before the definition array.

## Custom type: `XuiFigure.Points`

Handler table at 0x9279ac38 (to string, from string, save binary, load binary). Binary form: f32 width, f32 height, u32 count, then count records of six f32. The load handler applies width and height if at least 8 bytes are present and the points only if `count * 24` bytes follow the count; otherwise the points are ignored. In memory each point is 0x1C bytes: the six floats and an int that the binary form leaves 0 (the text form writes it).

## Reimplementation

`exui-core`:

- `archive::Archive` borrows the XUIZ bytes. `parse` validates every entry record without allocating, `entries()` decodes them on demand, and `open`/`find` apply the original normalization and first-match lookup.
- `xur::Document` borrows the XUR bytes. Strings, vectors, quaternions and custom blobs are resolved from the input on access. The object tree is decoded into flat arenas (objects, properties, values, timelines, keyframes) whose sizes are bounded by the input length; this is the one allocating part of the reader. `ObjectRef` gives children, parent, properties (looked up by definition address), named frames and timelines. Unchecked indices (property strings, vectors, quaternions, custom offsets) stay unchecked and resolve to `None` through the document accessors. Keyframe vectors and quaternions are copied at load, as the original does.
- Timeline value types are computed once per timeline and child `Id` strings once per parent (stably sorted), so parse time is linear in the input.
- `custom::FigurePoints` decodes the figure point blob.

Deliberate differences, all for inputs the original mishandles: archive entry ranges are checked when an entry is opened, section ranges are bounds-checked, every read is bounds-checked, a string length with bit 15 set is rejected, a compound holding more entries than its count is rejected, a timeline element that cannot match and a depth-0 path with keyframes are errors, and a non-compound definition inside a property path is an error (the original calls a null sub-definition getter).

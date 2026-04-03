# XUR Binary Format (Version 5)

Reverse-engineered from `xam_1888.exe` (Xbox 360 blades-era system library,
stripped PPC binary) and `xamd.dll` (v8 debug build with full MSVC symbols).

All multi-byte integers are **big-endian** (Xbox 360 / PowerPC).

---

## File Layout

```
+0x00  FileHeader (20 bytes)
+0x14  [Extra header: 40 bytes if reserved=1]
+vary  SectionHeader[section_count] (12 bytes each)
+vary  Section data (STRN, VECT, QUAT, CUST, DATA)
```

### FileHeader (20 bytes)

| Offset | Size | Type      | Description                                          |
| ------ | ---- | --------- | ---------------------------------------------------- |
| 0x00   | 4    | `[u8; 4]` | Magic: `XUIB` (0x58554942)                           |
| 0x04   | 4    | `u32`     | Binary format version (5 for blades-era, 8 for NXE+) |
| 0x08   | 4    | `u32`     | Reserved (0 or 1; 1 adds 40-byte extra header)       |
| 0x0C   | 2    | `u16`     | XUI schema version (e.g. 0x000c = 12)                |
| 0x0E   | 4    | `u32`     | Total file size in bytes (unaligned!)                 |
| 0x12   | 2    | `u16`     | Number of sections                                   |

When `reserved=1`, 10 u32 pre-computed values (40 bytes) follow the header
before the section table. These are memory size hints used by the runtime.

### SectionHeader (12 bytes each)

| Offset | Size | Type      | Description                                 |
| ------ | ---- | --------- | ------------------------------------------- |
| 0x00   | 4    | `[u8; 4]` | Section tag (ASCII FourCC)                  |
| 0x04   | 4    | `u32`     | Byte offset from file start to section data |
| 0x08   | 4    | `u32`     | Section data size in bytes                  |

### Known Section Tags

| Tag    | Description                     | v5  | v8  |
| ------ | ------------------------------- | --- | --- |
| `STRN` | String table                    | Yes | Yes |
| `VECT` | Vector3 table (3 x f32)         | Yes | Yes |
| `QUAT` | Quaternion table (4 x f32)      | Yes | Yes |
| `CUST` | Custom data blobs               | Yes | Yes |
| `DATA` | Element tree + property data    | Yes | Yes |
| `FLOT` | Float lookup table              | No  | Yes |
| `COLR` | Color lookup table              | No  | Yes |

---

## STRN Section

Consecutive length-prefixed UTF-16BE strings, no section-level header.

Each string: `u16(char_count) + u16[char_count]`.

String references are **1-based**: index 1 = first string, index 0 = null.

## VECT Section

Raw f32 BE triples (x, y, z). 12 bytes per entry. **0-based** indexing.

## QUAT Section

Raw f32 BE quads (x, y, z, w). 16 bytes per entry. **0-based** indexing.

## CUST Section

Raw byte blob. Referenced by byte offset from custom-type property values.

---

## DATA Section: Object Tree

### Object Encoding

```
u16    class_name    // 1-based STRN index
u8     flags         // bit 0=properties, bit 1=children, bit 2=timelines
```

#### Properties (if flags & 0x01)

```
u16    total_value_count    // metadata (total values across all class levels)
```

Then `LoadObjectPropsFromBinary` walks the class hierarchy via recursion:
1. Recurse for base class (until XuiElement, which has no base)
2. Call `LoadPropertiesFromBinary` for this class's own property definitions

Each class level reads one bitmask+values block via `LoadPropertiesFromBinary`.

#### Children (if flags & 0x02)

```
u32    child_count
Object[child_count]    // recursive
```

#### Timelines (if flags & 0x04)

See Timeline section below.

---

## Bitmask Encoding (v5)

**Confirmed from disassembly of `sub_100cad08` in xam_1888.exe.**

The first byte encodes the bitmask size in its low 3 bits:

```
first_byte = read_u8()
n = first_byte & 0x07    // number of ADDITIONAL data bytes
```

| n | Total bytes | Data read           | Instruction |
|---|-------------|---------------------|-------------|
| 0 | 1           | empty (bitmask = 0) | (none)      |
| 1 | 2           | u8 at offset+1      | `lbz`       |
| 2 | 3           | u16 BE at offset+1  | `lhz`       |
| 4 | 5           | u32 BE at offset+1  | `lwz`       |

Values n=3, 5, 6, 7 are invalid (runtime returns 0).

The high 5 bits of the first byte are metadata (not part of bitmask value).

### Difference from v8

In v8, bitmasks use packed_ulong encoding (see below). The v5 length-prefixed
format is unique to version 5.

---

## Packed Ulong Encoding (v8 only)

Used in v8 for bitmasks and various fields. **NOT used for v5 bitmasks.**

```
if byte < 0xF0:  value = byte                              (1 byte)
if byte < 0xFF:  value = ((byte & 0x0F) << 8) | next_byte  (2 bytes)
if byte == 0xFF: value = next_4_bytes_as_u32_be             (5 bytes)
```

---

## Compound Property Encoding (v5)

**Confirmed from `sub_100cbcb0` type-9 branch in xam_1888.exe.**

No compound index in the v5 stream. The runtime resolves sub-definitions
via a function pointer in the property definition struct.

```
u16    value_count    // metadata (total sub-property values)
// Then recursive LoadPropertiesFromBinary with compound's sub-definitions:
bitmask + values      // same format as a class level
```

### Difference from v8

In v8, compounds encode `packed_ulong(compound_idx)` + `packed_ulong(value_count)`
then recursively call `LoadPropertiesFromBinary`.

---

## Array Property Encoding (v5)

**From `sub_100cbcb0` array branch.**

Properties flagged as arrays read a count prefix before the values:

```
first_byte = read_u8()
if first_byte < 0x80:
    count = first_byte        // single byte, value IS the count
else:
    extra = first_byte & 0x7f
    count = read_be_uint(extra bytes)
```

Then `count` typed values follow.

Known array properties: Gradient StopColor (bit 2), Gradient StopPos (bit 3).

---

## Property Value Types (v5)

| Type ID | Name       | v5 Encoding                          |
| ------- | ---------- | ------------------------------------ |
| 1       | Bool       | u8 (0x00=false, 0x01=true)           |
| 2       | Integer    | u32 BE                               |
| 3       | Unsigned   | u32 BE                               |
| 4       | Float      | f32 BE                               |
| 5       | String     | u16 BE (1-based STRN index)          |
| 6       | Color      | u32 BE (ARGB)                        |
| 7       | Vector3    | u32 BE (0-based VECT index)          |
| 8       | Quaternion | u32 BE (0-based QUAT index)          |
| 9       | Compound   | u16(count) + recursive bitmask+values|
| 10      | Custom     | u32 BE (byte offset into CUST)       |

### Difference from v8

In v8, Integer/Unsigned/String/Vector3/Quaternion/Custom use packed_ulong
encoding, and Float/Color/Vector3/Quaternion reference FLOT/COLR/QUAT tables.

---

## Timeline Data (v5, partial)

**From `sub_100cb730` in xam_1888.exe.**

```
u32    named_timeline_count
// For each: u16(name) + u32(duration) + u8(type) + u16(from_name) = 9 bytes

u32    named_frame_count
// For each named frame:
    u16    frame_name        // STRN ref
    u32    keyframe_count
    // Keyframe paths (sub_100cb2d0):
    //   u8(depth_and_flag), depth=byte&0x7f, has_value=byte&0x80
    //   if depth>0: u8(hierarchy_level) + u8(property_index) + (depth-1)*u8(sub_index)
    //   if has_value: u32
    u32    subtimeline_count
    // Subtimelines (sub_100cb558):
    //   8 bytes fixed: u32(time) + u8(flags) + u8 + u8 + u8
    //   Then per-property interpolation values (type-dependent, variable size)
```

---

## Class Hierarchy

**From `CXuiClassBase<T>::Register()` in xamd.dll (v8). Same hierarchy in v5
except XuiHtmlElement (v5-only, extends XuiElement).**

```
XuiElement (root)
+-- XuiCanvas
+-- XuiFigure
+-- XuiText
+-- XuiImage
+-- XuiNineGrid
+-- XuiGroup
|   +-- XuiTextureSurface
+-- XuiVisual
+-- XuiSound
|   +-- XuiSoundXAudio
+-- XuiControl
|   +-- XuiButton
|   |   +-- XuiNavButton
|   |   +-- XuiBackButton
|   +-- XuiCheckbox
|   |   +-- XuiListItem
|   +-- XuiScene
|   |   +-- XuiTabScene
|   |   +-- XuiMessageBox
|   |   +-- XuiPerspectiveScene
|   +-- XuiLabel
|   +-- XuiRadioButton / XuiRadioGroup
|   +-- XuiScrollBar / XuiScrollEnd
|   +-- XuiList
|   |   +-- XuiCommonList
|   +-- XuiProgressBar / XuiSlider
|   +-- XuiEdit / XuiCaret
+-- XuiHtmlElement (v5 only)
+-- XuiTransition / XuiImagePresenter / XuiTextPresenter
+-- XuiGridPanel / XuiShader / XuiVariable
```

---

## v5 Property Tables

Property indices are version-specific. Known v5/v8 differences:

- **XuiElement**: v5 has Show=7/Anchor=9; v8 has Anchor=7/Show=9.
  v8 has 27 properties; v5 has ~14.
- **XuiControl**: v5 indices 14-15 = SizeToText/UseNuiAsMouse;
  v8 has ClipChildren/EnableEffects.
- **XuiHtmlElement**: v5 only (1 own property: text content as string).

### XuiElement (v5, 14 properties)

| Bit | Name                      | Type       |
|-----|---------------------------|------------|
| 0   | Id                        | String     |
| 1   | Width                     | Float      |
| 2   | Height                    | Float      |
| 3   | Position                  | Vector3    |
| 4   | Scale                     | Vector3    |
| 5   | Rotation                  | Quaternion |
| 6   | Opacity                   | Float      |
| 7   | Show                      | Unsigned   |
| 8   | Pivot                     | Vector3    |
| 9   | Anchor                    | Bool       |
| 10  | BlendMode                 | Unsigned   |
| 11  | DisableTimelineRecursion  | Bool       |
| 12  | ColorWriteFlags           | Bool       |
| 13  | ColorFactor               | Color      |

### XuiFigure (4 properties, stable across versions)

| Bit | Name   | Type     | Sub-types       |
|-----|--------|----------|-----------------|
| 0   | Stroke | Compound | StrokeProperty  |
| 1   | Fill   | Compound | FillProperty    |
| 2   | Closed | Bool     |                 |
| 3   | Points | Custom   |                 |

### StrokeProperty (stable)

| Bit | Name        | Type  |
|-----|-------------|-------|
| 0   | StrokeWidth | Float |
| 1   | StrokeColor | Color |

### FillProperty (11 properties, stable)

| Bit | Name             | Type     | Sub-types        |
|-----|------------------|----------|------------------|
| 0   | FillType         | Unsigned |                  |
| 1   | FillColor        | Color    |                  |
| 2   | TextureFileName  | String   |                  |
| 3   | Gradient         | Compound | GradientProperty |
| 4   | Translation      | Vector3  |                  |
| 5   | Scale            | Vector3  |                  |
| 6   | Angle            | Float    |                  |
| 7   | WrapX            | Unsigned |                  |
| 8   | WrapY            | Unsigned |                  |
| 9   | BrushFlags       | Unsigned |                  |
| 10  | TransformVersion | Unsigned |                  |

### GradientProperty (stable)

| Bit | Name      | Type     | Array? |
|-----|-----------|----------|--------|
| 0   | Radial    | Bool     | No     |
| 1   | NumStops  | Unsigned | No     |
| 2   | StopColor | Color    | Yes    |
| 3   | StopPos   | Float    | Yes    |

---

## References

- `xam_1888.exe` -- Blades-era Xbox 360 XAM library (stripped PPC binary)
  - `sub_100cc4b4` -- XUIB loader entry (validates magic, parses sections)
  - `sub_100cc180` -- LoadObjectFromBinary
  - `sub_100cc018` -- LoadObjectPropsFromBinary (recursive hierarchy walk)
  - `sub_100cbcb0` -- LoadPropertiesFromBinary (bitmask + values)
  - `sub_100cad08` -- Bitmask bit test (lbz/lhz/lwz dispatch)
  - `sub_100cb730` -- Timeline data loader
  - `sub_100cb2d0` -- Keyframe path reader
  - `sub_100cb558` -- Subtimeline reader
- `xamd.dll` -- v8 Xbox 360 XAM debug build (full MSVC symbols)
  - Used for class hierarchy (`CXuiClassBase<T>::Register`)
  - Used for property tables (`_GetPropDef` functions)
  - v8 encoding differs significantly from v5 (packed_ulong bitmasks, table refs)

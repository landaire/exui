# XUI rendering (dash 2.0.6717)

Scope: element transform, layout (Anchor), render traversal, render state (opacity, blend, color write, clip), the XUI render device context (DC) and its D3D usage, root canvas mapping, and the drawing of XuiFigure, XuiImage, XuiNineGrid. Text is in `xui-text.md`. Class registry, property tables and XUR parsing are in XuiReader's notes (`xui-format.md`, `xui-classes.md`).

All addresses are virtual addresses in the dash image (file offset = address - 0x92000000). Function names below are the names set in the Binary Ninja database. "RE status" per section: complete = all observable behavior traced to instructions; partial = main behavior traced, details listed as open; unknown = not traced.

Conventions: matrices are row-major 4x4 with row vectors (`v' = v * M`); the translation lives in row 3. `Mul(A, B)` is `A * B` (helper 0x9217e930, `out = r4 * r5`). Message ids are XUI message numbers (`XUIMessage.dwMessage` at +4, `bHandled` at +8, `pvData` at +0x10).

## 1. Element data layout (CXuiElement)

RE status: complete for the fields listed.

The element objproc thunk for every class is `mr r3, r5; b target` (0x9213d958...). The element object (`this`) fields used by rendering:

| Offset | Field |
|---|---|
| +0x00 | HXUIOBJ |
| +0x0c / +0x10 | Width / Height (f32) |
| +0x14 | Opacity (f32) |
| +0x18 / +0x20 / +0x28 | first child / next sibling / parent (element data pointers) |
| +0x30 | Position (vec3) |
| +0x3c | Scale (vec3) |
| +0x48 | Rotation (quaternion x,y,z,w) |
| +0x58 | Pivot (vec3) |
| +0x64 | local matrix (16 f32) |
| +0xa4 | world matrix (16 f32) |
| +0xe8 | Anchor (u32) |
| +0xec | BlendMode (u32) |
| +0xf0 | flags (below) |
| +0xf8 | design Position (vec3) |
| +0x104 / +0x108 | design Width / Height |
| +0x10c / +0x110 | parent size at design capture |
| +0x114 / +0x118 | design Scale.x / Scale.y |

Flags word +0xf0: bit0 Show, bit1 transform dirty, 0x10 "layout in progress", 0x20 "children keep design layout" (see 3), 0x40 DesignTime, 0x80 DisableTimelineRecursion, bits 8-11 ColorWriteFlags, 0x1000 ClipChildren, 0x4000 EnableEffects, 0x10000 DisableFocusRecursion, 0x20000 use alternate render parent, 0x40000 set by message 1.

Element property ids (XuiElement_GetPropDefs 0x92149960): Id 0, Width 1, Height 2, Position 3, Scale 4, Rotation 5, Opacity 6, Anchor 7, Pivot 8, Show 9, BlendMode 10, DisableTimelineRecursion 11, DesignTime 12, ColorWriteFlags 13, ClipChildren 14, EnableEffects 15, DisableFocusRecursion 16.

Element constructor (around 0x92163cb4) initializes flags = 3 (Show | dirty), Anchor = 0, BlendMode = 0.

## 2. Transform

RE status: complete (quaternion-to-matrix helper 0x9217eb10 is VMX128 and was not decoded instruction by instruction; it is used as the standard unit quaternion rotation matrix).

`XuiElement_UpdateTransform` (0x92159988), run when flag bit1 is set (any render pass, hit test, or explicit call):

```
S  = Scaling(Scale.x, Scale.y, Scale.z)
R  = RotationQuaternion(Rotation)
local = Translate(-Pivot) * S * R * Translate(Pivot) * Translate(Position)
clear bit1
parent = elem.parent  (if flag 0x20000: a render parent obtained by message, see open questions)
world = parent ? local * parent.world : local
for each child: set child bit1, recurse
```

The DC composes the vertex shader matrix (`XuiDC_BuildWorldViewProj` 0x92177128):

```
WorldViewProj = World(DC+0x08) * View(DC+0x48) * P,   P = identity with m[2][3] = 0.001
```

`m[2][3] = 0.001` gives `w = 1 + 0.001 * z` after world*view: elements with non-zero Z (Position.z or rotations about X/Y) get perspective. The orthographic viewport matrix (`XuiDC_BuildViewportMatrix` 0x92177280, constant "InvViewport", c4-c7) maps render-target pixels to clip space:

```
[ 2/RTw   0      0  0 ]
[ 0      -2/RTh  0  0 ]
[ 0       0      1  0 ]
[-1       1      0  1 ]
```

RTw/RTh come from the device back buffer description (DC vtable +0xb8). Render state 0x130 is set to 1 at DC reset, which writes bit0 of the D3D shadow dword +0x294c ([INFERENCE] PA_SU_VTX_CNTL PIX_CENTER, the Xenon half-pixel-offset state): pixel centers are at +0.5, the same convention as wgpu.

Element pushes its world matrix with `XuiDC_SetWorldTransform` (DC vtable +0x54) after `XuiElement_PushRenderState` (`XuiElement_PushRenderStateAndTransform` 0x921611d0).

## 3. Root canvas to display

RE status: complete. Function is in DashApp code: `DashApp_SetXuiViewTransform` (0x9210dc88), arguments = back buffer width/height (app struct +0x50/+0x54). It sets the DC view transform:

```
if BBw == 960:                     View = Scaling(960/1120, 720/770, 1)
else:                              View = Scaling(BBw/1280 * 1280/1120, BBh/720 * 720/660, 1)
                                   View.m[3][1] = -64 * BBh/720
```

Design space is 1120 wide. Outside the 960-wide case only y in [58.667, 718.667] (660 units) is visible: `y_screen = y * BBh/660 - 64*BBh/720`. Examples: 1280x720 gives sx = 1.142857, sy = 1.090909, ty = -64. 640x480 gives sx = 0.571429, sy = 0.727273, ty = -42.667. The scale is not uniform (16:9 design stretched to the buffer). The dash itself renders into a 1280x720 buffer and the display scaler produces 480-line output; which back buffer sizes reach this function is DashApp's area. No separate safe-area inset exists in this transform; the 64-unit top offset and the 660-unit visible height are the only adjustment.

## 4. Anchor layout

RE status: complete for the algorithm; not every class that triggers it is enumerated.

Anchor bits (`XuiElement_ApplyAnchor` 0x92157570):

| Bit | Meaning |
|---|---|
| 0x01 | left |
| 0x02 | top |
| 0x04 | right |
| 0x08 | bottom |
| 0x10 | center horizontally |
| 0x20 | center vertically |
| 0x40 | with left+right: scale X instead of stretching width |
| 0x80 | with top+bottom: scale Y instead of stretching height |

Design capture (`XuiElement_CaptureDesignLayout` 0x9215a150) runs on every size set that does not come from anchoring (skipped when the parent has flag 0x10): design W/H = new size, design parent size = parent's current size, design position = Position, design scale = Scale.xy.

Re-layout is triggered only by XuiGroup, XuiScene, XuiVisual (FitToSize) and XuiList when their own size changes (`XuiElement_LayoutChildren` 0x921577e8); other classes do not move children. For each child:

```
dx = newParentW - child.designParentW ; dy = newParentH - child.designParentH
if parent.flags & 0x20: dx = dy = 0
w = child.designW ; h = child.designH ; pos = child.designPos ; scale = child.Scale
if (anchor & 5) == 5:
    if anchor & 0x40:
        sw = child.designScale.x * child.designW
        scale.x = (child.designW == 0) ? 0 : child.designScale.x * (sw + dx) / sw
        if Pivot.x != 0: pos.x = designPos.x + Pivot.x*scale.x - Pivot.x
    else:
        w = (Scale.x == 0) ? 0 : child.designW + dx / Scale.x
elif anchor & 4:    pos.x = designPos.x + dx
elif anchor & 0x10: pos.x = designPos.x + dx * 0.5
(same for Y with 0xa, 0x80, 8, 0x20 and dy)
SetPosition(pos); SetBounds(w, h); SetScale(scale)
```

SetBounds (`XuiElement_SetBounds` 0x921452c0) clamps W/H to >= 0, sends message 0x10 (own size changed: old W,H, new W,H) to the element, message 8 (parent size changed) to each child, then message 0x80d. SetScale (`XuiElement_SetScaleAnchored` 0x92157240) keeps a scale-anchored axis when the parent has flag 0x20 by rescaling with parent W/designW.

## 5. Render traversal and draw order

RE status: complete for message 0 path; message 1 path partial.

Render message 0 (`XuiElement_OnRender` 0x921640f8) on a base element only does work when it has children: push state (6), `XuiElement_RenderChildren` (0x92161218), pop state. Derived classes (Figure, Image, Text...) draw themselves between their own push/pop and do not render children here (children of those classes are rendered through their own handlers; XuiFigure and XuiImage do not draw children).

```
RenderChildren(elem, rd, kind):
    UpdateTransform if dirty
    for child in first..last (sibling order, no Z sort):
        if !(child.flags & Show): skip
        if child.Opacity != 0: send render message (kind 0 or 1)
        else if child.BlendMode != 1 and (child.BlendMode != 0 or rd.blend != 1): send
```

Zero-opacity children are skipped only when their effective blend is "normal" (mode 1, or inherited mode 1); with additive/multiply/min/max modes they are still drawn. Draw order is document order; later siblings draw on top. There is no depth test (Z only affects perspective).

Hit test (message 3, `XuiElement_OnHitTest` 0x92159e18): point transformed by the inverse world matrix, inside when `-1 <= x <= W+1 and -1 <= y <= H+1` and Show set.

Message 1 (`XuiElement_OnRenderPass1` 0x92164078) sets flag 0x40000 and renders children with kind 1; with EnableEffects it goes through the effect path (0x92163e28). Effects are not documented here.

## 6. Render state inheritance (opacity, ColorFactor, BlendMode, ColorWriteFlags, ClipChildren)

RE status: complete.

Render data passed down (`rd`): {DC, colorFactor (ARGB u32), blendMode, colorWriteFlags}. `XuiElement_PushRenderState` (0x92160bd0) saves these and applies:

```
A = (cf >> 24) / 255 * Opacity
cf' = pack(R, G, B, A)  where each channel = round_half_even(c * 255) (3.0 + c*255/2^22 trick, vpkd3d128), RGB unchanged
SetColorFactor(cf')
if elem.BlendMode != 0: SetBlendMode(BlendMode)          (0 = inherit)
if (flags >> 8) & 0xf != 0: SetColorWriteFlags(that)     (0 = inherit)
if ClipChildren: clip planes (below)
```

Opacity accumulates multiplicatively in the color factor alpha and is quantized to 8 bits at every level. RGB of the color factor is only changed by text color (see xui-text.md) and by DC users that set it explicitly. The pixel shaders multiply their output by ColorFactor (PS constant c1).

`XuiRender_PopState` (0x921571a8) restores blend mode, then color write flags, then color factor, then clip planes if they were pushed.

ClipChildren: the 4 element-rectangle edges (corners (0,0),(W,0),(0,H),(W,H) transformed by world * view * viewport) become 4 user clip planes (`XuiDC_SetClipPlanes` 0x921789a0, render state 0xac CLIPPLANEENABLE = 0xf). The previous planes are saved and restored. Nested ClipChildren replaces the parent's planes; it does not intersect.

### BlendMode

`XuiDC_ApplyBlendMode` (0x92177ec8) sets SRCBLEND (state 0x48), DESTBLEND (0x4c), BLENDOP (0x50). Values are Xenon D3DBLEND/D3DBLENDOP (pass-through to RB_BLENDCONTROL fields, verified via the D3D setter table):

| BlendMode | src | dst | op | result |
|---|---|---|---|---|
| 0 | inherit | | | |
| 1 | SRCALPHA (6) | INVSRCALPHA (7) | ADD | normal alpha blend (DC default after reset) |
| 2 | ZERO (0) | SRCCOLOR (4) | ADD | multiply |
| 3 | ONE | ONE | MIN (2) | darken |
| 4 | ONE | ONE | MAX (3) | lighten |
| 5 | SRCALPHA | ONE | ADD | additive |
| 6 | SRCALPHA | ONE | op 1 (SUBTRACT: src - dst) | subtract |
| >6 | no change | | | |

ALPHABLENDENABLE (state 0x3c) is 1 for all XUI drawing. Separate alpha blend is off, so the alpha channel uses the same factors (the D3D setter copies color blend to alpha when SEPARATEALPHABLENDENABLE is 0). Textures are straight (non-premultiplied) alpha.

### ColorWriteFlags

`XuiDC_ApplyColorWrite` (0x92177f98) -> COLORWRITEENABLE (state 0xd4): 0 means 0xf; otherwise bit0 red, bit1 green, bit2 blue, bit3 alpha (same bit order as D3DCOLORWRITEENABLE).

## 7. DC (XUI render device context)

RE status: partial (complete for the parts listed).

Vtable 0x9200d718 (object 0x1d0 bytes), wraps a D3D device wrapper (vtable 0x9200ebf8, at DC+0x14c). Device wrapper SetRenderState(s, v) calls `*(d3d + 0x40 + s)`; SetSamplerState(i, t, v) calls `*(d3d + 0x1d4 + t)`. Those per-state setters come from the D3D table at 0x927984b0 ({getter, setter, default} per state, 101 states) and 0x92798970 (20 sampler states), copied by D3D init 0x9212cca8.

State numbers used by XUI and the register field each setter writes:

| State | Field | Use |
|---|---|---|
| 0x34 FILLMODE | PA_SU_SC_MODE_CNTL bits 3+ | 0, or 0x25 (wireframe) when a debug flag (holder+0x18) is set |
| 0x38 CULLMODE | bits 0-2 | 0 (none) |
| 0x3c ALPHABLENDENABLE | D3D flag | 1 |
| 0x48/0x4c/0x50 | RB_BLENDCONTROL color src/dst/op | BlendMode |
| 0xac CLIPPLANEENABLE | clip control bits 0-5 | 0 or 0xf |
| 0xd4 COLORWRITEENABLE | RB_COLOR_MASK | ColorWriteFlags |
| 0x130 | shadow +0x294c bit0 | 1 |

Sampler 0 states (texture fetch constant fields): 0x00 ADDRESSU (clamp X), 0x04 ADDRESSV (clamp Y), 0x10 MAGFILTER, 0x14 MINFILTER, 0x18 MIPFILTER. `XuiDC_SetTextureFlags` (0x92178888) maps a 6-bit flags value: bits 0-1 ADDRESSU, bits 2-3 ADDRESSV (0 wrap, 1 mirror, 2 clamp, 3 mirror-once; raw Xenon values), 0x10 set = MINFILTER point else linear, 0x20 set = MAGFILTER point else linear. Reset (`XuiDC_ResetSamplerState`): linear/linear, MIPFILTER = 1, wrap/wrap.

Shaders (`XuiDC_Init` 0x921782d0; microcode is Xenos ps_3_0/vs_3_0, constant tables parsed from the blobs):

| Index | Blob | Constants | Use |
|---|---|---|---|
| VS1 | 0x9200c038 | WorldViewProj c0-3, InvViewport c4-7, UVTrans c9, LocalToTexTransform c10-11 | position-only vertices, texcoords generated from position |
| VS2 | 0x9200c290 | WorldViewProj, InvViewport, ShadowOffset c9 | text quads (pos + uv) |
| PS0 | 0x9200c460 | SolidColor c0, ColorFactor c1 | solid brush |
| PS1 | 0x9200c550 | ColorFactor, TextureSampler s0 | texture / linear gradient |
| PS2 | 0x9200c670 | same | radial gradient |
| PS3 | 0x9200cdd8 | same | text without shadow |
| PS4 | 0x9200cef8 | + ShadowColor c2 | text with drop shadow |
| PS5-9 | | + ColorConvertMatrix c4-c6 | same as 0-4, selected when DC+0xb8 != 0 |

Vertex declarations: FLOAT2 position (stride 8) and FLOAT2 position + FLOAT2 texcoord (stride 16). XUI primitive kinds map to D3D types via 0x9200d860: 0 TRIANGLELIST(4), 1 TRIANGLESTRIP(6), 2 TRIANGLEFAN(5), 3 QUADLIST(13).

Texture transform: a brush/texture applies (`XuiTexture_Apply` 0x92178e00) SetTexture, SetTextureFlags, PS1, VS1, UVTrans = translation row of its 4x4 matrix, LocalToTexTransform = the rest. `uv = (pos.xy, 0, 1) * M`.

`XuiDC_DrawRect` (0x92177550): rect {x0,y0,x1,y1} drawn as triangle list (x0,y0),(x1,y0),(x1,y1),(x0,y0),(x1,y1),(x0,y1).

The pixel shader microcode was not decoded. Formulas `out = SolidColor * ColorFactor` and `out = tex * ColorFactor` are [INFERENCE] from constant names and usage.

## 8. XuiFigure

RE status: complete for geometry, stroke, fill brushes; radial gradient scale is [INFERENCE].

Properties (XuiFigure_GetPropDefs 0x92142d20): Stroke (compound at +0xa0: StrokeWidth +4 f32, StrokeColor +8), Fill (compound at +0x14), Closed (bit 1 of +0xc8), Points (custom, ops 0x9279ac38).

Points data: text form `n, then 7 values per point: x, y, c1x, c1y, c2x, c2y, flags`. Binary form (`XuiFigurePoints_LoadBinary` 0x92164a68): `f32 designW, f32 designH, u32 n, n * 6 f32` (no flags). Binary load stores designW/H at +0xbc/+0xc0 and sets bit 4 of +0xc8. In memory a point is 0x1c bytes; the flags dword is not used by rendering. Segment i is the cubic Bezier `pt[i].xy, pt[i].c1, pt[i].c2, pt[i+1].xy`; the last segment always closes to pt[0].

Render (message 0, `XuiFigure_OnMessage` 0x92164f00), only when n >= 1:

```
PushRenderState + world transform
if mesh dirty (+0xc8 bit0): XuiFigure_BuildMesh
if Fill.FillType != 0: brush = XuiFill_GetBrush(W, H); SetBrush(brush, (fill.flags84 >> 2) & 0x3f); DrawFill(mesh)
if StrokeWidth >= 1.0: SetBrush(solid StrokeColor); DrawStroke(mesh, closed = Closed, width)
Pop
```

Message 0x10 (size changed) stores W/H at +0xb4/+0xb8 and marks mesh and fill transform dirty.

Mesh build (`XuiFigure_BuildMesh` 0x92164e18, `XuiShape_Build` 0x9217cfc8):

1. Design size: if +0xc8 bit 4 is clear, `XuiFigure_ComputeDesignBounds` flattens the curves and takes the bounding box width/height of the flattened points (the box min is not subtracted later).
2. Scale `sx = W / designW` (1 if designW == 0), `sy = H / designH`. All point and control coordinates are multiplied by (sx, sy) before flattening.
3. Flatten (`XuiBezier_Flatten` 0x921597a0): emit pt[0]; for each segment recursive midpoint subdivision. A segment is flat (`XuiBezier_IsFlat` 0x92159210) when `|p3-p0|^2 < 0.5`, or when both control points have squared perpendicular distance to the chord line < 0.5. Flat segment emits p3. Split at t = 0.5 by de Casteljau; first half first.
4. Convexity: the point sink tracks the sign of the cross product of successive normalized edges (ignoring |cross| < 1e-4); any sign change marks non-convex.
5. Points list includes the closing point (equal to pt[0]); `count = flattened + 1`; triangle count = count - 3.
6. Convex: drawn as TRIANGLEFAN of the first count-1 points, no indices.
7. Non-convex: ear clipping (`XuiPolygon_EarClip` 0x9217ae80), indices u16, (count-3) triangles.

Ear clipping (O'Rourke-style, all predicates use `Orient(a,b,c) = cross(b-a, c-a)` with a dead zone: > 0.5 is left, < -0.5 is right, else collinear):

```
list = doubly linked ring in point order, node.index = 0..
orientation: m = first node with strictly smallest y; next = m.next; prev = m.prev (if m is head: head.prev.prev)
             flip = !(Orient(m, next, prev) > 0.5)        (flip negates every Left/LeftOn test)
remove last node if it equals head (closing duplicate)
for each v: v.ear = Diagonal(v.prev, v.next)
force = false
while n > 3:
    v = head; found = false
    loop over ring from head: if v.ear or force: emit (v, v.next, v.prev); 
          v.prev.ear = Diagonal(v.prev.prev, v.next); v.next.ear = Diagonal(v.prev, v.next.next)
          unlink v; head = v.next; n--; force = false; found = true; break
    if !found: force = true          (next pass clips head unconditionally)
emit (head, head.next, head.prev)
Diagonal(a,b) = InCone(a,b) && InCone(b,a) && no edge (c, c.next) with c,c.next not in {a,b} Intersects(a,b,c,c.next)
InCone(a,b): a0 = a.prev, a1 = a.next
    if LeftOn(a, a1, a0): Left(a, b, a0) && Left(b, a, a1)
    else: !(LeftOn(a, b, a1) && LeftOn(b, a, a0))
Intersect = IntersectProp || Between(a,b,c) || Between(a,b,d) || Between(c,d,a) || Between(c,d,b)
IntersectProp: false if any of the 4 triples is collinear, else Xor(Left(a,b,c), Left(a,b,d)) && Xor(Left(c,d,a), Left(c,d,b))
Between(a,b,c): collinear and (a.x != b.x ? c.x between a.x,b.x : c.y between a.y,b.y), inclusive
```

Self-intersecting outlines therefore fill according to this ear clipper, not even-odd or non-zero.

Stroke (`XuiShape_BuildStroke` 0x92179788, `XuiShape_DrawStroke` 0x921799f0): one-sided offset of the full width. For segment i from p = pts[i] to q = pts[(i+1) mod (count-1)], `d = normalize(q-p)`, `off = (d.y*w, -d.x*w)`. Strip vertices:

```
segment 0:          p, p+off, q, q+off
segment i (0<i<n-1): p+off_i, p+off_i, p, q+off_i, q
segment n-1:        p+off_i   (closing bevel only)
```

Triangle strip, `5*count - 5` vertices. Closed: `5*count - 7` primitives. Open (Closed = false): `5*m - 8` primitives, where m = number of flattened points before the closing segment (excludes the closing segment and its joins). Joins are bevels formed by the strip. Stroke uses the solid brush (PS0) with StrokeColor.

Fill (`XuiFill_GetBrush` 0x92161590). Fill struct (figure+0x14): +0x0c FillColor, +0x10 TextureFileName, +0x18 NumStops, +0x1c stops (8 bytes each: u32 ARGB color, f32 position), +0x20 bit0 Radial, +0x28 Translation (vec3), +0x34 Scale (vec3), +0x40 Rotation (degrees), +0x84 packed flags, +0x88 FillType.

+0x84: bit0 brush dirty, bit1 transform dirty, bits 2-3 WrapX, bits 4-5 WrapY, bits 6-7 BrushFlags (property values 0x10/0x20 stored shifted left by 2), bits 8-9 TransformVersion. `(flags >> 2) & 0x3f` is exactly the DC texture flags value (WrapX -> ADDRESSU, WrapY -> ADDRESSV, 0x10/0x20 -> point min/mag).

FillType: 0 none; 1 solid (FillColor, PS0); 2 and 3 gradient (needs NumStops >= 2; Radial bit selects radial vs linear, not FillType); 4 texture (TextureFileName resolved like ImagePath, see 9).

Gradient texture (`XuiDC_CreateGradientTexture` 0x9217b840, `XuiDC_FillGradientTexels` 0x921777e0): 128x1 A8R8G8B8.

```
lo = 0, hi = 1
for i in 0..127:
    t = i / 128
    if t > stop[hi].pos: lo = hi; if hi + 1 < count: hi++      (one step per texel)
    if hi == lo: w = 1
    else: d = stop[hi].pos - stop[lo].pos
          w = d < 1e-4 ? 1 : (t < stop[lo].pos ? 0 : (t - stop[lo].pos) / d)
    texel = round(lerp(stop[lo].color, stop[hi].color, w)) per channel (ARGB, /255 then *255 round-half-even)
```

Linear gradient brush: PS1, sampler flags 0xa (clamp, linear), uv from the fill transform. Radial: PS2, same texture, UVTrans offset by -0.5 in u and v (center at uv 0.5). [INFERENCE] PS2 samples at `2 * length(uv - 0.5)` (asset 'defaultbanner' rings use stop positions 0.87-0.96 that only work with this scale); shader microcode not decoded.

Fill transform (`XuiFill_ComputeTexTransform` 0x921613c0), W/H = figure size:

```
TransformVersion == 0:
    R = RotationZ(Rotation deg)
    gradient (type 2/3): M = Scaling(1/W, 1/H, 1) * Translate(-0.5,-0.5,0) * Scaling(Scale) * R * Translate(0.5,0.5,0)
    texture (type 4):    M = Translate(-W/2,-H/2,0) * Scaling(Scale) * R * Translate(W/2,H/2,0) * Scaling(1/texW, 1/texH, 1)
    M = M * Translate(Translation)
TransformVersion != 0:
    ref = (W, H); type 4: ref = (texW, texH); type 2: ref = (min(W,H), min(W,H)); type 3: ref = (max(W,H), max(W,H))
    M = RotationZ(Rotation deg) * Scaling(Scale.x/ref.x, Scale.y/ref.y, 0)
    A' = (W,0)*M ; B' = (W,H)*M ; C' = (0,H)*M        (TransformCoord)
    M.m[3][0] = 0.5 - 0.5*B'.x + Translation.x*A'.x + Translation.y*C'.x
    M.m[3][1] = 0.5 - 0.5*B'.y + Translation.x*A'.y + Translation.y*C'.y
```

## 9. XuiImage, XuiImagePresenter

RE status: complete for layout and sampling; texture decoding is app-provided (DashApp area).

Properties: SizeMode (custom accessor, stored in the image brush), ImagePath (+4), BrushFlags (+0x84). Image brush struct at object+4: +0 path, +4 SizeMode, +8 dirty byte, +0xc rect {x0,y0,x1,y1}, +0x74 kind (1 texture, 2 XUI visual), +0x80 BrushFlags, +0x84 texture.

Path resolution (`XuiImage_LoadPath` 0x9215ebd0, `XuiResolveRelativePath` 0x9215cde8): absolute URLs (with scheme) are used as-is; otherwise joined with the element base path. If characters [len-4, len-1) equal ".xu" the path is loaded as a XUI scene and drawn as a visual (kind 2); otherwise a texture is created through the app texture-loader callback (XuiRenderInitShared field +0x10, DC vtable +0xc0). Assets are PNG.

Draw (`XuiImage_Draw` 0x92162fd8): kind 1: SetTexture(tex, BrushFlags | 0xa) then DrawRect(rect). `| 0xa` sets clamp U and V; BrushFlags 0x10/0x20 select point min/mag filtering (other BrushFlags bits would alter the address modes). Kind 2 positions/scales the child visual by the same size-mode rules (0x9215f198).

SizeMode (`XuiImage_ComputeLayout` 0x9215eca0), tested in this order; W/H = element size, tw/th = texture size; if W or H is 0 nothing is drawn:

| Bit | Mode | rect | uv |
|---|---|---|---|
| 0x01 | resize element to texture | (0,0,tw,th) | pos / (tw,th) |
| 0x02 | center, no scaling | x0 = W/2 - tw/2, y0 = H/2 - th/2, size tw x th, clamped to [0,W]x[0,H] | (pos - (x0,y0)) / (tw,th) |
| 0x04 | stretch | (0,0,W,H) | pos / (W,H) |
| 0x08 | fit keeping aspect, top-left | dw,dh = fit (wider texture: dw=W, dh=W*th/tw; else dh=H, dw=H*tw/th) at (0,0) | pos / (dw,dh) |
| 0x10 | fit keeping aspect, centered | same size, centered on the free axis | (pos - (x0,y0)) / (dw,dh) |
| 0 | none | (0,0,min(W,tw),min(H,th)) | pos / (tw,th) |

The binary computes these via `uv = ((pos - C) * s + C) / tex + t` with C = half of the used size; the table gives the simplified result.

XuiImagePresenter (0x92163090) obtains its source from the parent control by message: a path (handled like XuiImage), a brush, or a texture handle (mode 4, drawn with `XuiDC` vtable +0x24 using flags | 0xa).

## 10. XuiNineGrid

RE status: complete (center variant); NoCenter variant structure partial.

Properties: TextureFileName/ImagePath (+8), LeftOffset +0xc, TopOffset +0x10, RightOffset +0x14, BottomOffset +0x18 (integers), NoCenter (+0x20, bool). Texture sampled with flags 0xa (clamp, linear).

Strip (`XuiNineGrid_BuildStrip` 0x92163298), 25 vertices (x, y, u, v), drawn as TRIANGLESTRIP with 23 primitives. Columns X = {0, L, W-R, W}, U = {0, L/tw, 1-R/tw, 1}. Rows Y = {0, T, H-T, H}, V = {0, T/th, 1-B/th, 1}.

The third row uses `H - TopOffset` for position but `1 - BottomOffset/th` for v (binary behavior; with asymmetric offsets the bottom border is drawn TopOffset pixels high).

Vertex order: row1/row0 pairs left to right ((0,T),(0,0),(L,T),(L,0),(W-R,T),(W-R,0),(W,T),(W,0)), repeat (W,T), then row2/row1 pairs right to left ((W,H-T),(W-R,T),(W-R,H-T),(L,T),(L,H-T),(0,T),(0,H-T)), three copies of (0,H-T), then row3/row2 pairs left to right ((0,H),(L,H-T),(L,H),(W-R,H-T),(W-R,H),(W,H-T),(W,H)). NoCenter (`XuiNineGrid_BuildStripNoCenter` 0x92160330) builds 28 vertices / 26 primitives with degenerate triangles over the center cell.

## 11. Other visual classes

RE status: partial.

- XuiGroup (0x9215cc68), XuiScene (0x92169720), XuiVisual (0x9215bf68): no drawing of their own; they lay out children on size change (4) and handle focus. XuiCanvas (0x9215ce58) and XuiControl (0x9215aa68) render via the base element path.
- XuiHtmlElement/Presenter/Control are registered from 0x921c3cf8 with objproc 0x9210c258 (a C++ virtual dispatch thunk); not traced.
- XuiTextureSurface: no class of that name is registered in 6717.

## Nuances

- Opacity is quantized to 8 bits in the color factor at each nesting level.
- Zero-opacity children are still drawn unless the effective blend mode is 1.
- Nested ClipChildren replaces, not intersects, the clip rectangle; clipping is by 4 clip planes, so rotated parents clip to rotated rectangles.
- WorldViewProj has m[2][3] = 0.001: Z produces perspective.
- Figure strokes are one-sided (the full width lies on one side of the outline) and are only drawn when StrokeWidth >= 1.
- Figure fill always closes the outline; the Closed property only affects the stroke.
- Figure points are scaled by W/designW where designW is the flattened bounding-box size (text form) or the stored size (binary form); the box origin is not subtracted.
- Ear clipping and convex fan selection use dead zones of 0.5 (area units) and 1e-4.
- The gradient stop search advances at most one stop per texel.
- NineGrid lower inner row uses TopOffset.
- The root view is non-uniform (1120x660 visible design area onto the back buffer).

## Open questions

- Pixel/vertex shader microcode semantics (texture interpolation perspective-correct or affine, exact radial formula, ColorConvertMatrix use) are inferred from constant names; decoding Xenos microcode would settle them.
- Render parent used when element flag 0x20000 is set (message-based lookup in UpdateTransform).
- When DC+0xb8 (shader variants 5-9) is set.
- Texture image decoding (PNG) is supplied by the app loader callback.

## Reimplementation

`exui-runtime` (generic XUI runtime, no dashboard knowledge):

- `scene::ElementTree` holds elements in an arena with generation-checked `ElementId`s. `scene::instantiate` creates elements from a `xur::Document`, applies class defaults from the schema and then the loaded properties in file order, and attaches each element to its parent before creating its children.
- `ElementTree::set_property` runs the documented setters: Width/Height through SetBounds and design capture, Position through `XuiElementSetPosition`, Anchor re-layout for XuiGroup, XuiScene, XuiVisual and XuiList. Size changes are queued for the controls runtime (`take_size_changes`).
- `scene::render` walks the tree and writes an `xbox360-draw` draw list. World, view and the `m[2][3] = 0.001` projection are applied on the CPU: a vertex lands at `(x / w, y / w)` with `w = w0 + 0.001 * z`. ClipChildren planes become a draw-list clip rectangle when they are axis-aligned (pixel-center rule) and are clipped on the CPU otherwise.
- `Ui` owns the registry, resources and the root canvas; the runtime holds the fonts and the text renderer. The DC view transform and the clear color are set by the application.

Deviations and inferences:

- The draw list is 2D, so texture coordinates are interpolated affinely; Xenos interpolates perspective-correct. This only differs for elements with depth.
- ColorWriteFlags are stored but not applied (no genuine scene sets them).
- Draw-list samplers have one filter; it follows the magnification bit (0x20). Mirror-once addressing (3) maps to mirror.
- BlendMode 2 to 4 use straight vertex colors and unpremultiplied texels (`BlendMode::uses_straight_source`); a texel with alpha 0 has no color left after premultiplied decoding. Figure gradients drawn with these modes are built with opaque texels instead, so their transparent stops keep their color (the `twist_nitem` edge fades multiply by white). This changes only the target alpha, which no XUI blend factor reads.
- Triangles with a vertex at `w <= 0` are dropped instead of clipped against the near plane. A ClipChildren rectangle with a corner at `w <= 0` or with no area clips everything.
- A zero-length figure edge normalizes to the zero vector.
- Effects are not drawn. Scene image content (paths ending in `.xu?`) is drawn as a plain child of the XuiImage, without size-mode placement.
- The order of property application and attachment during instantiation was not traced; parents get their properties before their children are created.

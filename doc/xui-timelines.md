# XUI timelines, transitions and scene navigation

Timeline playback, named frames, keyframe interpolation, timeline notifications, scene transitions and scene navigation of the XUI runtime in dash 2.0.6717. Message dispatch, focus, input and the control classes are in [xui-controls.md](xui-controls.md). The XUR records that timelines are built from are in [xui-format.md](xui-format.md).

All addresses are in the retail dash image. Functions listed here are renamed in the Binary Ninja database and carry `RE status:` comments. Constants were read from the image. `FMS` below is the float constant 16.666666 at 0x9200b9bc (milliseconds per frame, 60 frames per second). `EPS` is 0.1 at 0x9200b9c0. "TL" is a timeline object.

Summary for implementers:

- Time is float milliseconds, 60 frames per second, advanced once per frame by a variable wall-clock dt (section 4).
- `PlayTimeline(element, start, initial, end, loop, recurse)` sets a play range; the first tick after it jumps to `initial` without consuming dt (sections 3 and 4).
- Keyframe segments are linear, hold or ease (a cubic Bezier on the y axis only); vectors and colors interpolate per component, quaternions slerp, bools, unsigned values and strings step (section 4).
- Named frames carry commands: none, stop, and three jump variants (section 5).
- 0x14 (start) and 0xf (end) notify listeners; 0xf repeats every tick while held at the end frame (section 6).
- Scene transitions are named frames on the scene (`TransTo`, `TransFrom`, ... with optional index suffix) or a skin visual wrapped around the scene in an XuiTransition element (section 7).

## 1. Data structures

### 1.1 Element instance fields used by timelines (XuiElement class-level instance)

| Offset | Meaning | Status |
| --- | --- | --- |
| +0x00 | owner hObj | complete |
| +0x18 | first child instance; siblings linked at child +0x20 | complete |
| +0x2c | effect children list: children whose class derives from XuiEffect are linked here instead of +0x18 (0x921643a8, 0x92164530); also walked by play/stop recursion without the XuiVisual filter | complete |
| +0xe4 | timeline object pointer (TL), 0 if none | complete |
| +0xf0 bit 0x4 | input disabled (set by XuiElementSetInputEnabled(h,0), recursive) | complete |
| +0xf0 bit 0x8 | timeline recursion disabled by API XuiElementSetTimelineRecursionDisabled 0x9214e150 | complete |
| +0xf0 bit 0x80 | `DisableTimelineRecursion` property (propdef: field 0xf0, mask 0x80, bool) | complete |
| +0xf0 bit 0x2000 | element was instantiated without autoplay: its own TL is skipped by recursive play (not by stop). Query: XuiElementIsTimelineAutoplayDisabled 0x9214e1b8 | complete |

### 1.2 PlayState (16 bytes + 8; embedded at TL+0x04 and at subtimeline +0x00)

| Rel | Field |
| --- | --- |
| +0x00 | flags: bit0 stopped, bit1 start notification pending |
| +0x04 | start frame (i32) |
| +0x08 | end frame (i32); negative means "use length" |
| +0x0c | loop flag |
| +0x10 | saved start frame (written only when play has save=1; no reader found) |
| +0x14 | saved end frame (written only when save=1). >= 0 enables the end notification 0xf |

XuiPlayState_Init 0x921720a8 sets {flags=1, start=0, end=0, loop=1}; saved fields are not initialized (XuiAlloc result).

### 1.3 Timeline object TL (XuiAlloc 0x50; ctor XuiTimeline_Init 0x92154410)

| Offset | Field | Init |
| --- | --- | --- |
| +0x00 | owner hObj (set by XuiElement_SetTimelineImpl) | 0 |
| +0x04 | PlayState (flags, start +0x08, end +0x0c, loop +0x10, savedStart +0x14, savedEnd +0x18) | {1,0,0,1} |
| +0x1c | subtimeline count | 0 |
| +0x20 | subtimeline array (0x44 each) | 0 |
| +0x24 | named frame array (0x10 each) | 0 |
| +0x28 | named frame count | 0 |
| +0x2c | named frames shared flag (array owned by loaded object) | 0 |
| +0x30 | shared owner reference (released with 0x92170400) | 0 |
| +0x34 | length in frames = max over subtimelines of last keyframe frame (XuiTimeline_ComputeLength 0x92154c28) | 0 |
| +0x38 | bit0 seek pending | 1 |
| +0x3c | initial offset = initial - start | 0 |
| +0x40 | index of next named frame, -1 = must search | -1 |
| +0x44 | stop-next flag (set by command 4) | 0 |
| +0x48 | skip-next-stop flag (set by command 3) | 0 |
| +0x4c | position in ms (float) | 0.0 |

### 1.4 Runtime named frame (0x10)

| Rel | Field |
| --- | --- |
| +0x00 | wchar* name (XuiAlloc copy) |
| +0x04 | frame (u32 in comparisons) |
| +0x08 | command (u32, from XUR u8) |
| +0x0c | target named frame index; resolved only when command is not 0 and not 1; -1 if target name not found; uninitialized for commands 0/1 |

Named frames must be sorted by frame: the search functions scan forward and stop at the first match.

### 1.5 Subtimeline (0x44; XuiSubTimeline_Init 0x921546e0)

| Offset | Field |
| --- | --- |
| +0x00 | PlayState (flags +0x00, start +0x04, end +0x08, loop +0x0c, savedStart +0x10, savedEnd +0x14) |
| +0x18 | target hObj (child found by Id; for transitions, the scene) |
| +0x1c | keyframe count |
| +0x20 | keyframe array (0x1c each) |
| +0x24 | frame of last keyframe (fallback end) |
| +0x28 | bit0 seek pending |
| +0x2c | initial offset |
| +0x30 | position in ms (float) |
| +0x34 | property path count |
| +0x38 | property path array (0xc each) |
| +0x3c | keyframes shared flag |
| +0x40 | shared owner reference |

Property path (0xc): +0 compound/custom object (0 for a depth-1 path; else resolved object whose vtable +0x14 is the setter), +4 property definition pointer (definition +4 = global property id, definition +0 flags), +8 array index (from XUR path header bit 0x80, else 0).

### 1.6 Keyframe (0x1c; allocated by XuiKeyframeArrayAlloc 0x92172638)

| Offset | Field |
| --- | --- |
| +0x00 | frame (u32) |
| +0x04 | interpolation type: 0 linear, 1 hold, 2 ease |
| +0x08 | P1.x (float, unused by evaluation) |
| +0x0c | P1.y (float) |
| +0x10 | P2.x (float, unused by evaluation) |
| +0x14 | P2.y (float) |
| +0x18 | value array (XuiValue 0x14 each, one per property path) |

XuiValue (0x14): +0 type (1 bool, 2 int, 3 unsigned, 4 float, 5 string, 6 color packed ARGB, 7 vector 3 floats, 8 quaternion 4 floats), +4 data. Setters: XuiValue_SetInt 0x92156190, SetFloat 0x92156220, SetColor 0x92156268, SetVector 0x92156388, SetQuaternion 0x92156890, init 0x92156070, clear 0x921560d8.

Status of all of section 1: complete except where marked.

## 2. Construction from a loaded scene

Loaded object record (XUR loader, see [xui-format.md](xui-format.md)): +0 class name, +4 property block, +8 children, +0xc named frames (0x10 each: name, frame, command, target name), +0x10 named frame count, +0x14 cached runtime named frame array, +0x1c timeline count, +0x20 timeline records (0x20 each: +4 element name, +8 path count, +0xc paths, +0x10 per-path index, +0x14 keyframe count, +0x18 keyframes 0x10 each {+0 frame, +4 flag byte with interpolation << 1, +5 s8 ease in, +6 s8 ease out, +7 s8 ease scale, +8 value count, +0xc values}).

XuiInstantiateObjectTree 0x921523f8 (args: object, &hOut, autoplay, share, owner, &counter, ...):
```
create element of class; if autoplay == 0: element.flags |= 0x2000
apply properties; recurse into children (same autoplay)
if object.timelineRecords or object.namedFrames:
    tl = XuiAlloc(0x50); XuiTimeline_Init(tl)
    XuiTimeline_BuildFromObject(element, object, tl, share, owner)    // 0x92152080
    if autoplay: XuiTimeline_Play(tl, start=0, initial=0, end=-1, loop=1, save=1)
    XuiElementSetTimeline(element, tl)                                  // registers tl
```
XuiTimeline_BuildFromObject copies named frames (target index resolved by linear name search, only for commands other than 0 and 1), then XuiSubTimeline_BuildFromRecord 0x92151ff8 per record: find child by Id (0x9214db88, then 0x9214dc10), XuiSubTimeline_Build 0x921510f8 resolves paths and converts keyframes:
```
kf.frame = rec.frame
if rec.flags & 2: kf.type = 1        // XUR interpolation 1 (also 3)
elif rec.flags & 4: kf.type = 2      // XUR interpolation 2
else: kf.type = 0                    // XUR interpolation 0
XuiEaseToBezierPoints(easeIn, easeOut, easeScale, &kf.P1, &kf.P2)   // bytes sign-extended
```
XuiEaseToBezierPoints 0x9214fa28:
```
s  = scale * 0.01
a1 = (1 - easeIn * 0.01) * (pi/4)         // pi/4 = 0x3f490fdb
a2 = (easeOut * 0.01 - 1) * (-pi/4)       // = (1 - easeOut*0.01) * pi/4
P1 = rotate((s, 0), a1)                   // matrix from 0x9217f088, applied by 0x9217e758
P2 = rotate((-s, 0), a2) + (1, 1)
```
[INFERENCE] rotate is the standard counterclockwise 2D rotation (x' = x cos a - y sin a, y' = x sin a + y cos a). The matrix builder 0x9217ef68 stores sin, cos and -sin rows through VMX128 instructions not decoded here; the inference gives P1 = (s cos a1, s sin a1), P2 = (1 - s cos a2, 1 - s sin a2), which yields a monotonic curve and linear motion for ease 0. Status: partial (sign).

Element creation via scene loading (XuiSceneCreate 0x92153048 -> XuiSceneCreateInternal 0x92152da0 -> 0x9214a9f8 -> 0x92153140 -> 0x92153058 flag 0) uses autoplay = 1. 0x92153058 passes autoplay = (flag & 1) == 0.

`XuiSceneCreateInternal(base, path, initData, &out, flags)` 0x92152da0 (complete):
```
full = path absolute (0x9216ba10) ? copy(path) : combine(base, path) (0x9216bac0)
root = load full (0x9214a9f8, flags)                      // the file's root element, usually an XuiCanvas
scene = first child of root, in child order, that has a XuiScene level
if !scene: XuiDestroyObject(root); return 0x8030000a
if scene is not the only child: XuiElementGetId(scene)     // result unused
XuiElementSetBasePath(scene, full)                         // 0x9214f350
load depth += 1; unlink scene (0x9214d478); XuiDestroyObject(root); load depth -= 1
XuiInitElementTree(scene, initData)                        // 0x92150c98
if flags & 2: broadcast 0x29 to scene
out = scene
```
The file's root and every other child of it are destroyed; the returned scene is detached. `XuiApp_LoadFirstScene` and the scene press routing (PressPath) both go through `XuiSceneCreate`.

Element destroy (message 0xa in XuiElement impl 0x92168ea0): XuiTimeline_Unregister, XuiTimeline_Destroy, XuiFree, +0xe4 = 0.

## 3. Public timeline API

| Address | Name | Behavior | Status |
| --- | --- | --- | --- |
| 0x9214e070 | XuiElementPlayTimeline(h, start, initial, end, loop, recurse) | -> XuiElement_PlayTimelineImpl 0x9215a050 | complete |
| 0x9214e020 | XuiElementStopTimeline(h, recurse) | -> XuiElement_StopTimelineImpl 0x92159f98 | complete |
| 0x9214e0d0 | XuiElementFindNamedFrame(h, name, &frame) | out=-1; E_INVALIDARG if out null; 0x80300016 if not an element; -> 0x921573e8 wcscmp scan, out = frame | complete |
| 0x9214dfc8 | XuiElementGetTimeline(h, &tl) | out = +0xe4 | complete |
| 0x9214df78 | XuiElementSetTimeline(h, tl) | -> 0x92159f20 | complete |
| 0x9214e150 | XuiElementSetTimelineRecursionDisabled(h, b) | flag 0x8 | complete |

```
XuiElement_PlayTimelineImpl(e, start, initial, end, loop, recurse):
    if e.tl and !(e.flags & 0x2000):
        XuiTimeline_Play(e.tl, start, initial, end, loop, save=1)
    if recurse and !(e.flags & 0x8) and !(e.flags & 0x80):
        for c in e.children(+0x18): if c is not XuiVisual-derived: PlayTimelineImpl(c, same args, recurse=1)
        for c in e.list2(+0x2c): PlayTimelineImpl(c, same args, recurse=1)
    return 0

XuiTimeline_Play(tl, start, initial, end, loop, save):        // 0x921545d0
    tl.flags = 2; tl.start = start; tl.end = end; tl.loop = loop
    if save: tl.savedStart = start; tl.savedEnd = end
    tl.stopNext = 0; tl.skipStop = 0; tl.seek = 1; tl.nextNF = -1; tl.initOff = initial - start
    for s in tl.subs:
        s.flags = 2; s.start = start; s.end = end; s.loop = loop
        if save: s.savedStart = start; s.savedEnd = end
        s.seek |= 1; s.initOff = initial - start

XuiElement_StopTimelineImpl(e, recurse):
    if e.tl: e.tl.flags = 1            // XuiTimeline_Stop 0x921545c0; no 0x2000 check
    recursion identical to play
```
Play does not validate frames and does not touch the position. Children are always played with recurse = 1. XuiVisual-derived children are skipped only in the +0x18 list.

XuiElement_SetTimelineImpl 0x92159f20: unregister and destroy the old TL, store new, TL+0 = owner hObj, XuiTimeline_Register 0x92155a48 (append to the active list).

## 4. Tick

Active list: count/array global at 0x9280b330 (+0 array, +4 count), critical section 0x9280b314, iteration index global 0x9279ac34. XuiTimeline_Unregister 0x92154cc0 removes an entry and decrements the iteration index when the removed slot is <= it, so a TL destroyed by a message handler during the tick does not cause a skip. Timelines registered during the tick are processed in the same tick (count reread each iteration).

```
XuiTimelinesTick(float dtMs):                 // 0x92155b18
    enter cs; g_index = 0
    while g_index < count: XuiTimeline_Advance(list[g_index], dtMs); g_index++
    leave cs
```
Callers: DashApp_RunFrame 0x9210ee90 (dt clamped to <= 60000.0 ms, constant 0x92001f30), XuiApp_RunFrame 0x9210d280 (no clamp), 0x9210fc78 (no clamp), 0x922a6c48 (clamp 1000.0 ms). dt comes from XuiApp_GetFrameDeltaMs 0x9210d030: (timebase_now - last) as u64 -> double / frequency (int64 at +0, fcfid) * 1000.0, last updated every call. One call per frame, variable dt. Status: complete.

Direct caller 0x922725a8 (dash code) calls XuiTimeline_Advance(tl, 0.0) right after playing control state frames to apply them in the same frame. A dt of 0 performs the pending seek and applies values.

### 4.1 XuiTimeline_Advance 0x92155600 (complete)

`frameMs(f)` = (float)(double)f * FMS (single multiply). Named frame frames are converted unsigned in comparisons marked (u) and signed elsewhere.

```
Advance(tl, dt):
    if tl.flags & 1: return
    if tl.seek & 1:
        tl.pos = frameMs(tl.start + tl.initOff)
        tl.nextNF = FindNext(tl, 0, tl.pos - 0.2)
        dt = 0; tl.initOff = 0; tl.seek &= ~1
    endMs = frameMs(tl.end); startMs = frameMs(tl.start)
    if endMs < 0: endMs = frameMs(tl.length)
    if |endMs - startMs| < EPS: dt = 0
    first = 1; endHit = 0
    do:
        if tl.nextNF < 0: tl.nextNF = FindNext(tl, 0, tl.pos)
        step = dt
        if tl.nextNF >= 0:
            nf = frameMs(nfs[tl.nextNF].frame) (u)
            if nf >= tl.pos and tl.pos + dt > frameMs(frame) (signed):
                step = nf - tl.pos; if step < EPS: step = 0
        newPos = tl.pos + step; tl.pos = newPos
        if newPos >= endMs - EPS:
            step -= newPos - endMs; if step < EPS: step = 0
            tl.pos = endMs
        for s in tl.subs: XuiSubTimeline_Advance(s, step)
        if first and (tl.flags & 2):
            XuiTimeline_BubbleNotify(tl, 0x14); first = 0; tl.flags &= ~2
        if tl.savedEnd >= 0 and tl.pos >= frameMs(tl.savedEnd) - EPS: endHit = 1
        if tl.stopNext: tl.flags = 1; tl.stopNext = 0
        if tl.nextNF >= 0 and tl.pos >= frameMs(nfs[tl.nextNF].frame) (u) - EPS:
            nf = nfs[tl.nextNF]; tl.nextNF += 1; if tl.nextNF >= count: tl.nextNF = -1
            if nf.command == 1:
                if tl.skipStop: tl.skipStop = 0 else: tl.flags = 1
            elif nf.command in 2..4:
                if 0 <= nf.target < count:
                    f = nfs[nf.target].frame
                    XuiTimeline_Play(tl, f, f, tl.length, loop=1, save=0)
                    tl.pos = frameMs(tl.start); startMs = tl.pos
                    tl.nextNF = FindNext(tl, 0, tl.pos - 0.2); tl.seek &= ~1
                    endMs = frameMs(tl.end); if endMs < 0: endMs = frameMs(tl.length)
                if nf.command == 4: tl.stopNext = 1
                elif nf.command == 3: tl.skipStop = 1
        if newPos >= endMs and tl.loop: tl.pos = startMs
        if endHit: XuiTimeline_BubbleNotify(tl, 0xf); return
        if tl.flags & 1: return
        if step < EPS: step = EPS
        dt -= step
    while dt > EPS

FindNext(tl, i, posMs):        // XuiTimeline_FindNextNamedFrame 0x92154550
    for i..count-1: if frameMs(nfs[i].frame) (u) >= posMs + EPS: return i
    return -1
```
`0.2` is the constant at 0x9200b9e0. `newPos` is the unclamped position. Unused helpers: XuiTimeline_FindNamedFrameIndex 0x92154498 (by name), XuiTimeline_FindNamedFrameAtOrAfterFrame 0x92154508, XuiSubTimeline_Play 0x921543d0 (no callers).

### 4.2 Subtimeline evaluation

```
XuiSubTimeline_Advance(s, step):            // 0x92155508
    if (s.flags & 1) or s.kfCount < 1: return
    if s.seek & 1: s.seek &= ~1; s.pos = frameMs(s.start + s.initOff); s.initOff = 0
    XuiSubTimeline_ApplyAt(s, s.pos + step)

XuiSubTimeline_ApplyAt(s, p):               // 0x92154e10
    endMs = frameMs(s.end); if endMs < 0: endMs = frameMs(s.lastKf)
    startMs = frameMs(s.start)
    endHit = s.savedEnd >= 0 and p >= frameMs(s.savedEnd) - EPS
    live = 1
    if p <= endMs + EPS: s.pos = p
    elif s.loop: s.pos = max(startMs, startMs + p - endMs - FMS)
    else: s.pos = endMs; live = 0
    k = largest index with frameMs(kf[k].frame) (u) - EPS <= s.pos      // scan from last
    if none: goto notify_end                                            // nothing applied, no 0x14
    k1 = min(k + 1, n - 1)
    t = (k1 != k) ? (s.pos - frameMs(kf[k].frame)) / ((float)(u32)(kf[k1].frame - kf[k].frame) * FMS) : 0
    if t < 0: t = 0                                                      // no upper clamp
    if kf[k].type == 2:
        u = 1 - t
        t = (kf[k].P1y * u + kf[k].P2y * t) * u * t * 3.0 + t * t * t   // 1.0 0x92001dcc, 3.0 0x920059c8
    for i, path in s.paths:
        if path.def.flags & 0x20 and not (live and |frameMs(kf[k].frame) - s.pos| <= EPS): continue
        if kf[k].type == 1: v = kf[k].values[i]
        elif XuiValueInterpolate(&tmp, kf[k].values[i], kf[k1].values[i], t): v = tmp
        else: v = kf[k].values[i]
        if path.obj: path.obj->vtbl[0x14](path.obj, path.def.id, path.index, v)
        else: XuiObjectSetPropertyById(s.target, path.def.id, path.index, v)   // 0x92140d40
    if s.flags & 2: XuiSubTimeline_SendNotify(s, 0x14); s.flags &= ~2
notify_end:
    if endHit: XuiSubTimeline_SendNotify(s, 0xf)
```
The interpolation type of a segment is the type of its first keyframe. Values are written on every evaluation even when unchanged. The loop wrap maps one frame past the end onto the start frame. With end = -1 each subtimeline loops on its own last keyframe while the TL position runs to the TL length. Status: complete.

XuiObjectSetPropertyById walks the element's class levels (class +0x2c first id, +0x30 count, +0x28 next) and sets through 0x92140a10; it returns 0x80300007 when no level owns the id. The return value is ignored by the timeline code.

### 4.3 XuiValueInterpolate 0x921547f0

| Type | Result | Status |
| --- | --- | --- |
| 2 int | fctiwz((float)(i32)a + (float)(i32)(b - a) * t); truncation toward zero; 32-bit wrap in b - a | complete |
| 4 float | (b - a) * t + a | complete |
| 6 color | channels R=(c>>16)&0xff, G=(c>>8)&0xff, B=c&0xff, A=c>>24 each * 0.0039215689 (0x920048f0), vector lerp v = a + (b - a) * t (vmaddfp), then v' = 3.0 - v * (-6.0797e-5) (vnmsubfp, constant 0xb87f0000 = 255/2^22), packed by VMX128 opcode word 0x19a106d0 (D3DCOLOR pack) into ARGB | complete except pack saturation [INFERENCE: hardware pack takes the low 8 mantissa bits, i.e. round-to-nearest of v*255] |
| 7 vector | per component (b - a) * t + a on x, y, z | complete |
| 8 quaternion | 0x9217f018 -> 0x9217e4d0 VMX128 spherical interpolation. Observed: dot product, sign-mask xor (shortest path), reciprocal square root, calls 0x9217e1b8 and 0x9217df50, constants 0.5 and 1/65536 built with vcfsx | partial |
| 1 bool, 3 unsigned, 5 string, others | returns 0: the first keyframe value is used (step) | complete |

## 5. Named frame commands

| Command | Runtime behavior when playback reaches the frame | Status |
| --- | --- | --- |
| 0 | label only | complete |
| 1 | stop (tl.flags = 1), unless skipStop is set, in which case skipStop is cleared and playback continues | complete |
| 2 | jump: restart at target frame, end = TL length, loop = 1, saved fields kept, continue playing | complete |
| 3 | jump as 2, then ignore the next command 1 | complete |
| 4 | jump as 2, then stop on the next sub-step after the target frame values were applied | complete |
| other | ignored | complete |

The XUR stores the command as u8 and the target as a frame name. The decompiled dash XML contains only command `stop`. Command names for 2..4 are not present in the binary.

A command at the exact frame where playback starts runs on the first tick (FindNext with pos - 0.2 finds frames >= initial - 0.1). The jump commands use `loop = 1` and `end = length` regardless of the original play arguments, so the original end frame no longer clamps the position but the saved end still raises 0xf.

## 6. Notifications 0x14 and 0xf

Builder XuiBuildTimelineNotifyMsg 0x92154680(msg, data, id, hObj): data 4 bytes `{+0 u32 hObj}`.

| Id | Meaning | Sender | Delivery |
| --- | --- | --- | --- |
| 0x14 | timeline started | TL: first tick after a play (flags bit1); subtimeline: first evaluation that applied values after a play | TL: XuiBubbleMessage from TL owner (XuiTimeline_BubbleNotify 0x92154c70); subtimeline: XuiSendMessage to target (XuiSubTimeline_SendNotify 0x92154bd8) |
| 0xf | timeline reached saved end frame | TL: when pos >= savedEnd*FMS - 0.1; subtimeline: same test on its own position; XuiTimeline_FinishNow 0x9215b5c0 | same as 0x14 |

0xf is raised on every tick while the condition holds: once per pass for a looping play, every tick for a held non-looping play, once if a stop command at the end frame stops the TL. A play with end = -1 (autoplay, scene fallback) never raises 0xf. Data +0 identifies the TL owner, so ancestors receiving the bubbled message compare it with their own hObj.

Consumers found:
- XuiControl 0x9215aa68 -> XuiControl_OnTimelineEnd 0x9215a980 (no hObj check). When its pending condition 0x9220eaf0 holds it resets press state, replays state frames and calls `XuiTimeline_Play(vtl, vtl.start, vtl.end, vtl.end, vtl.loop, vtl.savedEnd != -1)` on the visual's TL: this seeks the current state range to its end frame. The next tick applies end-frame values with dt dropped, runs the command at that frame, raises 0x14 and, when savedEnd is set, 0xf again.
- XuiScene_OnMessage 0x92169720: own TL only (section 7.3).
- XuiTransition_OnMessage 0x92165438: own TL only (section 7.4).
- XuiTabScene_OnMessage 0x92160100: own TL only, clears animating flag (section 8).
- XuiList 0x92166b28 (see the list section of xui-controls.md).
- DashMainScene_OnBootTimelineEnd 0x9210fdb0 (dashboard application code).

## 7. Scenes, transitions and navigation

### 7.1 Scene instance (XuiScene level, XuiGetSceneInstance 0x9214c790)

| Offset | Field | Status |
| --- | --- | --- |
| +0x00 | hObj | complete |
| +0x04 | DefaultFocus (string; prop 0) | complete |
| +0x08 | last focused control | partial |
| +0x0c/+0x10 | registered control array / count (messages 0x4, 0x20) | partial |
| +0x18 | base path (0x18 get, 0x19 set) | partial |
| +0x1c | string (0x22 get, 0x23 set) | unknown |
| +0x20 | back scene (0x1b get {+0 out}; 0x1c set {+0 new, +4 out previous}) | complete |
| +0x24 | transition partner: forward nav stores next in cur, back nav stores from-scene in target | complete |
| +0x28 | TransFrom (string, prop 1) | complete |
| +0x2c | TransTo (prop 2) | complete |
| +0x30 | TransBackFrom (prop 3) | complete |
| +0x34 | TransBackTo (prop 4) | complete |
| +0x38 | flags: bit0 own named-frame transition running, bit1 XuiTransition wrapper running, bit2 set on 0x1f and consumed by 0x921505c8, bit3 timer flag (0x1d/0x1e/0x27 timer 0xaabbccdd), bit4 IgnorePresses (prop 6, bool mask 0x10), bit5 destroying (set on 0xa) | partial (bits 2, 3) |
| +0x3c | InterruptTransitions (prop 5, type 3 with custom enum ops, flags 8) | no reader found |
| +0x40 | pending transition kind | complete |
| +0x44 | pending transition action | complete |
| +0x48 | source transition index (XuiSceneSetTransIndex(h, 0, i)) | complete |
| +0x4c | destination transition index (XuiSceneSetTransIndex(h, 1, i)) | complete |
| +0x50 | saved focus | partial |

Transition kinds: 1 TransTo, 2 TransFrom, 3 TransBackTo, 4 TransBackFrom. Actions: 0 keep scene in tree, 1 remove scene from tree after the transition, 2 destroy scene after the transition.

### 7.2 Starting a transition

Starters: XuiScene_StartTransTo 0x921531f8 (kind 1, path +0x2c, action 0), XuiScene_StartTransFrom 0x92153250 (kind 2, path +0x28, action arg < 3 else E_INVALIDARG), XuiScene_StartTransBackTo 0x921531a0 (kind 3, +0x34, action 0), XuiScene_StartTransBackFrom 0x92153148 (kind 4, +0x30, action 2).

```
XuiScene_StartTransition(scene, name, kind, action):        // 0x92152730
    p = parent(scene); if !p: return E_INVALIDARG
    if p is XuiTransition: 0x9214db28(scene, &x); return E_FAIL
    S = scene instance
    msg 0x1d data {+0 kind, +4 out deferred=0, +8 in/out action}; XuiSendMessage(scene)
    if handled and data.deferred: S.flags |= 1; S.kind = kind; S.action = data.action
    obj = (name non-empty) ? XuiLookupLoadedObject(name, basePath(scene)) : 0
    if obj and obj.timelineCount < 1: obj = 0
    if !obj:
        if !(S.flags & 1): XuiScene_OnTransitionEnd(scene, kind, data.action); return 1
        return 0
    t = create "XuiTransition"; t.kind = kind (+8, 1..4); t.action = data.action (+4, < 3)
    if data.action != 2: XuiTransition_SaveSceneProps(t, scene, obj.timelines)   // pivot and animated property values
    if obj has properties: read Pivot/Width/Height (defaults 60, 40)
        sceneW,sceneH = size of scene (message via 0x9214f408)
        if visualW > 0.01: pivot.x = pivot.x / visualW * sceneW
        if visualH > 0.01: pivot.y = pivot.y / visualH * sceneH
        XuiElementSetPivot(scene, pivot)    // 0x9214da88
    insert t into p at scene position; unlink scene; add scene as child of t
    sub = subtimeline built from obj.timelines[0] with target = scene
    tl = new TL with sub; XuiElementSetTimeline(t, tl)
    XuiElementPlayTimeline(t, 0, 0, tl.length, loop=1, recurse=0)
    XuiTimeline_ApplyAtFrame(tl, 0)
    S.flags |= 2
```
Only timeline record 0 of the transition visual is used. Its property paths were resolved against the class of the placeholder child (in the skin: an XuiFigure `box`) and are applied to the scene by global property id, so properties absent from the scene class are silently not set. The TL plays with loop = 1. The transition ends on its 0xf.

Scene 0x1d handler XuiScene_OnStartTransition 0x921692e8:
```
(base, index) = kind 1: ("TransTo", +0x4c); 2: ("TransFrom", +0x48); 3: ("TransBackTo", +0x48); 4: ("TransBackFrom", +0x4c)
if index != 0: try names base + "%d"(index) and "End" + base + "%d"(index)    // format at 0x9200a884
if index == 0 or either indexed name missing: use base and "End" + base
if both found: bHandled = 1; if XuiElementPlayTimeline(scene, s, s, e, 1, 0) >= 0: data.deferred = 1
else: XuiElementPlayTimeline(scene, 0, 0, -1, 1, 0)        // bHandled not set
```
XuiTabScene handles 0x1d by setting bHandled only, so tab scenes never play own transition frames.

### 7.3 Ending a transition

Scene 0xf: if (S.flags & 1) and data.hObj == scene: S.flags &= ~1; if !(S.flags & 2): XuiScene_OnTransitionEnd(scene, S.kind, S.action).

XuiTransition_OnMessage 0x92165438, message 0xf with data.hObj == self (always sets bHandled):
```
p = parent(self); scene = first child(self); if !p or !scene: return
own = S.flags & 1
if own or self.action == 0:
    TLS flag 0x9214d2f8(1); unlink scene; insert scene into p before next sibling of self; 0x9214d2f8(0)
    XuiTransition_RestoreSceneProps(self, scene)
S.flags &= ~2
if !own:
    XuiTransition_RestoreSceneProps(self, scene)
    XuiScene_OnTransitionEnd(scene, self.kind, self.action)
XuiDestroyObject(self)
```

XuiScene_OnTransitionEnd 0x92151438 (scene, kind, action):
```
if action in (1, 2):
    msg 0x2f data {+0 i32 = -1}; XuiSendMessage(scene); if data+0 != 0: 0x921508b8(scene, data+0)
    unlink scene from parent (0x9214d478)
partner = S.+0x24 or XuiSceneGetBackScene(scene)
if partner is a scene with flags & 3: skip input re-enable
else:
    XuiElementSetInputEnabled(scene, 1)
    if S.+0x24: XuiElementSetInputEnabled(S.+0x24, 1); S.+0x24 = 0
    elif back scene: XuiElementSetInputEnabled(back, 1); back.+0x24 = 0
msg 0x1e data {+0 kind, +4 0, +8 action}; XuiSendMessage(scene)
if kind == 4: XuiSceneSetBackScene(scene, 0)
if data.action (after send) == 2: XuiDestroyObject(scene)
```
Message 0x1e handlers may change data +8. Status: complete for the core; 0x2f/0x921508b8 contents partial.

### 7.4 Interrupting

XuiScene_InterruptTransitions 0x92150668(scene): send 0x2a (no data) to S.+0x24 if it is a scene with flags & 3, else to the back scene if it has flags & 3; then to scene itself if its flags & 3. Called unconditionally at the start of forward and back navigation.

Scene 0x2a handler XuiScene_OnInterruptTransition 0x9215b618: if flags & 1: XuiTimeline_FinishNow(own TL). If flags & 2 and parent is XuiTransition: XuiTimeline_FinishNow(parent TL) then XuiElementStopTimeline(scene, 1). FinishNow applies the end frame (or length when end < 0) with XuiTimeline_ApplyAtFrame and bubbles 0xf, which runs the normal end path synchronously.

XuiSubTimeline_ApplyAtFrame 0x921551b8 differs from ApplyAt: integer frame; when loop is set and span = end - start > 0, frame = frame mod (span + 1) + start (signed divide), span <= 0 gives frame = start; the next keyframe index wraps to 0 when looping; properties with flag 0x20 apply only when frame equals the keyframe frame; no notifications.

### 7.5 Navigation

User argument: 0..3, 0xff, 0xfe (do not set focus), 0xfd (use the user of the focus owner of the source scene, else 0xff). Other values return E_INVALIDARG.

```
XuiSceneNavigateForward(cur, stayVisible, next, user):     // 0x92153590
    validate; resolve 0xfd
    msg 0x2b data {+0 next}; XuiSendMessage(cur); next = data+0      // handler may replace or clear
    if !next: return S_FALSE (1)
    if XuiSceneSetBackScene(next, cur) != 0: return E_FAIL            // previous back scene existed; link already overwritten
    XuiScene_InterruptTransitions(cur)
    if !parent(next):
        p = parent(cur); if !p: return 0x8030000b
        insert next into p before cur's next sibling (0x9214d718)
    send 0x29 to next tree (0x92147d40)
    if user != 0xfe: XuiElementInitFocus(next, user, 1)
    XuiElementSetInputEnabled(cur, 0); XuiElementSetInputEnabled(next, 0)
    S(cur).+0x24 = next
    XuiScene_StartTransFrom(cur, action = stayVisible ? 0 : 1)
    XuiScene_StartTransTo(next)
    return 0

XuiSceneNavigateBack(cur, target, user):                    // 0x92153780
    validate; resolve 0xfd
    walk b = back(cur), back(b)... until b == target; reach 0 -> E_FAIL
    XuiScene_InterruptTransitions(cur)
    if target != back(cur):
        x = XuiSceneSetBackScene(cur, target)                 // returns old back
        while x != target: y = XuiSceneSetBackScene(x, 0); XuiDestroyObject(x); x = y
    if !parent(target):
        p = parent(cur); if !p: return 0x8030000b
        insert target into p between cur's previous sibling and cur; send 0x29 to target tree
    0x921505c8(cur, target)        // if S(cur).flags & 4: bubble 0x18 query from target, 0x9214f518, clear bit
    msg 0x1a data {+0 cur}; XuiSendMessage(target)
    if user != 0xfe: XuiElementInitFocus(target, user, 1)
    XuiElementSetInputEnabled(cur, 0); XuiElementSetInputEnabled(target, 0)
    S(target).+0x24 = cur
    XuiScene_StartTransBackFrom(cur)      // action 2: cur destroyed at the end
    XuiScene_StartTransBackTo(target)
    return 0

XuiSceneNavigateFirst(parent, scene, user):                 // 0x92153450
    validate; resolve 0xfd from parent's focus owner
    if XuiSceneSetBackScene(scene, 0) != 0: return
    if !parent(scene): if !parent or add child fails: return
    send 0x29 to scene tree; if user != 0xfe: XuiElementInitFocus(scene, user, 1)
    XuiElementSetInputEnabled(scene, 0)
    XuiScene_StartTransTo(scene)
```
Input blocking: XuiElementSetInputEnabled 0x9214d388 sets element flag 0x4 on the whole subtree; XuiProcessKeystroke 0x92150170 consults XuiElementAcceptsInput 0x9214d3e0. Input returns only when both scenes of a navigation finished (section 7.3). Status: complete.

### 7.6 XuiNavButton feed

XuiNavButton_OnMessage 0x9213e800 instance: +4 PressPath, +8 bit0 StayVisible, +0xc SrcTransIndex, +0x10 DestTransIndex. 0x803 get {+4 path, +8 stay, +0xc src, +0x10 dst}, 0x804 set (same layout). Accessors: XuiNavButtonGetPressPath 0x9214d0c0, XuiNavButtonGetStayVisible 0x9214d0f8, XuiNavButtonGetTransIndex 0x9214d130.

Scene message 0xe data {+0 kind, +4 hObj source, +0xc ...}, kind 1 (press), when IgnorePresses (flags bit4) is clear:
```
if XuiIsBackButton(src): bHandled; XuiSceneNavigateBack(scene, XuiSceneGetBackScene(scene), 0xfd)
elif XuiIsNavButton(src): bHandled
    XuiSceneCreate(XuiElementGetBasePath(scene), PressPath, 0, &next)
    if XuiNavButtonGetTransIndex(src, &srcIdx, &dstIdx) ok:
        XuiSceneSetTransIndex(scene, 0, srcIdx); XuiSceneSetTransIndex(next, 1, dstIdx)
    XuiSceneNavigateForward(scene, StayVisible, next, 0xfd)
elif src is XuiList: item path from 0x921590a8, same create/index/navigate sequence (StayVisible from list data)
```
srcIdx selects TransFrom<n> on the current scene, dstIdx selects TransTo<n> on the new scene; the back path later uses +0x48 of the target (TransBackTo<n>) and +0x4c of the leaving scene (TransBackFrom<n>). Status: complete for the navigate side.

## 8. XuiTabScene frame selection

Instance (create 0x92141af0, 0x18 bytes): +0 hObj, +4 flags (bit0 Wrap, bit1 UserInterrupt, bit2 tabbing disabled via XuiTabSceneEnableTabbing 0x9214ec80, bit3 animating, bit4 VerticalTabs, bit5 NoAutoHide), +8 TabCount (default 1), +0xc current index 0-based (default -1), +0x10 current tab element, +0x14 DefaultTab 1-based (default 1). Tab elements are children with Id `Tab%u` (1-based).

```
Next():                                   // 0x9215fcd0
    if count < 2: return
    if cur + 1 < count: name = "%uTo%u"(cur + 1, cur + 2)
    elif Wrap and cur + 1 == count: name = "%uTo1"(count)
    else return
    s = FindNamedFrame(name); e = FindNamedFrame(name + "End")
    cur = (cur + 1 == count) ? 0 : cur + 1
    Activate(s, e, focus = 1)
Prev():                                   // 0x9215fde8
    if count < 2: return
    if cur != 0: name = "%uTo%u"(cur + 1, cur)
    elif Wrap: name = "1To%u"(count)
    else return
    s, e as above; cur = (cur == 0 ? count : cur) - 1; Activate(s, e, 1)
GotoTab(i, focus):                        // 0x9215fef0
    if i >= count: return E_INVALIDARG; if i == cur: return 0
    f = -1
    if count > 1: f = FindNamedFrame(i == 0 ? "2To1End" : "%uTo%uEnd"(i, i + 1))
    old = tabElement; cur = i; Activate(f, f, focus)
    if old and !NoAutoHide: hide old
Activate(s, e, focus):                    // 0x9215fb88
    if s != -1: flags |= 8; r = XuiElementPlayTimeline(self, s, s, e, loop=1, recurse=0); if r < 0: return
    if tabElement: if !NoAutoHide: hide it; send 0x7f1 to it
    tabElement = child "Tab%u"(cur + 1)
    if found: show; if focus: XuiElementInitFocus(tab, focus-owner user or 0xff, 1); send 0x7f0 to it
```
Missing named frames give -1. When the start frame is -1, Activate skips playback and only switches the tab elements (index already updated). When the start frame exists but the `End` frame is missing, Play runs with end = -1: the timeline plays to its length, never raises 0xf, and the animating flag (bit3) stays set, which blocks key input unless UserInterrupt is set. GotoTab plays a single frame (start = end = the `End` frame of the transition into tab i+1); that frame raises 0xf on the first tick and clears the animating flag.

Messages: 0xf with own hObj clears bit3. 0x13: send 0xbb8 (no data) to self; if unhandled, hide every `Tab%u` child except the current one unless NoAutoHide (XuiTabScene_HideOtherTabs 0x9215ffe0); then if cur >= count, GotoTab(DefaultTab in 1..count ? DefaultTab - 1 : 0, focus 0). 0x7f0 = tab activated, 0x7f1 = tab deactivated, both sent (not bubbled) to the tab element with no data. 0xbb8 lets a derived class suppress automatic hiding by setting bHandled. Key input (0x24) moves tabs only when bit2 is clear and (bit3 clear or UserInterrupt set). The key table is in the XuiTabScene section of xui-controls.md. Status: complete.

## Nuances

- Time is float milliseconds with 1 frame = 16.666666 ms. dt is wall clock and variable. The only fixed sub-stepping is at named frames: the step is cut to land exactly on the next named frame so its command runs before passing it.
- The first tick after a play drops dt entirely and applies values at the initial frame.
- A named frame at the initial frame runs its command on that first tick.
- 0xf is level-triggered: repeated every tick while held at a non-looping end, once per wrap when looping.
- The loop wrap of the TL sets pos to start after the end values were applied; subtimelines wrap independently and map end + 1 frame to start.
- Autoplay plays every timeline from 0 to its length with loop = 1 and no end notification; stops come only from named frame commands.
- Recursive play skips XuiVisual-derived children in the +0x18 list and skips an element's own TL when it was created with autoplay off (0x2000); stop has no such check.
- Jump commands replace end with the TL length and force loop = 1.
- XuiTimeline_Play with save = 0 leaves the saved end frame, so 0xf still fires at the end frame of the original play.
- Interpolation uses the first keyframe's type for the segment. Unsigned values do not interpolate. Integers truncate toward zero.
- Ease: only the y components of the control points are used; time is not reparameterized.
- Before the first keyframe nothing is applied and 0x14 is not sent for that subtimeline.
- Properties whose definition flags have bit 0x20 are applied only when the position is within 0.1 ms of a keyframe.
- Transition visuals animate the scene through property ids resolved against the placeholder child class.
- XuiSceneNavigateForward fails with E_FAIL when the next scene already had a back scene, after overwriting it.
- Scenes without TransX named frames play their whole timeline from 0 (loop, no end) as the in-scene transition.
- XuiTransition_RestoreSceneProps runs twice when action == 0 and no own transition is pending.

## Open questions

- InterruptTransitions (+0x3c): no reader found in XUI core code (searched all users of g_hXuiScene and XuiGetSceneInstance, and Scene impl callees to depth 2).
- Exact VMX128 math of XuiQuaternionSlerpV 0x9217e4d0 (threshold and fallback) and the rotation sign in 0x9217ef68.
- Saturation behavior of the color pack instruction 0x19a106d0.
- Contents of 0x2f data handling in 0x921508b8 and of Scene flags bits 2 and 3.

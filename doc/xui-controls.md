# XUI controls, scenes and message dispatch

Behavior of the XUI runtime statically linked in dash 2.0.6717: messages, input routing, focus, navigation, the built-in control classes, scene press handling, visuals and skins. Timelines, named-frame playback, scene transitions and scene navigation are in [xui-timelines.md](xui-timelines.md). Class registration and property tables are in [xui-classes.md](xui-classes.md). The XUR loader is in [xui-format.md](xui-format.md).

All addresses are in the retail dash image (load base 0x92000000). Functions named here are renamed in the Binary Ninja database. Status labels: complete (behavior fully read from the disassembly), partial (main path read, some branches or fields open), unknown.

Virtual key values are given as numbers. Which physical button produces each pad key code is up to the application.

Summary for implementers:

- Every class has one message handler. A message is offered to the most-derived class first and moves to the base class until one handler sets bHandled. Bubbling repeats this on each parent.
- Keystrokes go to the focused element of the keystroke's user and bubble up. Scenes forward unhandled keys to their UnfocusedInput children.
- Arrow keys move focus only through the explicit NavLeft/NavRight/NavUp/NavDown id paths. Tab walks the tree in child order. Lists move between their own items. There is no spatial search.
- A button press starts on key down, plays the Press frames of the visual, and notifies the parent when that animation ends. Scenes turn presses of XuiNavButton, XuiBackButton and list items with ItemsNavPath into navigation.
- A control's look is a skin visual instantiated as its first child and driven by named frames such as Normal, Focus, Press and their Disable/Check variants, with a fixed fallback table.
- Visuals are looked up by the Visual property, then by class name up the class chain, first with the blade-skin prefix and then without.
- Text and images are pulled by presenters inside the visual from the nearest control on every render.

Sections: Object model; Messages (id table, notify codes); Input; Focus; Navigation; Control visual state; XuiControl; Press; XuiButton, XuiNavButton, XuiBackButton; XuiScene; XuiCheckbox and XuiRadioButton; XuiRadioGroup; XuiTabScene; Timers; Visual and skin resolution; Lists, scrolling and range controls (XuiList, XuiCommonList, XuiListItem, XuiScrollEnd, XuiScrollBar, XuiSlider, XuiProgressBar); Text, edit, message box and sound classes (XuiText, XuiTextPresenter, XuiLabel, XuiCaret, XuiEdit, XuiMessageBox, XuiSound, XuiSoundXAudio).

## Object model

Status: complete for the parts listed.

- An element handle is a u32. The low 16 bits index a slot table (pages of 0x200 slots of 0xc bytes; page array 0x92809cf8, slot count 0x92809f18). The high 16 bits are a generation that must equal the slot's u16 at +0. Slot +2 bit 0 marks the slot valid, slot +4 points to the most-derived class-level node.
- Each element has one node per class level, most-derived first: node +0 handle passed to the class procedure, +4 next (base class) node, +0xc shared element record, +0x18 class handle, +0x1c class procedure, +0x20 instance data created by the class `CreateInstance` function.
- `0x9213d690(h, hClass)` returns the node for a class level or 0 (also the "is this element an instance of class X" test). `0x9214c728(h, hClass)` returns that level's instance data.
- `0x9213d748(h)` maps any handle of the element to the canonical element handle. Focus slots store canonical handles.
- Class procedures registered in `XUIClass` +0x10 are thunks `mr r3,r5; b impl` at 0x9213d958..0x9213da94, so every handler below is `impl(instance, XUIMessage*)`.

Per class handler and create function (instance size in parentheses):

| Class | Handler | Create | Instance fields used here |
| --- | --- | --- | --- |
| XuiElement | 0x92168ea0 | 0x92110610 | see below |
| XuiControl | `XuiControl_OnMessage` 0x9215aa68 | 0x92141408 (0x40) | see "XuiControl" |
| XuiButton | `XuiButton_OnMessage` 0x9215b8b0 | 0x921415d0 (0xc) | +4 PressKey (default 0x5840) |
| XuiNavButton | `XuiNavButton_OnMessage` 0x9213e800 | 0x92141650 (0x14) | +4 PressPath, +8 bit0 StayVisible, +0xc SrcTransIndex, +0x10 DestTransIndex |
| XuiBackButton | none (procedure 0x92378080 returns 0) | 0x92144900 | none |
| XuiCheckbox, XuiRadioButton | `XuiCheckbox_OnMessage` 0x92161940 via 0x92165428 | 0x92144960 (0xc) | +4 bit1 checked, +8 PressKey (default 0x5840) |
| XuiRadioGroup | `XuiRadioGroup_OnMessage` 0x9215bb78 | 0x921416e0 | +4 selected element, +8 next child index |
| XuiScene | `XuiScene_OnMessage` 0x92169720 | 0x9214afc0 | see "XuiScene" |
| XuiTabScene | `XuiTabScene_OnMessage` 0x92160100 | 0x92141af0 (0x18) | see "XuiTabScene" |
| XuiLabel | inline 0x9213d970 | 0x92144900 | none |

XuiElement instance fields used by this document: +0 handle, +0xc Width, +0x10 Height, +0x14 Opacity, +0x18 first child, +0x2c ..., +0xe4 timeline (see xui-timelines.md), +0xf0 packed flags. Flags at +0xf0: 0x1 Show, 0x2 transform dirty, 0x4 input blocked (`XuiElementAcceptsInput` 0x9214d3e0 returns `!(flags & 4)`), 0x40 DesignTime, 0x80 DisableTimelineRecursion, 0x1000 ClipChildren, 0x4000 EnableEffects, 0x8000 set on message 0x13 when the element has children, 0x10000 DisableFocusRecursion, 0x20000 transform parent override (message 0x36).

## Messages

Status: complete for the message structure and send functions; per-id status in the table.

`XUIMessage` (0x14 bytes): +0 cbSize (0x14), +4 id, +8 bHandled, +0xc cbData, +0x10 pvData. `XuiMessageInit` 0x92246350 zeroes it and sets size and id. `XuiMessageSetData` 0x921bf698 attaches a zeroed data block. Every id has a small builder that fills the data block; builders are named `XuiMessageInit*` in the database.

Builder addresses by id (status: observed): 0x0 0x92158638, 0x1 0x92158670, 0x3 0x921585e8, 0x4 0x92158548, 0x5 0x9214eeb8, 0x8 0x9213dd30, 0x9 0x922121e8, 0xa 0x92147c50, 0xb 0x9214ef40, 0xc 0x921cd290, 0xd 0x9214ef00, 0xe 0x921af9b8, 0x10 0x9213dda0, 0x13 0x9214f0f0, 0x18 0x921586e0, 0x19 0x9213de10, 0x1a 0x9214f190, 0x1d 0x9214f220, 0x1e 0x9214f260, 0x1f 0x9214f518, 0x20 0x92158598, 0x22 0x92158728, 0x23 0x9210cf88, 0x26 0x9214efd0, 0x27 0x92155e90, 0x28 0x9214f018, 0x29 several, 0x2a 0x92150668, 0x2b 0x9214f140, 0x2f 0x9214f068, 0x36 0x921586a8, 0x7d0 0x92158928, 0x7d2 0x92158960, 0x7d6 0x92158a20 and 0x92158a58, 0x7d9 0x92158ab0, 0x7db 0x92158b00, 0x7e4 0x92158b50, 0x7e5 0x92158be0, 0x7e6 0x92158c30. Further builders follow at 0x92158c70 to 0x92158cc0 (ids not recorded). `0x9214d4c0(h, &hParent)` returns the parent. Class procedure implementations not listed elsewhere: XuiEffect 0x92160778, XuiTextureEffect 0x92160878.

Send functions:

- `XuiSendMessage(h, msg)` 0x92144370. Clears bHandled. Walks the class-level nodes from most-derived to base. Per level it first calls a global hook if one is installed (`*0x92809c50`, called as `hook(nodeHandle, msg)`), then the level's procedure. It stops as soon as bHandled is set, except for id 0xa (destroy), which always reaches every level. The element is reference counted (+0x10 of the element record) for the duration; a destroy requested during dispatch is completed after the walk (`0x921440e8`).
- `XuiBubbleMessage(h, msg)` 0x92144500. `XuiSendMessage` to h, then to each parent in turn while bHandled is still 0.
- `0x92147d40(h, msg)` sends a message to an element and its whole subtree (used for 0x9, 0x1f, 0x28, 0x29).

Data for the notify message 0xe (`XuiMessageInitNotify` 0x921af9b8): +0 notify code, +4 source element, +8 extra size, +0xc pointer to extra data (attached by `XuiNotifySetExtra` 0x921b3930).

### Message ids

Directions: "self" = sent to the element, "bubble" = `XuiBubbleMessage`, "tree" = subtree broadcast.

| Id | Meaning | Data (offsets in pvData) | Sent by | Status |
| --- | --- | --- | --- | --- |
| 0x0 | Render | +0 render context | `0x92161218` to each shown child | partial (render path not covered here) |
| 0x1 | Render, second variant | same | `0x92161218` | partial |
| 0x3 | Hit test | 0xc bytes: +0 point pointer, +8 out bool | `0x9214f3a8` | partial |
| 0x4 | Child added | +0 child | parent, self, from `0x921643a8`/`0x92164530` | complete |
| 0x5 | Attach / init, recursive | +0 u32, +4 u32, +8 parent | `0x9214fb40`, `0x921598c8` (pre-order over subtree) | partial |
| 0x6 | Get size | +0 out width, +4 out height (floats) | `0x9214f408` | complete |
| 0x8 | Parent size changed | +0 w, +4 h, +8 w, +0xc h; both pairs hold the NEW size because the parent's size fields are updated before the child loop | `0x921452c0` to each child | complete |
| 0x9 | Create visual (controls) | none | Control on 0x0/0x1/0x29 without a visual, Visual property setter, skin change broadcast (tree) | complete |
| 0xa | Destroy | none | `XuiDestroyObject` 0x92147c50 | complete |
| 0xb | Query focusable | +0 out bool | `XuiElementQueryFocusable` 0x9214ff50 | complete |
| 0xc | Set focus | +0 previous focus, +4 u8 user | `XuiElementSetFocus` | complete |
| 0xd | Kill focus | +0 new focus, +4 u8 user | `XuiElementSetFocus` | complete |
| 0xe | Notify | see notify codes | bubble from the source's parent | complete |
| 0xf | Timeline end | +0 timeline owner | see xui-timelines.md | complete |
| 0x10 | Size changing | +0 old w, +4 old h, +8 new w, +0xc new h | `0x921452c0` to self | complete |
| 0x11 | Get Show | +0 out bool | `XuiElementIsShown` 0x9226b048 | complete |
| 0x12 | Set Show | +0 bool | `XuiElementSetShow` 0x9221f848 | complete |
| 0x13 | Init complete (post-order, after 0x5 of all children) | +0 u32 | `0x9214fb40` | partial |
| 0x14 | Timeline start | +0 timeline owner | see xui-timelines.md | complete |
| 0x18 | Get base path for relative paths (bubbled) | +0 out string | `XuiElementGetBasePath` 0x921ec178; answered by XuiScene (+0x18), XuiCanvas (+4), XuiVisual (source path of its skin) | complete |
| 0x19 | Set base path | +0 string | `0x9214f350` | complete |
| 0x1a | Navigated back to | +0 scene that was left | `XuiSceneNavigateBack` to the target | partial |
| 0x1b | Get back scene | +0 out element | `XuiSceneGetBackScene` 0x92206620 | partial |
| 0x1c | Set back scene | +0 new, +4 out old | `0x922604a0` | partial |
| 0x1d | Start scene transition | +0 kind (1 TransTo, 2 TransFrom, 3 TransBackTo, 4 TransBackFrom), +4 out deferred, +8 in/out action (0 keep, 1 remove, 2 destroy) | `XuiScene_StartTransition` 0x92152730 to the scene | complete |
| 0x1e | Scene transition ended | +0 kind, +4 0, +8 in/out action | `XuiScene_OnTransitionEnd` 0x92151438 to the scene | complete |
| 0x1f | Scene transition control (sets scene flag 0x4) | none | `0x9214f518`, tree | partial |
| 0x20 | Child removed | +0 child | `0x92164190` to parent | partial |
| 0x21 | Is my visual | +0 element, +4 out bool | Control/Image answer | partial |
| 0x22 | Get skin prefix (bubbled) | +0 out string | `0x9214f2a0`; answered by XuiVisual (+4, always), XuiCanvas (+8, always), XuiScene (+0x1c, only when set) | complete |
| 0x23 | Set skin prefix | +0 string | `0x9214f2f8` | complete |
| 0x24 | Key down | +0 u32 virtual key, +4 u16 unicode, +8 u32 key flags, +0xc u8 user | `XuiProcessKeystroke`, bubble from focus | complete |
| 0x25 | Key up | same as 0x24 | same | complete |
| 0x26 | Character | +0 u16 unicode, +4 u32 key flags, +8 u8 user | same, key down only | complete |
| 0x27 | Timer | +0 timer id | `XuiProcessTimers` 0x92155ee0 | complete |
| 0x28 | Release resources | +0 mask: 0x1 textures, 0x2 sounds, 0x4 fonts, 0x10 destroy control visuals | `0x921508b8` tree | complete |
| 0x29 | (Re)create resources; scene becoming active | none | navigate functions, tree | complete |
| 0x2a | Interrupt transition | none | `XuiScene_InterruptTransitions` 0x92150668 | complete |
| 0x2b | Navigate forward query | +0 next scene (handler may replace) | `XuiSceneNavigateForward` to current scene | partial |
| 0x2c | Edit: get line start | +0 line, +4 out index | XuiEdit | complete |
| 0x2d | Edit: insert text | +0 index, +4 string | XuiEdit | complete |
| 0x2e | Edit: delete text | +0 index, +4 count | XuiEdit | complete |
| 0x2f | Before unlinking a scene after a transition | +0 i32 in -1, out resource flags | `XuiScene_OnTransitionEnd` | partial |
| 0x36 | Get transform parent override | +0 out element | `0x92159988` to self when flag 0x20000 | partial |
| 0x7d0 | Get value | +0 out u32 | `XuiControlGetValue` 0x92736c90 | complete |
| 0x7d1 | Set value | +0 u32 | `XuiControlSetValue` 0x92158fa8 | complete |
| 0x7d2 | Get enabled | +0 out bool | `XuiControlIsEnabled` 0x922215d8 | complete |
| 0x7d3 | Set enabled | +0 bool | `0x92238b78`, Enabled setter 0x9215b508 | complete |
| 0x7d4 | Get selected element (radio group) | +0 out element | | complete |
| 0x7d5 | Set selected element (radio group) | +0 element | `0x9221f890` | complete |
| 0x7d6 | Update visual state | +0 explicit flag, +4 focused | `XuiControlUpdateState` 0x92158f20 ({0,0}), `XuiControlUpdateStateFocus` 0x92158f60 ({1,f}) | complete |
| 0x7d7 | Get current item | +0 out index, +4 out element | `XuiControlGetCurItem` 0x921b39b8 | complete |
| 0x7d8 | Set current item by index | +0 index | `0x92158df8` | complete |
| 0x7d9 | Get child control by index | +0 index, +4 out element | `XuiControlGetChildByIndex` 0x921e56f8 | complete |
| 0x7da | Get top item | +0 out index | `XuiListGetTopItem` 0x921bbb50 | complete |
| 0x7db | Set top item | +0 in/out index (clamped value written back) | `XuiListSetTopItem` 0x921cbf10 | complete |
| 0x7dc | Get count | +0 kind (0 items, 1 visible, 2 checked, 3 item controls, 4 lines, 5 per line), +4 out | `0x921cda20` | complete |
| 0x7dd | Insert items | +0 index, +4 count | `XuiListInsertItems` 0x92218aa0 | complete |
| 0x7de | Delete items | +0 index, +4 count | `XuiListDeleteItems` 0x921da840 | complete |
| 0x7df | Get source text | 0x28 bytes: +0 item, +4 DataAssociation, +8 item flag, +0xc out text, +0x10 in/out PointSize (-1.0 none), +0x14 extended request (text presenter sets 1), +0x18 out float, +0x1c in/out TextStyle, +0x20 out run count, +0x24 out runs | `0x921b5508` | complete for the binding fields |
| 0x7e0 | Set source text | 0x28 bytes: +0 item, +4 DataAssociation, +8 item flag, +0xc text | `0x9224a278` | complete |
| 0x7e1 | Get source image | 0x1c bytes: +0 item, +4 DataAssociation, +8 item flag, +0xc out force reload, +0x10 out path, +0x14 out texture object, +0x18 out element | `0x921a5be0` | complete |
| 0x7e2 | Set source image | same layout, inputs | `0x922aa120` | complete |
| 0x7e3 | Get item check | +0 index, +4 out | `XuiListGetItemCheck` 0x92736cd8 | complete |
| 0x7e4 | Set item check | +0 index, +4 value | `XuiListSetItemCheck` 0x92158e90 | complete |
| 0x7e5 | Press | +0 out accepted, +4 u8 user, +8 from-input flag | `XuiButton_OnKeyDown` | complete |
| 0x7e6 | Get navigation target | +0 direction, +4 source, +8 out target, +0xc out "target is source" | `XuiControl_OnMessage` key down | complete |
| 0x7e7 | ScrollEnd set state | +0 0 normal, 1 ScrollMore, 2 Scrolling | `XuiScrollEndSetState` 0x92159178 | complete |
| 0x7e8 | Get item enabled | +0 index, +4 out | `XuiListGetItemEnable` 0x92736d20 | complete |
| 0x7e9 | Set item enabled | +0 index, +4 value | `XuiListSetItemEnable` 0x92250a80 | complete |
| 0x7ea | Edit: get caret | +0 out position | XuiEdit | complete |
| 0x7eb | Edit: set caret | +0 position | XuiEdit | complete |
| 0x7ec | Message box result | +0 button index | XuiMessageBox, sent (not bubbled) to the owner | complete |
| 0x7ed | Edit: get top line | +0 out line, +4 bool current | `0x92272de0` | complete |
| 0x7ee | Edit: scroll to line | +0 line, +4 bool animate | `0x92272da0` | complete |
| 0x7ef | Edit: line info | +0 mode (0/1/2), +4 out, +8 bool current | `0x92272e30` | complete |
| 0x7f0 | Tab activated | none | `XuiTabScene_Activate` to the new tab | complete |
| 0x7f1 | Tab deactivated | none | `XuiTabScene_Activate` to the old tab | complete |
| 0x7f2 / 0x7f3 | Edit: get / set smooth scroll | +0 must be 0, +4 bool enable, +8 f32 start lines/s, +0xc f32 acceleration, +0x10 f32 max lines/s | `0x92272e70` (0x7f3) | complete |
| 0x7f4 | Sound: play | none | `0x9216f840` | complete |
| 0x7f5 | Sound: stop | none | `0x9216f888` | complete |
| 0x7f6 | Sound: query done | +0 out u32 (bit0) | `0x9216f748` | complete |
| 0x7f7 | Get pressed | +0 out bool | `XuiControlIsPressed` 0x9220eaf0 | complete |
| 0x7f8 | Set pressed | +0 bool | `XuiControlSetPressed` 0x92158ed8 | complete |
| 0x7f9 | Sound: get volume | +0 out f32 dB | `0x9216f780` | complete |
| 0x7fa | Sound: set volume | +0 f32 dB (valid -96..6) | `0x9213de58` | complete |
| 0x7fb | Get text with version | 0x14 bytes: +0 out version, +4 item, +8 DataAssociation, +0xc item flag, +0x10 out text | `0x9219adc0` (XuiHtmlPresenter 0x9219bfa8) | complete |
| 0x7fc | Sound: get mute | +0 out u32 | `0x9216f808` | complete |
| 0x7fd | Sound: set mute | +0 u32 | `0x9216f7b8` | complete |
| 0x800 | Find tab stop | +0 kind, +4 source, +8 out target, +0xc out "target is source" | Control key down (Tab) | complete |
| 0x803 | Get navigation data | 0x14 bytes: +0 item index, +4 out PressPath, +8 out StayVisible, +0xc out SrcTransIndex, +0x10 out DestTransIndex | `XuiControlGetNavData` 0x921590a8 | complete |
| 0x804 | Set navigation data | same layout, inputs | `0x92159118` | complete |
| 0x805 | XuiSoundXAudio: nonzero stops, zero re-applies State | +0 u32 | no sender found | complete (handler) |
| 0x806 | Get DisableFocusRecursion | +0 out bool | `XuiElementIsFocusDisabled` 0x9214f478 | complete |
| 0x807 | Set DisableFocusRecursion | +0 bool | `0x921455a0` | partial |
| 0x808 | ScrollBar set range | +0 count, +4 top, +8 visible | `XuiScrollBarSetRange` 0x921591c0 | complete |
| 0x809 | HTML text: string lookup | 8 bytes: +0 key string, +4 out | `0x9219ae08` from the HTML text code (0x9219af28); no handler in the XUI library | partial |
| 0x80a | HTML: resolve image path (bubbled) | 0x10 bytes: +0 path, +4 resolved path, +8/+0xc out | `0x921962e8` from 0x92196eb0; no handler in the XUI library | partial |
| 0x80d | Size changed | none | `0x921452c0` to self after 0x10 and 0x8 | complete |
| 0xbb8 | Tab scene init query | none; if unhandled the tab scene hides inactive tabs | `XuiTabScene_OnMessage` on 0x13 | complete |

Ids not listed were not found in the XUI library.

### Notify codes (message 0xe)

| Code | Meaning | Extra data | Sent by | Status |
| --- | --- | --- | --- | --- |
| 1 | Press | +0 u8 user | `XuiControlNotifyPress` 0x9215a758, bubbled from the control's parent | complete |
| 2 | Set focus | +0 previous focus | Control/Scene/RadioGroup/TabScene on 0xc | complete |
| 3 | Kill focus | +0 new focus | Control on 0xd | complete |
| 4 | Selection changed | +0 new index, +4 old index | `XuiNotifySelChanged` 0x92158e40, bubbled from the element itself | complete |
| 5 | ScrollEnd state request (after its Scrolling animation) | none | XuiScrollEnd | complete |
| 6 | Item check changed | +0 index, +4 checked | `0x921588e0` (list) | complete |
| 7 | Pressing (cancelable) | +0 repeat flag, +4 out cancel, +8 u8 user | `XuiControlNotifyPressing` 0x9215a6e0 | complete |
| 0xa | Value changing (handler may replace new) | +0 old, +4 new | `0x92158d10` (slider) | complete |
| 0xb | Value changed | +0 value | slider | complete |
| 0xe | ScrollBar scroll | +0 top | scrollbar | complete |

## Input

Status: complete.

The application polls one keystroke per frame and passes it to `XuiProcessKeystroke` 0x92150120. The keystroke record is: +0 u16 virtual key, +2 u16 unicode, +4 u16 flags, +6 u8 user index.

```
XuiProcessKeystroke(ks):
    if ks == null: return E_INVALIDARG
    target = focus_slot(ks.user) or focus_slot(0xff)
    if target == 0 or !XuiElementAcceptsInput(target): return S_OK      # dropped, reported as handled
    if ks.flags & 0x1:   up = false
    elif ks.flags & 0x2: up = true
    else: return E_UNEXPECTED
    mflags = ((ks.flags >> 2) & 0xf) | (((ks.flags >> 15) & 1) << 4)
    handled = false
    if ks.vk != 0:
        bubble(target, key message (0x24 if !up else 0x25) {vk, unicode, mflags, user})
        handled = msg.bHandled
    if !up and ks.unicode != 0:
        bubble(target, 0x26 {unicode, mflags, user})
        handled |= msg.bHandled
    return handled ? S_OK : S_FALSE
```

Message key flag bits (mflags): bit0 repeat (keystroke flag 0x4), bit1 keystroke flag 0x8 (used as "shift" for Tab), bit2 keystroke 0x10, bit3 keystroke 0x20 (suppresses navigation, see below), bit4 keystroke 0x8000.

The global at 0x92809cc8 bit 0, when set, disables the keyboard keys Enter, Space and Escape as press keys and the keyboard arrow, Page and Tab keys as navigation keys.

### Key to navigation direction

`XuiKeyToNavDirection` 0x9214c920(keydata, forceKeyboard). Returns 0 when key flag bit3 is set.

| Result | Keys |
| --- | --- |
| 1 left | 0x5812, 0x5823; keyboard 0x25 |
| 2 right | 0x5813, 0x5822; keyboard 0x27 |
| 3 up | 0x5810, 0x5820; keyboard 0x26 |
| 4 down | 0x5811, 0x5821; keyboard 0x28 |
| 5 page up | 0x5806; keyboard 0x21 |
| 6 page down | 0x5807; keyboard 0x22 |
| 7 tab | keyboard 0x09 |

Keyboard entries apply only when 0x92809cc8 bit0 is clear or forceKeyboard is nonzero.

### Unfocused input

A scene forwards key down and key up messages that bubbled up to it unhandled to each control in its unfocused-input list, in list order, until one handles it (`XuiScene_OnMessage` ids 0x24/0x25). A control is added to the list when the scene receives 0x4 (child added) for it and the control's UnfocusedInput flag (Control +0x10 bit1) is set. Only direct children of the scene are registered this way. 0x20 (child removed) removes it. The dash skin uses this for the legend buttons (for example `XuiBackButton` with `UnfocusedInput=true`, `PressKey=22593`).

## Focus

Status: complete.

There are five focus slots: one per user (0x9280b274 + 4*user) and one shared slot (0x9280b270, user 0xff). A flag at 0x9279ac28 selects single-focus mode (shared slot in use). A parallel set of "pressed control" slots lives at 0x9280b22c + 4*user and 0x9280b228 (`XuiGetPressSlot` 0x9214c7d8).

```
XuiElementSetFocus(h, user):                                   0x921515a0
    if user not in 0..3 and user != 0xff: return E_INVALIDARG
    e = h ? canonical(h) : 0
    if e and user != 0xff:
        for i in 0..3: if per_user[i] == e and i != user: return E_INVALIDARG
    if e:
        if user < 4:
            single_focus = 0; pressed_shared = 0
            if shared_slot: XuiElementSetFocus(0, 0xff)
        else:
            single_focus = 1
            for i in 0..3: pressed[i] = 0; if per_user[i]: XuiElementSetFocus(0, i)
        if send(e, 0xb).out == false: return 0x80300014
    old = slot(user)
    if old: send(old, 0xd {new = e, user})          # sent even when old == e
    slot(user) = e
    if e:
        send(e, 0xc {previous = old, user})
        if !handled: slot(user) = 0; return E_FAIL
    return S_OK
```

Only classes that handle 0xc can hold focus: XuiControl and its subclasses, XuiScene, XuiTabScene, XuiRadioGroup and XuiGroup. A plain XuiElement never handles 0xc, so focusing it clears the slot.

XuiGroup (`0x9215cc68`) is a focus-forwarding container. On 0xb it sends 0x800 kind 2 (first tab stop) to itself and answers true only if a descendant other than itself was found. On 0xc it bubbles notify 2 and then focuses that first tab stop for the same user. On 0x10 it resizes itself like a scene.

Query focusable (0xb) on XuiControl: `IsShown(self) && !XuiElementIsFocusDisabled(self, recursive=1)`. The recursive check walks the element and every ancestor and sends 0x806; any ancestor with DisableFocusRecursion blocks focus. XuiLabel answers 0xb with false. XuiCaret answers 0xb too (see edit section).

Initial focus, `XuiElementInitFocus(h, user, initial)` 0x92152ca0:

```
    if initial: init_focus_depth += 1                 # 0x9280b23c
    hr = send(h, 0xb)
    if hr failed: result = hr
    elif msg.out: result = XuiElementSetFocus(h, user)
    elif msg.bHandled: result = E_FAIL                # element answered "not focusable": its children are not searched
    else:                                             # 0xb unhandled (plain element, canvas): search children
        result = E_FAIL
        for child in children(h) in order:
            if XuiElementInitFocus(child, user, initial) == S_OK: result = hr; break
    if initial: init_focus_depth -= 1
    return result
```

`XuiIsInitialFocusActive` 0x9214ca48 is `init_focus_depth > 0`. It selects the InitFocus frames instead of Focus frames (see "Control visual state").

Control on 0xc: handled; `XuiControlUpdateStateFocus(self, 1)`; bubble notify 2 {previous} from the parent. Control on 0xd: handled; `XuiControlUpdateStateFocus(self, 0)`; bubble notify 3 {new} from the parent.

Control on 0x12 (set Show, 0x9215ae8c): the message is first passed to the base class level explicitly; then if the control is now hidden and the control itself is in a focus slot (`0x9214cac8`: the shared slot, then users 0..3, compared with the canonical handle; descendants are not checked), focus moves to the control's parent for that slot's user (`XuiElementGetFocusUser` 0x9214cb40). Hiding an ancestor of the focused element leaves focus where it is. The Show property setter 0x92145698 checks for a Bool value (else E_INVALIDARG) and calls `XuiElementSetShow`, so timeline Show keyframes send 0x12 too.

Helper queries: `XuiElementHasFocus` 0x9214ffd0 (any user's focus is the element or a descendant), `XuiElementGetFocusUser` 0x9214cb40, `XuiElementIsDescendant` 0x9214dc98.

XuiElement on 0x807 (set DisableFocusRecursion, Element +0xf0 bit 0x10000), always handled. When the flag becomes set, focus is pushed out of the element:

```
while (owner = XuiElementGetFocusOwner(self)) != 0:          # focus inside self for some user
    user = XuiElementGetFocusUser(owner)
    m = 0x800 {kind 0 (next), source self}; send(self, m)     # next tab stop after self
    target = (m.bHandled and !m.targetIsSource) ? m.target : 0
    XuiElementSetFocus(target, user)                           # 0 clears the slot
```

Status: partial (loop structure from HLIL; the per-iteration user selection was not traced instruction by instruction).

## Navigation

Status: complete. There is no automatic spatial navigation in the XUI library. Arrow keys move focus only through the explicit Nav properties, through handlers of 0x7e6 above the control (lists handle their own keys), or through the application.

XuiControl key down (0x24), reached when no subclass handled the key:

```
if !XuiElementHasFocus(self): return (unhandled)
d = XuiKeyToNavDirection(key, 0)
dir = {1:2 (left), 2:3 (right), 3:0 (up), 4:1 (down), 7: 4 | (key.flags>>1 & 1)}[d]  else return
msg = 0x7e6 {dir, self}
send(self, msg)
if msg.bHandled:
    key.handled = 1
    if !msg.targetIsSource: XuiElementSetFocus(msg.target, focus user of self, or 0xff)
elif d == 7:                                  # Tab
    key.handled = 1
    msg2 = 0x800 {kind = key.flags>>1 & 1 (0 next, 1 previous), self}
    send(self, msg2)
    if msg2.bHandled and !msg2.targetIsSource: XuiElementSetFocus(msg2.target, user)
```

Directions in 0x7e6: 0 up, 1 down, 2 left, 3 right, 4 tab forward, 5 tab backward. Page up and page down (5, 6) are not mapped by XuiControl.

XuiControl on 0x7e6 when the source is this control (Control level of `msg.source` == self):

```
parent = Parent(self)
if parent: send(parent, msg)                  # parent may answer first
if !msg.bHandled:
    t = XuiControlFindExplicitNav(self, self, dir, checkEnabled=0, checkShown=1)
    msg.target = t
    if t: msg.bHandled = 1; msg.targetIsSource = (t == msg.source)
```

`XuiControlFindExplicitNav(origin, cur, dir, checkEnabled, checkShown)` 0x9214fdb8:

```
c = Control instance of cur; if none: return 0
if c.flags & 0x10: return 0                   # cycle guard (Control +0x10 bit4); two discarded id lookups
c.flags |= 0x10
path = {0: NavUp, 1: NavDown, 2: NavLeft, 3: NavRight, 4: NavTabForward, 5: NavTabBackward}[dir]   # dir > 5: 0
result = 0
if path non-empty and Parent(cur):
    t = XuiElementFindByPath(Parent(cur), path)
    if t == origin:
        result = t                            # leads back to the origin: "target is source"
    elif t:
        if (checkShown and !IsShown(t)) or (checkEnabled and !IsEnabled(t)) or XuiElementIsFocusDisabled(t, 0):
            result = XuiControlFindExplicitNav(origin, t, dir, ...)    # skip t, continue along t's own Nav property
        else:
            result = t
c.flags &= ~0x10
return result
```

When the result is the origin itself, the 0x7e6 handler marks the message handled with "target is source", so the key is consumed without a focus change. For Tab this also blocks the tab-order fallback (0x800). A Nav property that names the control itself, or a skip chain that leads back to it, therefore swallows the key.

`XuiElementFindByPath(start, path)` 0x92172390 splits the path on `\`; each component `..` moves to the parent, any other component is a child id lookup (`0x9214db88`). Nav ids are therefore relative to the control's parent and may contain `..\`.

Disabled controls are valid navigation targets (checkEnabled is 0); they show the FocusDisable frames.

XuiElement on 0x800 (find tab stop): `t = XuiElementFindTabStop(self, kind, checkEnabled=0, checkShown=1)`; if t: handled, target = t, targetIsSource = (t == source). Kinds (0x92151c70):

- 0 next: walk next siblings of the current element; for each candidate that is shown, not focus-disabled, and whose own 0x800 kind 2 (first descendant) answer or itself is a focusable control, take it. When the siblings are exhausted, move to the parent and continue with the parent's next siblings; at the root, wrap via kind 2 from the root. (0x92151a18)
- 1 previous: mirror image with previous siblings and kind 3. (0x92151b40)
- 2 first descendant: children in order. (0x92151828)
- 3 last descendant: children in reverse order. (0x92151920)

For every candidate the walker sends 0x800 with kind 2 (or 3) to the candidate first, so a container can redirect to its own first or last tab stop; if that is unanswered or answers "target is source", the candidate itself is used when `XuiElementIsFocusableControl` 0x921517c0 (0xb true and the element is a XuiControl).

XuiVisual answers every 0x800 with handled and "target is source", so tab order never enters a control's visual.

NavTabForward and NavTabBackward properties, when set, take precedence over this order because 0x7e6 (dir 4/5) is tried first.

## Control visual state

Status: complete.

A control's look is driven by named frames in its visual (`XuiControlGetVisual` 0x92150240: the control's first child, accepted only if it is a XuiVisual; visual creation always inserts it as the first child). State changes send 0x7d6 to the control.

XuiControl on 0x7d6 calls `XuiControl_PlayFocusStateFrames` 0x921579b0:

```
if no visual: return
focused = data.explicit ? data.focused : XuiElementHasFocus(self)
enabled = Control.flags & 1
if !focused:
    if !enabled:          frames = (NormalDisable, EndNormalDisable)
    elif !data.explicit:  frames = (Normal, EndNormal)
    else:                 frames = (KillFocus, EndKillFocus)
else:
    if data.explicit and XuiIsInitialFocusActive():
        frames = enabled ? (InitFocus, EndInitFocus) : (InitFocusDisable, EndInitFocusDisable)
    else:
        frames = enabled ? (Focus, EndFocus) : (FocusDisable, EndFocusDisable)
XuiControlPlayStateFrames(self, frames)
```

`XuiControlPlayStateFrames(h, start, end, altStart, altEnd)` 0x92150448:

```
if h, start or end is null: return E_INVALIDARG
loop:
    if play(visual, start .. end) succeeds: return S_OK      # 0x921502f0, needs both named frames
    if altStart and altEnd: start, end = altStart, altEnd; altStart = altEnd = 0; continue
    find i with wcscmp(table[i].start, start) == 0 and wcscmp(table[i].end, end) == 0
    if found: return XuiControlPlayFallbackFrames(h, table[i].fallback)
    return 0x80300026
XuiControlPlayFallbackFrames(h, i): while i >= 0: if play(table[i]) succeeds: return S_OK; i = table[i].fallback
                                    return 0x80300026
```

Every caller in this binary passes null alternates, so the alternate pair is never used.

`play` finds both named frames on the visual and calls the timeline play function (`0x9214e070(visual, start, start, end, 1, 1)`); see xui-timelines.md for the meaning of the last two arguments.

Fallback table at 0x9200b810, 16-byte entries {u32 index, start, end, i32 fallback}:

| Index | Start | End | Fallback |
| --- | --- | --- | --- |
| 0 | Normal | EndNormal | none |
| 1 | Focus | EndFocus | 0 |
| 2 | KillFocus | EndNormal | 0 |
| 3 | NormalDisable | EndNormalDisable | 0 |
| 4 | FocusDisable | EndFocusDisable | 1 |
| 5 | Press | EndPress | 1 |
| 6 | NormalPress | EndNormalPress | 5 |
| 7 | NormalCheck | EndNormalCheck | 0 |
| 8 | FocusCheck | EndFocusCheck | 7 |
| 9 | NormalCheckDisable | EndNormalCheckDisable | 7 |
| 10 | FocusCheckDisable | EndFocusCheckDisable | 8 |
| 11 | NormalSel | EndNormalSel | 0 |
| 12 | NormalSelCheck | EndNormalSelCheck | 11 |
| 13 | NormalSelDisable | EndNormalSelDisable | 11 |
| 14 | NormalSelCheckDisable | EndNormalSelCheckDisable | 13 |
| 15 | ScrollMore | EndScrollMore | 0 |
| 16 | Scrolling | EndScrolling | 15 |
| 17 | ScrollMoreDisable | EndScrollMoreDisable | 15 |
| 18 | InitFocus | EndInitFocus | 1 |
| 19 | InitFocusDisable | EndInitFocusDisable | 4 |
| 20 | InitFocusCheck | EndInitFocusCheck | 8 |
| 21 | InitFocusCheckDisable | EndInitFocusCheckDisable | 10 |
| 22 | PressDisable | EndPressDisable | 6 |
| 23 | KillFocus | EndKillFocus | 2 |

The (start, end) pair must match a table entry exactly for fallback to apply. For example a visual without KillFocus frames falls back to KillFocus/EndNormal, then Normal/EndNormal.

0x7d6 is not sent while a scene is being loaded (TLS flag tested by `0x9214d350`).

## XuiControl

Status: partial (messages listed are complete; 0x10, 0x21, 0x28 and 0x29 visual details are in the visual section).

Instance (0x40 bytes, create 0x92141408): +0 handle, +4 ClassOverride, +8 Visual, +0xc index (initial -1; child index assigned by the parent scene, radio group or tab container, or the data index of a list item control; `XuiControlGetIndex` 0x9214cdf8, set by 0x9214ce38), +0x10 flags (initial 1), +0x14 NavLeft, +0x18 NavRight, +0x1c NavUp, +0x20 NavDown, +0x24 NavTabForward, +0x28 NavTabBackward, +0x2c Text, +0x30 PointSize (initial from 0x92005a24), +0x34 ImagePath, +0x38 u8 user of the press in progress (initial 0xff), +0x3c text version counter.

Flags at +0x10: 0x1 Enabled, 0x2 UnfocusedInput, 0x4 visual created, 0x8 pressed, 0x10 navigation cycle guard.

| Id | Behavior |
| --- | --- |
| 0x0, 0x1 | If the visual is not created yet (flag 0x4 clear) send 0x9 to self, then fall through to the base. |
| 0x9 | Handled. If not loading and creating the visual (0x9214ccf0) succeeds, send 0x7d6 {0,0}. |
| 0xa | Destroy the visual (`0x92157958`), clear flag 0x4. |
| 0xb | Handled. out = shown and not focus-disabled (recursive). |
| 0xc / 0xd | Focus in / out, see "Focus". |
| 0xf | Timeline end: finish a press, see "Press". |
| 0x12 | Show change; move focus to the parent when the hidden control itself holds focus. |
| 0x24 | Navigation, see "Navigation". |
| 0x28 | If data & 0x10, destroy the visual (it is recreated on the next 0x0/0x1/0x29). |
| 0x29 | If no visual yet, send 0x9 to self. |
| 0x7d2 | out = flags & 1. |
| 0x7d3 | Set or clear flag 1; unless loading, send 0x7d6 {0,0}. Focus is not moved. |
| 0x7d6 | `XuiControl_PlayFocusStateFrames`. |
| 0x7d9 | Find the child control whose child index equals data+0; data+4 = child or 0. |
| 0x7df | If data+0 == 0 and data+4 == 0: data+0xc = Text, data+0x10 = PointSize. |
| 0x7e0 | If data+0 == 0 and data+4 == 0: if the new text (data+0xc) differs from Text (wcscmp; both null counts as equal), increment the version counter (skipping 0) and copy it. Handled when equal or copied. |
| 0x7e1 | If data+0 == 0 and data+4 == 0: data+0x10 = ImagePath. |
| 0x7e2 | If data+0 == 0 and data+4 == 0: copy data+0x10 into ImagePath. |
| 0x7e5 | Press, see "Press". |
| 0x7e6 | Navigation target, see "Navigation". |
| 0x7f7 / 0x7f8 | Get / set flag 0x8 (pressed). |
| 0x7fb | If item == 0 and DataAssociation == 0: data+0 = version, data+0x10 = Text. |

## Press

Status: complete.

```
XuiButton key down (0x24), XuiButton_OnKeyDown 0x9215b6c8; same code for XuiCheckbox/XuiRadioButton:
    if *XuiGetPressSlot(key.user) != 0: key.handled = 1; return        # another press in progress for this user
    match = key matches PressKey (below)
    if XuiControlIsPressed(self):
        key.handled = 1                                                 # all keys swallowed while pressed
        if match: XuiControlNotifyPressing(self, key.user, repeat=1)
    elif match:
        key.handled = 1
        if !(key.flags & 1):                                            # ignore auto-repeat
            send(self, 0x7e5 {accepted=0, user=key.user, fromInput=1})

PressKey match:
    0x5840: vk in {0x5800, 0x5814}, or vk in {0x0d, 0x20} and 0x92809cc8 bit0 clear
    0x5841: vk in {0x5801, 0x5815}, or vk == 0x1b and 0x92809cc8 bit0 clear
    other:  vk == PressKey
```

XuiControl on 0x7e5 (`XuiControl_OnPress` 0x9215a7e0), always handled:

```
if !enabled:
    if visual has both PressDisable and EndPressDisable:
        data.accepted = 1
        if data.fromInput: XuiSetPressSlot(self, 1, data.user)
        XuiControlSetPressed(self, 1); Control.pressUser = data.user
        XuiControlUpdateState(self)                     # plays PressDisable via the class 0x7d6 handler
    return
if XuiControlIsPressed(self):
    XuiControlNotifyPressing(self, data.user, 1); return
if XuiControlNotifyPressing(self, data.user, 0) returns cancel: return
data.accepted = 1
if data.fromInput: XuiSetPressSlot(self, 1, data.user)
XuiControlSetPressed(self, 1)
XuiControlSetValue(self, !XuiControlGetValue(self))  # toggles checkboxes; no effect on classes without 0x7d0/0x7d1
if visual exists:
    Control.pressUser = data.user
    XuiControlUpdateState(self)                         # Button/Checkbox 0x7d6 handlers play the press frames
else:
    XuiControlNotifyPress(self, data.user)             # immediate notify 1
    XuiControlSetPressed(self, 0); XuiSetPressSlot(self, 0, data.user)
```

`XuiControlNotifyPressing(self, user, repeat)` 0x9215a6e0 bubbles notify 7 {repeat, cancel=0, user} from the control's parent and returns the cancel field. No class in the XUI library sets it; application scenes can.

Completion, XuiControl on 0xf (`XuiControl_OnTimelineEnd` 0x9215a980). It does not check which timeline ended; any timeline end that reaches the control while it is pressed completes the press (the visual's end message bubbles through the control):

```
if !XuiControlIsPressed(self): return
XuiControlSetPressed(self, 0)
XuiSetPressSlot(self, 0, Control.pressUser)
if enabled: XuiControlNotifyPress(self, Control.pressUser)      # notify 1 bubbled from the parent
XuiControlUpdateState(self)                                      # back to Focus/Normal frames
v = visual; tl = timeline of v (0x9214dfc8)
if tl: XuiTimeline_Play(tl, tl.start, tl.end, tl.end, tl.loop, tl.savedEnd != -1)     # 0x921545d0
```

The final call re-seeks the state animation that `XuiControlUpdateState` just started to its end frame, so after a press the control snaps to the final pose of its Focus (or Normal) frames instead of animating into it. See xui-timelines.md for the timeline fields.

The press notify is therefore delivered when the Press animation reaches its end frame, not on key down or key up. Disabled controls with PressDisable frames animate but never notify.

XuiButton on 0x7d6 (`XuiButton_PlayStateFrames` 0x9215b7c8), handled:

```
if no visual: return
focused = data.explicit ? data.focused : XuiElementHasFocus(self)
if XuiControlIsPressed(self):
    frames = !enabled ? (PressDisable, EndPressDisable) : !focused ? (NormalPress, EndNormalPress) : (Press, EndPress)
    XuiControlPlayStateFrames(self, frames)
else: XuiControl_PlayFocusStateFrames(self, data)
```

## XuiButton, XuiNavButton, XuiBackButton

Status: complete.

- XuiButton handles only 0x24 (above) and 0x7d6 (above). Everything else goes to XuiControl.
- XuiNavButton handles 0x803 (copy PressPath, StayVisible, SrcTransIndex, DestTransIndex into data +4..+0x10) and 0x804 (set them; PressPath is copied). It does not navigate itself. The containing scene acts on its press notify (next section).
- XuiBackButton has a procedure that returns 0 without handling anything. It behaves exactly like XuiButton; the scene recognizes it by class.

## XuiScene

Status: partial. Press routing, focus and unfocused input are complete. Transitions and the navigate functions are in xui-timelines.md.

Instance fields used here: +0 handle, +4 DefaultFocus, +8 last focused descendant, +0xc unfocused-input control array, +0x10 its count, +0x18 base path (0x18/0x19), +0x1c string (0x22/0x23), +0x20 back scene (0x1b/0x1c), +0x24 transition partner (see xui-timelines.md), +0x28 TransFrom, +0x2c TransTo, +0x30 TransBackFrom, +0x34 TransBackTo, +0x38 flags (0x1 own transition running, 0x2 XuiTransition wrapper running, 0x4 and 0x8 see xui-timelines.md, 0x10 IgnorePresses, 0x20 being destroyed), +0x3c InterruptTransitions (no reader found), +0x40/+0x44 pending transition kind/action, +0x48/+0x4c source/destination transition index, +0x50 next child index.

Press routing, notify 1 received by the scene (bubbled from a child):

```
if scene.flags & 0x10 (IgnorePresses): return (not handled, keeps bubbling)
src = notify.source
if XuiIsNavButton(src) or XuiIsBackButton(src):
    handled
    if XuiIsBackButton(src):
        XuiSceneNavigateBack(self, XuiSceneGetBackScene(self), 0xfd)
    else:
        if XuiSceneCreate(XuiElementGetBasePath(self), PressPath(src), 0, &next) succeeded:
            if XuiNavButtonGetTransIndex(src, &s, &d) succeeded:
                set transition index (self, 0, s); set transition index (next, 1, d)      # 0x9214e4f0
            XuiSceneNavigateForward(self, StayVisible(src), next, 0xfd)
elif src is a XuiList:
    item = XuiControlGetCurItem(src)
    send(src, 0x803 {item}) -> path, stay, s, d          # list ItemsNavPath
    if path:
        handled; create scene as above; set indexes; XuiSceneNavigateForward(self, stay, next, 0xfd)
```

User 0xfd means "use the user that currently has focus inside the scene". The press notify keeps bubbling past the scene when not handled (other button kinds), so application scene classes and their ancestors see every press.

Focus handling:

- 0xc (scene receives focus): handled; bubble notify 2 from the parent; if the remembered descendant (+8) is still valid and inside the scene, focus it; if that fails or none is remembered: if DefaultFocus is set, resolve it with `XuiElementFindByPath(scene, DefaultFocus)` and focus it (a DefaultFocus that is not found or not focusable does not fall back to the children); otherwise run `XuiElementInitFocus(child, user, XuiIsInitialFocusActive())` over the children in order until one returns S_OK. If nothing took focus, `XuiControlUpdateStateFocus(scene, 1)`. The scene does not start an initial-focus period of its own; InitFocus frames appear only when a navigate function or tab switch started one.
- Notify 2 from inside: if the focused element is a descendant, remember it in +8. If the previous focus was outside the scene, play the scene's focus frames (`0x9214cd88(scene, 1)`). If the remembered child changed, bubble notify 4 {new index, old index} from the scene.
- Notify 3 from inside: if the new focus is outside the scene, play the scene's unfocused frames (`0x9214cd88(scene, 0)`).
- 0x7d7: data = {index of remembered child or -1, remembered child or 0}. 0x7d8: focus the child with the given index for the focused user and bubble notify 4.

Child bookkeeping: 0x4 (child added) assigns the child control's index (+0xc) from the scene counter +0x50; if the new child is not the last child (its next-sibling link, Element +0x20, is set) all children are renumbered instead (`XuiControlRenumberChildren` 0x92157b30, indexes 0.. in child order). Unfocused-input controls are registered at the same time. 0x20 renumbers and unregisters. 0xa sets flag 0x20.

Timer: the scene uses timer id 0xaabbccdd (see xui-timelines.md). On 0x27 with that id it kills the timer and, if the scene does not accept input (`!XuiElementAcceptsInput`), calls `0x9214d388(scene, 1)`.

## XuiCheckbox and XuiRadioButton

Status: complete.

Both classes use `XuiCheckbox_OnMessage` 0x92161940 with the instance {+0 handle, +4 flags (bit1 checked), +8 PressKey}.

| Id | Behavior |
| --- | --- |
| 0x24 | Same as XuiButton key down with this PressKey. |
| 0x7d0 | out = checked. |
| 0x7d1 | If pressed: set or clear checked without replaying frames. Otherwise, if the value changes, set it and send 0x7d6 {0,0}. |
| 0x7d6 | `XuiCheckbox_PlayStateFrames` 0x9215b950. |

`XuiCheckbox_PlayStateFrames`: if not checked, use `XuiControl_PlayFocusStateFrames`. If checked: unfocused -> NormalCheck (Disable variant when disabled); focused -> FocusCheck, or InitFocusCheck when explicit and initial focus is active (Disable variants when disabled). Checkboxes have no pressed frames of their own; during a press they show the check state frames.

The value is toggled at the start of the press (in XuiControl on 0x7e5) and the press notify follows at the end of the animation. A radio button toggles the same way; a pressed radio button that was checked becomes unchecked until its group re-checks it.

## XuiRadioGroup

Status: partial (0x0-0x3 not read; they reach the base).

Instance: +0 handle, +4 selected element, +8 next child index.

| Id | Behavior |
| --- | --- |
| 0x4 | If the child is a checkbox or radio button: assign its child index. |
| 0x20 | If the removed child is a checkbox or radio button: renumber the remaining children and set its index to -1. |
| 0xc | Handled. Bubble notify 2. If a selected element exists, focus it; else `XuiElementInitFocus` over the children in order. |
| 0xe code 1 | From a child: handled; if the child is the selected element clear the selection first; `XuiRadioGroup_Select(child)`; then bubble notify 1 for the group itself (user from the child's notify). |
| 0xe code 2 | If the source is a descendant and the previous focus was outside the group: play the group's focus frames. |
| 0xe code 3 | If the new focus is outside the group: play the group's unfocused frames. |
| 0x7d4 | out = selected element. |
| 0x7d5 | `XuiRadioGroup_Select(data+0)`. |
| 0x7d7 | out = {index of selected, selected}. |
| 0x7d8 | `XuiRadioGroup_Select(child with index data+0)`. |

`XuiRadioGroup_Select(e)` 0x9215bac8: ignored unless e is 0 or a descendant. If e differs from the current selection: set value 0 on the old selection, value 1 on e, store e, and bubble notify 4 {new index, old index} from the group (index -1 for none).

## XuiTabScene

Status: complete.

Instance (0x18 bytes, create 0x92141af0): +0 handle, +4 flags, +8 TabCount (default 1), +0xc current tab (0-based, initial -1), +0x10 current tab element, +0x14 DefaultTab (1-based, default 1). Flags: 0x1 Wrap, 0x2 UserInterrupt, 0x4 tabbing disabled (set through `XuiTabSceneEnableTabbing` 0x9214ec80), 0x8 tab transition playing, 0x10 VerticalTabs, 0x20 NoAutoHide.

Tab elements are children with ids `Tab1` .. `TabN` (format `Tab%u` with current+1).

Initialization on 0x13: send 0xbb8 to self; if unhandled and NoAutoHide is clear, hide every `Tab%u` child except the current one. Then, if the current tab is still unset (>= TabCount), go to tab `DefaultTab - 1` (DefaultTab 0 or > TabCount selects tab 0) without setting focus.

Key down (0x24); the key is consumed only when a switch is possible:

| VerticalTabs | Next tab | Previous tab |
| --- | --- | --- |
| clear | direction right, or vk 0x5804 | direction left, or vk 0x5805 |
| set | direction down, or vk 0x5807 | direction up, or vk 0x5806 |

```
step(next):
    possible = next ? (cur + 1 < TabCount or Wrap) : (cur != 0 or Wrap)
    if !possible: return (unhandled)
    key.handled = 1
    if (flags & 4) == 0 and ((flags & 8) == 0 or (flags & 2)): next ? Next() : Prev()
```

`XuiTabScene_Next` 0x9215fcd0 (needs TabCount >= 2): frame name `"%uTo%u"(cur+1, cur+2)`, or at the last tab with Wrap `"%uTo1"(TabCount)`; end frame is the same name + `End`. Current becomes (cur+1) mod TabCount. `XuiTabScene_Prev` 0x9215fde8: `"%uTo%u"(cur+1, cur)`, or at tab 0 with Wrap `"1To%u"(TabCount)`; current becomes cur-1 (wrapping to TabCount-1). Both then call `Activate(start, end, focus=1)`.

`XuiTabScene_GotoTab(i, focus)` 0x9215fef0: returns E_INVALIDARG if i >= TabCount and does nothing if i == current. If TabCount > 1 the frame is `"2To1End"` for i == 0, otherwise `"%uTo%uEnd"(i, i+1)`; it plays from that frame to that same frame (a snap to the final pose). With TabCount 1 no timeline is played. Then `Activate`; if NoAutoHide is clear the previously current tab element is hidden.

`XuiTabScene_Activate(start, end, focus)` 0x9215fb88:

```
if start != -1:
    flags |= 8
    if play timeline (self, start, start, end, 1, 0) fails: return error
old = current tab element
if old: if !NoAutoHide: SetShow(old, 0); send(old, 0x7f1)
new = child with id Tab%u(cur+1); current tab element = new
if new:
    SetShow(new, 1)
    if focus: XuiElementInitFocus(new, focus user of the tab scene or 0xff, initial=1)
    send(new, 0x7f0)
```

Flag 0x8 is set by `Activate` before it plays and stays set if playback fails. It is cleared when a 0xf (timeline end) whose owner is the tab scene itself reaches it. While it is set, further switches are ignored unless UserInterrupt is set. With TabCount 1 and Wrap set, the keys are consumed but `Next`/`Prev` return without switching.

0xc (focus): if a current tab element exists: handled, bubble notify 2, focus the tab element for the same user. 0x1d: handled (suppresses the XuiScene transition behavior for tab scenes).

Public entry points that call these: 0x9214ece8 (next), 0x9214ed28 (previous), 0x9214ed68 (go to tab).

## Timers

Status: complete.

`XuiSetTimer(h, id, periodMs)` 0x92155d48 adds or resets a node {+0 handle, +4 id, +8 period, +0xc last fire tick, +0x10 flags, +0x14 next} in the list at 0x9280b358 (critical section 0x9280b33c). Resetting an existing (h, id) clears its killed flag and restarts its period. `XuiKillTimer(h, id)` 0x92155e00 sets the killed flag (E_INVALIDARG when the timer does not exist). `XuiProcessTimers` 0x92155ee0, called once per frame by the application, holds the critical section for the whole walk, frees killed nodes, and for each live node with `now - last >= period` (unsigned) sets `last = now` and sends 0x27 {id} to the element; the next pointer is read before the send. The tick source is `0x925900a8` (millisecond tick). Missed periods are not accumulated: at most one 0x27 per timer per frame, and the next period counts from the frame that fired.

The handle is the one the class passes, normally its own class-level handle (instance +0, see "Object model"). The 0x27 send to that handle starts at that class level, so derived levels never see a base class's timer. CXuiEditAutoScroll depends on this: it handles every timer id on its level, while XuiEdit's smooth scroll timer 0xaabbccdd is set on the XuiEdit level handle and still reaches XuiEdit. exui-runtime keeps a `LevelHandle` (element plus class level, `None` for the most-derived level) per timer.

After the timers it calls `0x9216b4b0` and `0x9216f5f0` (timeline and sound updates, see xui-timelines.md and the sound section).

## Visual and skin resolution

Every statement was read from disassembly unless marked [INFERENCE]. Message data offsets are offsets into `XUIMessage.pvData`. "Canonical handle" is the most-derived class-level handle (`0x9213d748`).

Related notes: XUR loader and in-memory object records in [xui-format.md](xui-format.md); timelines in [xui-timelines.md](xui-timelines.md). Skins are loaded by the application.

### 1. Skin registry

Status: complete (string table application: partial).

A skin is a XUR file whose root object holds one top-level object per visual. Each top-level object is registered under its `Id` property, optionally prefixed by a skin id prefix. Registration keeps the loaded (not instantiated) object records; elements are created on demand per control.

#### Globals

| Address | Meaning |
| --- | --- |
| 0x9280b250 | Visual map (XUI string map): +0 key array (0xc-byte entries, +0 key string, +8 length), +4 value array (8-byte entries `{node, root}`), +8 count. `node` is the loaded top-level object record, `root` the loaded root record of the same file. |
| 0x9280b1d0 | Critical section for the visual map. |
| 0x9280b240 | Skin source map: prefix string -> pointer to a 0x10-byte source record. Same string map layout, 4-byte values. |
| 0x9280b260 | Source record of the unprefixed (base) skin, inline. |
| 0x9280b1ec | Critical section for source records (recursive; taken again by nested calls). |
| 0x9280b208 | Critical section held while a visual is instantiated. |
| 0x92809cd0 | Current locale string (`XuiSetLocale` 0x9213d848). Skin string tables load only when it is set. |

Source record (0x10 bytes): +0 skin file path (owned copy), +4 capacity in UTF-16 units, +8 length, +0xc string table object loaded from the same path with the extension replaced by `xus` (0 when no locale is set or loading failed).

Loaded object record fields used here (full layout in xui-format.md): +0 class name, +4 property entry array `{count, stride, entries}`, +8 child array, +0xc and +0x20 timeline / named-frame data, +0x18 reference count (atomic, `0x926f4e58` add, `0x92170400` release), +0x24 flags: bit0 instantiation in progress (visual recursion guard), bit2 not separately allocated [INFERENCE from the release path], bit3 set on a skin root by `XuiLoadVisualFromBinary`.

#### Entry points

| Address | Name | Role | Status |
| --- | --- | --- | --- |
| 0x92154050 | `XuiLoadVisualFromBinary(path, prefix)` | Load a skin and register its visuals. | complete |
| 0x92153f00 | `XuiFreeVisuals(prefix)` | Unregister visuals of a prefix, or all. | complete |
| 0x921542b0 | `XuiReloadVisuals()` | Locale change: free all, reload every registered skin. | complete |
| 0x92153c88 | `XuiSkin_SetSource(path, prefix)` | Create, replace or remove a source record. | complete |
| 0x92150ee8 | `XuiSkin_LoadStringTableForPath(path)` | `<path>.xus` string table when a locale is set. | partial |
| 0x92140b60 | `XuiPathReplaceExtWithXus` | Copy path, overwrite last 3 chars with `xus` (length > 3). | complete |
| 0x92150e68 | `XuiSkin_GetStringTable(prefix)` | Record +0xc. | complete |
| 0x92151f80 | `XuiSkin_GetSourcePath(prefix)` | Record +0 (skin file path). NULL prefix: base record. | complete |
| 0x92153e38 | `XuiSkin_RegisterVisual(key, node, root)` | Insert into the visual map. | complete |
| 0x92153a60 | `XuiSkin_UnregisterVisual(key)` | Remove one key, release node and root. | complete |
| 0x92150f70 | `XuiLookupLoadedObject(name, prefix, &node, &root, &usedPrefix)` | Scoped lookup. | complete |
| 0x92152368 | `XuiSkin_LookupVisual(name, prefix, &node, &root)` | Same without `usedPrefix`. Used by `XuiShowMessageBoxEx` and `XuiList_InitFromVisual`. | complete |
| 0x92153058 | `XuiSkin_CreateVisual(name, prefix, &h, autoplayDisabled)` | Lookup and instantiate. | complete |
| 0x92153140 | `XuiSkin_CreateVisualNoFlag` | `XuiSkin_CreateVisual(..., 0)`. No callers. | complete |
| 0x92171ba0 | `XuiSkin_ApplyStringTable(table, node, prefix)` | Localize a registered visual tree. | partial |

#### XuiLoadVisualFromBinary (0x92154050)

```
XuiLoadVisualFromBinary(path, prefix):
    hr = XuiLoadBinaryScene(path, &root)          // 0x9216f180
    if hr < 0: return hr
    addref(root)
    hr = XuiSkin_SetSource(path, prefix)
    if hr < 0: release(root); return hr
    for each child c of root: addref(c)
    root.flags |= 8
    for each child c of root, in file order:
        id = first property entry of c whose definition name is "Id" (wcscmp) and type is String (5)
        if id == null: continue                   // objects without Id are not registered
        key = prefix ? prefix || id : id          // plain concatenation, no separator
        hr = XuiSkin_RegisterVisual(key, c, root)
        table = XuiSkin_GetStringTable(prefix)
        if table: XuiSkin_ApplyStringTable(table, c, prefix)   // runs even when registration failed
        free key if allocated
        if hr < 0: release(root); return hr       // E_FAIL on duplicate key; earlier keys stay registered
    release(root)
    return 0
```

Allocation failure while building the key returns 0x8007000E.

#### XuiSkin_RegisterVisual (0x92153e38)

```
lock 0x9280b1d0
if map_find(visual_map, key) >= 0: unlock; return 0x80004005
addref(node); if root: addref(root)
if !map_insert(visual_map, key, {node, root}): release both; unlock; return 0x8007000E
unlock; return 0
```

First registration of a key wins; a second skin with the same key (same prefix) fails to load at that object.

#### XuiSkin_SetSource (0x92153c88)

```
table = XuiSkin_LoadStringTableForPath(path)      // computed before locking; 0 if no locale
lock 0x9280b1ec
if prefix == NULL:
    rec = 0x9280b260
    set rec path = path (copy; NULL clears)       // 0x92150cf0
    replace rec+0xc with table (old table destroyed and freed)
else if prefix not in source map:
    if path == NULL: unlock; return 0
    rec = alloc 0x10 (zeroed); set path; rec+0xc = table; insert source_map[prefix] = rec
else:
    rec = source_map[prefix]
    if path == NULL: destroy rec (path, table), free, remove prefix from map
    else: set path; replace rec+0xc with table
unlock
```

`XuiSkin_LoadStringTableForPath(NULL)` returns 0, so removal never computes a table.

#### XuiFreeVisuals (0x92153f00)

```
lock 0x9280b1d0
if prefix == NULL:
    for every entry: release(node); if root: release(root)
    clear visual map                               // 0x92173720; source records are kept
else:
    n = wcslen(prefix)
    collect every key with length != 0 and wcsncmp(prefix, key, n) == 0
    for each collected key: XuiSkin_UnregisterVisual(key)
    XuiSkin_SetSource(NULL, prefix)                // removes the prefix source record
unlock
```

#### XuiReloadVisuals (0x921542b0)

Called from `XuiSetLocale` (0x9213d848) when the locale string changes and argument 2 bit0 is clear.

```
XuiFreeVisuals(NULL)
lock 0x9280b1ec
if base record length != 0:
    if XuiLoadVisualFromBinary(copy of base path, NULL) < 0: stop
for i in 0 .. source_map.count-1:
    rec = source_map.values[i]
    if rec.length != 0:
        if XuiLoadVisualFromBinary(copy of rec.path, source_map.keys[i]) < 0: stop
unlock
```

Existing visual elements are not touched. They pick up reloaded definitions only when recreated (message 0x9 or 0x28 bit4, see section 4).

#### Lookup and scope order: XuiLookupLoadedObject (0x92150f70)

```
XuiLookupLoadedObject(name, prefix, &node, &root, &usedPrefix):
    if any out pointer is NULL: return 0x80070057
    node = root = usedPrefix = 0
    key = prefix ? prefix || name : name
    lock 0x9280b1d0
    loop:
        i = map_find(visual_map, key)              // wcscmp, linear, case-sensitive
        if key != name:                             // prefixed attempt
            free key
            if i >= 0: usedPrefix = 1; break
            key = name; continue                    // retry unprefixed
        break
    if i < 0: unlock; return 0x80300026
    node, root = visual_map.values[i]
    unlock; return 0
```

Scope order: `prefix || name` first, then `name`. There is exactly one prefix in scope per lookup; there is no chain of skins beyond prefixed then unprefixed.

#### Application-level calls

| Caller | Call |
| --- | --- |
| `DashApp_LoadSkinAndRootScene` 0x9210f018 | `XuiLoadVisualFromBinary("section://<mod>,dashuisk#skin.xur", NULL)` |
| `DashApp_ApplyProfileStyle` 0x9210dd70 | `XuiFreeVisuals("dashSkin")`, then for style N != 0 `XuiLoadVisualFromBinary("section://<mod>,dashskn<N>#BladeSkin.xur", "dashSkin")`; then `XuiElementSetSkinPrefix(root scene, "dashSkin" or NULL)` (0x9214f2f8, message 0x23) and `XuiBroadcastRecreateVisuals(root scene)` (0x922121e8, message 0x9 to the whole tree). |
| disc playback module 0x922a7750 (call 0x922a783c) | `XuiLoadVisualFromBinary("section://%X,dashuisk#skin.xur", NULL)`. If the base skin is already registered this fails at the first object with 0x80004005 and the module only leaves its "loaded" flag (+0x30) unset. |
| `XuiReloadVisuals` | see above |
| `XuiUninit` 0x9214a608 (0x9214a960) | `XuiFreeVisuals` |

Keys in practice: base skin registers `XuiButton`, `XuiScene`, ... (296 top-level XuiVisual objects in `dashuisk/skin.xui`, no duplicates). Blade skins register `dashSkinXuiScene`, `dashSkinwing`, ... (33 in `dashskn1`, 31 in `dashskn2`).

#### Skin string tables

Status: partial. When a locale is set, `XuiSkin_SetSource` loads `<skin path minus 3 chars>xus` (`XuiPathReplaceExtWithXus` 0x92140b60, `0x92150ab8`). `XuiSkin_ApplyStringTable` (0x92171ba0) walks the registered object tree (`XuiStringTable_ApplyToNodeRecursive` 0x92171a70, `XuiStringTable_ApplyNodeProps` 0x92171970) and builds keys `prefix || Id1 "." Id2 "." ... PropertyName` (format strings `"%s."` 0x9200bf58 and `"%s"` 0x9200bf50; a node without `Id` uses its class name). For every property definition of every class level of the node class it looks the key up (`0x92170c90`) and on a hit replaces the loaded property value (`0x921710f0`). This edits the loaded records, so every element instantiated later gets the localized value.

### 2. Skin prefix and base path propagation

Status: complete.

| Id | Meaning | Data | Sender |
| --- | --- | --- | --- |
| 0x22 | Get skin prefix | +0 out wchar* (init 0) | `XuiElementGetSkinPrefix` 0x9214f2a0: bubble from the element; returns 0 on failure |
| 0x23 | Set skin prefix | +0 wchar* (copied; NULL or empty clears) | `XuiElementSetSkinPrefix` 0x9214f2f8: send to the element |
| 0x18 | Get base path | +0 out wchar* | `XuiElementGetBasePath` 0x921ec178: bubble |
| 0x19 | Set base path | +0 wchar* | `XuiElementSetBasePath` 0x9214f350 |

Handlers:

| Class | 0x22 | 0x23 | 0x18 | 0x19 |
| --- | --- | --- | --- | --- |
| XuiScene 0x92169720 | if +0x1c != 0: out = +0x1c, handled; else unhandled (keeps bubbling) | +0x1c = copy | if +0x18 != 0: out, handled | +0x18 = copy |
| XuiCanvas 0x9215ce58 | out = +8, always handled | +8 = copy | out = +4, always handled | +4 = copy |
| XuiVisual 0x9215bf68 | out = +4, always handled | +4 = copy | out = `XuiSkin_GetSourcePath(XuiElementGetSkinPrefix(self))`, always handled | not handled |

Consequences:
- A control finds its prefix from the nearest scene with a prefix, or from the nearest visual or canvas (which answer even when empty).
- `XuiSkin_CreateVisual` sends 0x23 to a new visual only when the prefixed key matched. A visual found by the unprefixed fallback has a NULL prefix and therefore hides the scene prefix from every control inside it.
- Relative resource paths inside a visual resolve against the path of the skin file the visual's prefix maps to (base record for NULL).

### 3. ClassOverride

Status: complete.

`XuiControl.ClassOverride` (global property id cached at 0x9279ac2c, initially -1, found by `0x9213d228(g_hXuiControl, "ClassOverride")`) is applied like any string property (stored at Control +4). `XuiApplyLoadedProperty` (0x9214f7f8) additionally copies the entry's string pointer (`XuiPropEntryGetString`, not a copy of the characters) into the caller's out slot when the entry is a plain (non-compound, non-custom) property, the set succeeded and its global id equals the cached id. The last such entry wins.

`XuiInstantiateObjectTree` (0x921523f8), after all property entries of the object are applied and before children are created:

```
if overrideName:
    cls = XuiFindClass(overrideName)
    if cls == 0 and g_XuiResolver != 0:
        if resolver.vtbl[1](resolver, overrideName, &cls) < 0: cls = 0
    cur = XuiElementGetClass(h)                   // 0x9213cf80
    if cls != 0 and cls != cur:
        if !XuiClassIsDerivedFrom(cls, cur):      // 0x92140bf0: equal or cur in cls's base chain (+0x28)
            XuiElementGetId(h)                    // result unused
            destroy h; return 0x80300016          // whole load fails
        hr = XuiElementApplyClassOverride(overrideName, h, &hNew)   // 0x92147680
        if hr < 0: destroy h; return hr
        h = hNew
create children and attach them to h; build timelines
```

`XuiElementApplyClassOverride` calls `0x92147398(name, 0, 0, existing top node, &node)`: it creates class-level nodes from the override class down to the first level whose class matches the existing chain, links the existing nodes below them, points every existing node's +0xc at the new top node and runs CreateInstance only for the new levels. The result is a new canonical handle; the old handle stays valid as a base-level handle. Property values already loaded stay in the base levels; the new derived levels start from their CreateInstance state (no XUR properties are applied to them).

In the dashboard the resolver's method 1 is `0x9272b668` (E_NOTIMPL), so a ClassOverride naming an unregistered class is silently ignored (unlike an unknown element class, which fails the load).

### 4. Visual creation (XuiControl)

Status: complete.

#### Instance fields

- XuiControl (0x40 bytes): +0 handle, +4 ClassOverride, +8 Visual (wchar*), +0x10 flags (bit 0x4 = visual created), +0x2c Text, +0x30 PointSize (initial -1.0), +0x34 ImagePath, +0x3c text version.
- XuiVisual (8 bytes, `XuiVisual_CreateInstance` 0x92141510): +0 handle, +4 skin prefix (wchar*, owned).
- Element level (shared): +4 Id, +8 element count / index from instantiation, +0xc width, +0x10 height, +0x18 first child, +0x1c last child, +0x20 next sibling, +0x24 previous sibling, +0x28 parent, +0xf0 flags (bit 0x2000 timeline autoplay disabled).

#### Triggers (message 0x9 to the control)

| Source | Condition |
| --- | --- |
| Render 0x0 / 0x1 reaching the XuiControl level | Control flag 0x4 clear. Sent synchronously before the element level renders children, so the new visual draws in the same frame. |
| 0x29 (scene becoming active, also `XuiSceneCreateInternal` flags bit1) | Control flag 0x4 clear. |
| `XuiControl_Visual_Set` 0x9215b458 (Visual property setter) | Always (after storing the string; NULL/empty clears it). Type must be String (else 0x80070057). |
| `XuiBroadcastRecreateVisuals` 0x922121e8 | Broadcast to a subtree (`XuiBroadcastMessage` 0x92147d40: element first, then children in order, pre-order; the next sibling is read before recursing; the first child is read after the parent's handler ran). |

XuiControl on 0x9:

```
msg.handled = 1
if !XuiIsLoading() and XuiControlCreateVisual(self) >= 0:    // 0x9214ccf0 -> 0x9215b2b0
    send 0x7d6 {0, 0} to self                                 // play state frames on the new visual
```

`XuiIsLoading` (0x9214d350) tests the per-thread load depth (TLS slot index stored at 0x9279ac30), raised by `XuiCreateFromLoadedObject` and by `XuiLoadDepthAdjust` (0x9214d2f8). Visual property values applied while a XUR is being instantiated therefore never create visuals; creation is deferred to the first render, 0x29 or an explicit 0x9.

Classes that cache visual parts (XuiScrollBar, XuiProgressBar, XuiSlider, XuiEdit, XuiList, XuiMessageBox, XuiHtmlControl) handle 0x9 themselves: they clear their cached part handles, forward the same message to their base level (`XuiObjectGetBaseLevel` 0x9213ced8 then `XuiSendMessage`) so that XuiControl creates the visual, set handled, then look up parts (section 5).

#### XuiControl_CreateVisual (0x9215b2b0)

```
XuiControl_CreateVisual(ctl):
    XuiControl_DestroyVisual(ctl)                         // 0x92157958
    prefix = XuiElementGetSkinPrefix(ctl.h)               // bubbled 0x22
    noAutoplay = XuiElementIsTimelineAutoplayDisabled(ctl.h)   // 0x9214e1b8, element +0xf0 & 0x2000
    hVis = 0
    if ctl.Visual and ctl.Visual[0] != 0:
        hr = XuiSkin_CreateVisual(ctl.Visual, prefix, &hVis, noAutoplay)
        if hr == 0x80300025: return hr
    if hVis == 0:
        if ctl.Visual non-empty: XuiElementGetId(ctl.h)   // result unused
        lvl = canonical handle of ctl.h
        while lvl != 0:
            hr = XuiGetClassInfo(XuiElementGetClass(lvl), &info)    // 0x9213cda0; info+4 = class name
            if hr < 0: return hr
            hr = XuiSkin_CreateVisual(info.name, prefix, &hVis, noAutoplay)
            if hr == 0x80300025: return hr
            if hVis: break
            lvl = XuiObjectGetBaseLevel(lvl)
        if hVis == 0:
            XuiElementGetId(ctl.h)                        // result unused
            return 0x80300026
    first = XuiElementGetFirstChild(ctl.h)                // 0x9214d538
    XuiLoadDepthAdjust(1)
    hr = XuiElementInsertChild(ctl.h, hVis, after=0, before=first)   // 0x9214d718 -> 0x92164530
    XuiLoadDepthAdjust(0)
    if hr < 0: return hr                                  // hVis is not destroyed
    ctl.flags |= 4
    XuiInitElementTree(hVis, 0)                           // 0x92150c98
    return its result
```

Notes on the steps:
- Any failure of the name step other than 0x80300025 (not found, instantiation error) falls back to the class-name walk.
- Class walk order: most-derived class first, e.g. a `XuiNavButton` without Visual tries `XuiNavButton`, `XuiButton`, `XuiControl`, `XuiElement`. Each name is looked up with the same prefix scope as section 1.
- The visual becomes the first child, so it renders before (beneath) the control's own children.
- `XuiElementInsertChild` sends 0x4 (child added) to the control. Outside loading it would also send 0x5 to the new subtree (`0x921598c8`); the raised load depth suppresses that so that `XuiInitElementTree` performs the only init.
- `XuiInitElementTree(h, initData)` (0x92150c98 -> 0x9214fb40): per element, pre-order, send 0x5 `{+0 1, +4 0, +8 parent}`; a result < 0 other than 0x80300026 stops; recurse into children (they get initData 0); then send 0x13 `{+0 initData}` to the element.

#### XuiSkin_CreateVisual (0x92153058)

```
XuiSkin_CreateVisual(name, prefix, &h, noAutoplay):
    h = 0
    hr = XuiLookupLoadedObject(name, prefix, &node, &root, &usedPrefix)
    if hr < 0: return hr                              // 0x80300026 not found
    if node.flags & 1: return 0x80300025              // tested before taking the lock
    lock 0x9280b208
    node.flags |= 1
    hr = XuiCreateFromLoadedObject(node, &h, bInit=0, autoplay=!noAutoplay, 1, root ? root : node)   // 0x92152b90
    node.flags &= ~1
    unlock
    if usedPrefix and hr >= 0: XuiElementSetSkinPrefix(h, prefix)     // 0x23 to the new visual
    return hr
```

`XuiCreateFromLoadedObject` (0x92152b90) raises the load depth, runs `XuiInstantiateObjectTree` (0x921523f8): the element class is the loaded object's class (XuiVisual for every dash skin entry; not checked), properties are applied, ClassOverride processed, children created, timelines built with the skin root as context (`XuiTimeline_BuildFromObject` 0x92152080). With autoplay, each timeline gets `0x921545d0(tl, 0, 0, -1, 1, 1)`; without, every created element gets +0xf0 bit 0x2000. So the visual inherits the control's autoplay setting. The root element's +8 receives the element count.

#### XuiControlGetVisual (0x92150240)

```
if out == NULL: return 0x80070057
out = 0
lvl = XuiControl level of h; if none: return 0x8030000A
c = first child of lvl
if c != 0 and c has a XuiVisual level: out = c; return 0
return 0x80300017
```

Only the first child is examined.

#### XuiControl_DestroyVisual (0x92157958), XuiControlDestroyVisual (0x9214cca8)

`XuiControlGetVisual`; if found, `XuiDestroyObject` (0x92147c50); then clear Control flag 0x4. Called by `XuiControl_CreateVisual`, by XuiControl on 0xa (destroy) and on 0x28 with data bit 0x10.

#### XuiVisual (impl 0x9215bf68)

| Id | Behavior |
| --- | --- |
| 0x5 | If data+0 != 0, data+8 != 0 and data+8 has a XuiControl level: `XuiElementGetSize(data+8)` (0x9214f408, message 0x6) and `XuiVisual_FitToSize(self, w, h)`. Handled in all cases. |
| 0x8 | `XuiVisual_FitToSize(self, data+8, data+0xc)` (the parent's new size). Handled. |
| 0x18 | out = `XuiSkin_GetSourcePath(XuiElementGetSkinPrefix(self))`. Handled. |
| 0x22 | out = +4. Handled. |
| 0x23 | +4 = copy of data+0; handled if the copy succeeded. |
| 0x800 | data+0xc = 1 ("target is source"), handled: tab order never enters a visual. |

`XuiVisual_FitToSize` (0x9215bea0): if the current size differs, set width and height (`0x921507d8`, which sends 0x10/0x8/0x80d), then re-anchor the visual's children (`0x921577e8`, `0x9215a150`). The visual's authored size is only the reference for the children's Anchor layout; at run time the visual always takes the control's size.

### 5. Named parts inside a visual

Status: complete for the mechanism; per-class use is in the class notes of other agents.

There is no generic binding by name. A control that needs a part calls `XuiControlGetVisual(self)` and then `XuiElementGetChildById(visual, id)` (0x9214db88 -> `XuiElement_GetChildById` 0x921574f8): direct children only, `wcscmp` (case-sensitive) against each child's Id (element +4), in child order, first match; not found 0x80300017. `XuiElementFindByPath` (0x92172390) is the multi-level form used for navigation ids.

Ids looked up in this binary:

| Class / function | Parent searched | Ids |
| --- | --- | --- |
| XuiScrollBar 0x92161ec0 (on 0x9) | visual | `ScrollBarBody`, `ScrollBarThumb` |
| XuiProgressBar 0x921620a0 (on 0x9) | visual | `ProgressBody` |
| XuiSlider `0x9215e770` | cached part (Slider +4) | `SliderThumb` |
| XuiEdit `0x9215eaa0` | visual | `Caret`, `Text`, `ScrollLeft`, `ScrollRight`, `ScrollUp`, `ScrollDown` |
| XuiHtmlControl `0x9219d3e8` | visual | `ScrollUp`, `ScrollDown`, `ScrollBar`, `HtmlPresenter` |
| `0x9215f6d8` (message box setup) | first child of the visual | `Button0`.. (`"Button "` with the last char replaced by `'0'+i`), `MessageText` |
| XuiMessageBox 0x9215f5e8 on 0xe | notify source Id | `Button<digit>` |
| `XuiShowMessageBoxEx` 0x9215f8e8 | skin lookup | visual name `XuiMessageBox<N>` (N = button count) checked with `XuiSkin_LookupVisual`, then set as the Visual property of a new XuiMessageBox |

Presenters are not looked up by Id (the skin uses arbitrary ids such as `text_Label`); they bind by ancestry (section 6).

### 6. Presenter data binding

Status: complete for the binding protocol. Text layout and drawing are not covered here; text presenter fields are in "Text, edit, message box and sound classes".

#### Source control

`XuiFindOwnerControl` (0x92157c60): walk parents starting at the presenter's parent; return the canonical handle of the first ancestor that has a XuiControl level, else 0. The presenter itself is never its own source. Inside a visual this is the control that owns the visual. Results are cached: XuiTextPresenter +0x24 (cleared on 0x5), XuiImagePresenter +0x8c (never cleared).

#### Messages

Data buffers are zeroed by `XuiMessageSetData` before the builder fills them. "item flag" 1 means data+0 is a list item index (sent by list items and the `*Item` helpers); 0 means the control itself.

| Id | Builder | Size | Layout |
| --- | --- | --- | --- |
| 0x7df get source text | `XuiMessageInitGetSourceText` 0x921b5508 (item, assoc, flag) | 0x28 | +0 item, +4 DataAssociation, +8 item flag, +0xc out wchar* text, +0x10 in/out float point size (builder sets -1.0 = none), +0x14 u32 (text presenter sets 1), +0x18 out float, +0x1c in/out u32 TextStyle, +0x20 out run count, +0x24 out run array (16-byte runs, +0 start, +4 end) |
| 0x7e0 set source text | `XuiMessageInitSetSourceText` 0x9224a278 (item, assoc, flag, text) | 0x28 | +0 item, +4 DataAssociation, +8 item flag, +0xc wchar* text |
| 0x7e1 get source image | `XuiMessageInitGetSourceImage` 0x921a5be0 (item, assoc, flag) | 0x1c | +0 item, +4 DataAssociation, +8 item flag, +0xc out u32 force reload, +0x10 out wchar* path, +0x14 out texture object, +0x18 out element handle |
| 0x7e2 set source image | `XuiMessageInitSetSourceImage` 0x922aa120 (item, assoc, flag, path, texture, element) | 0x1c | +0 item, +4 DataAssociation, +8 item flag, +0x10 path, +0x14 texture, +0x18 element |
| 0x7fb get source text with version | `XuiMsgInit_GetSourceTextVersion` 0x9219adc0 (item, assoc, flag) | 0x14 | +0 out u32 version, +4 item, +8 DataAssociation, +0xc item flag, +0x10 out wchar* text |

Senders: `XuiTextPresenter_GetSourceText` 0x9215c820 (0x7df), `XuiImagePresenter_UpdateFromSource` 0x9215f348 (0x7e1), XuiHtmlPresenter `0x9219bfa8` (0x7fb, re-parses only when the version changed), XuiListItem 0x9215d148 (forwards 0x7df/0x7e1 to its list as `{item index, assoc, 1}`), `XuiControlGetText`/`XuiControlSetTextById` 0x921451b0/0x92145208 (`{0,0,0}`), `XuiSendSetSourceTextItem` 0x92158ff0 (`0x7e0 {item, 0, 1, text}`), `XuiSendSetSourceImageItem` 0x92159048 (`0x7e2 {item, 0, 1, path, 0, 0}`).

#### XuiControl handlers (impl 0x9215aa68)

| Id | Condition | Behavior |
| --- | --- | --- |
| 0x7df | data+0 == 0 and data+4 == 0 | handled; +0xc = Text (+0x2c), +0x10 = PointSize (+0x30, default -1.0) |
| 0x7e0 | data+0 == 0 and data+4 == 0 | see below |
| 0x7e1 | data+0 == 0 and data+4 == 0 | handled; +0x10 = ImagePath (+0x34). +0xc/+0x14/+0x18 stay 0. |
| 0x7e2 | data+0 == 0 and data+4 == 0 | ImagePath = copy of +0x10; handled if the copy succeeded |
| 0x7fb | data+4 == 0 and data+8 == 0 | handled; +0 = text version (+0x3c), +0x10 = Text |

The item flag is not tested. Other DataAssociation values fall through to the base (unhandled) unless a derived class answers.

0x7e0:

```
cur = ctl.Text; new = data.text
if cur == NULL and new == NULL: handled; return
if cur != NULL and new != NULL and wcscmp(cur, new) == 0: handled; return
ctl.version += 1; if ctl.version == 0: ctl.version = 1
if assign(&ctl.Text, new) >= 0: handled        // empty string stores NULL
```

The Text property of XuiControl (bit 10, field +0x2c) has getter `XuiControl_Text_Get` 0x92145a88 (sends 0x7df `{0,0,0}`, returns +0xc) and setter `XuiControl_Text_Set` 0x92145a60 (type must be String; sends 0x7e0 `{0,0,0,text}`), so derived classes (lists, XuiEdit) can redirect it. ImagePath (bit 12, +0x34) has no accessor overrides. Neither path notifies presenters: presenters poll their source (below).

Derived answers seen: XuiProgressBar (0x921620a0) and XuiSlider (0x92162358) answer 0x7df when data+0 == 0 and data+4 == 1 with +0xc = their decimal value string (ProgressBar +0x14, written by `_itow_s` in `0x92162038` when the value is set; Slider +0x30). XuiEdit, lists and list items: see their owners.

DataAssociation values in the shipped XUI: no presenter in any 6717 archive sets DataAssociation, so all use 0.

#### XuiTextPresenter (impl 0x9215c9f0)

Instance: +0 handle, +4 TextColor, +8 DropShadowColor, +0xc PointSize, +0x10 Font, +0x14 font object, +0x18 TextStyle, +0x1c LineSpacingAdjust, +0x20 point size of the font object, +0x24 cached source control, +0x28 DataAssociation.

`XuiTextPresenter_GetSourceText(tp, &text, &size, &style, &extra, &runCount, &runs)`:

```
if extra: extra = 0.0
if style: style = tp.TextStyle
if tp.source == 0: tp.source = XuiFindOwnerControl(tp.h)
if tp.source == 0:
    resolver.vtbl[2](resolver, tp.h, &text)       // dash: 0x9272b668, E_NOTIMPL, writes nothing
    size = tp.PointSize
    return 0
d = 0x7df {0, tp.DataAssociation, 0}; d.+0x14 = 1; d.+0x1c = tp.TextStyle
hr = XuiSendMessage(tp.source, d); if hr < 0: return hr
text = d.text
size = (d.size != -1.0) ? d.size : tp.PointSize
extra = d.+0x18; style = d.+0x1c
if d.runCount != 0 or d.runs != 0:
    if text == 0 or runCount == 0 or runs == 0: return 0x80070057
    for each run i: require end >= start, and (i < count-1) next.start > end; else 0x80070057
    out runCount, runs
return 0
```

Messages: 0x0 render (`text` empty or NULL draws nothing; font recreated when missing, point size changed or TextStyle bits 1-3 differ), 0x5 clear the source cache, 0x28 with bit 0x4 release the font object, 0x29 create the font without drawing.

#### XuiImagePresenter (impl 0x92163090) and image brush

Instance: +0 handle, +4 image brush, +8 SizeMode (brush +4), +0x84 BrushFlags (brush +0x80), +0x8c cached source control, +0x90 DataAssociation, +0x94 texture identity (`0x9217a700(tex)` = tex+8).

Image brush (at instance +4, shared with XuiImage): +0 current path, +4 SizeMode, +8 u8 dirty, +0x5c and +0x68 element size data, +0x74 content kind (0 none, 1 texture, 2 child element), +0x78 source kind (1 none or pending, 2 path, 3 external element, 4 external texture), +0x7c last load HRESULT, +0x80 BrushFlags, +0x84 texture or element handle.

Dispatch: 0x0 update then render; 0x10 set dirty; 0x28 with bit0 and content kind 1: release, set dirty; 0x29 update; all other ids ignored.

`XuiImagePresenter_UpdateFromSource` (0x9215f348):

```
if ip.source == 0: ip.source = XuiFindOwnerControl(ip.h)
if ip.source == 0: return 0                       // no fallback
d = 0x7e1 {0, ip.DataAssociation, 0}
if XuiSendMessage(ip.source, d) < 0: return hr
kind = d.texture ? 4 : d.path ? 2 : d.element ? 3 : 1
reload = d.force != 0 or kind != brush.sourceKind or (kind == 3 and d.element != brush.object)
if !reload:
    if kind == 4 and (identity(d.texture) != ip.identity or d.texture != brush.object): reload
    if kind == 2 and (brush.path == 0 or wcscmp(brush.path, d.path) != 0): reload
if !reload: return 0
XuiImageBrush_Release(brush)                      // 0x92158238; sets contentKind 0, sourceKind 1
if kind == 4: brush.object = d.texture; ip.identity = identity(d.texture); brush.contentKind = 1
elif kind == 2:
    hr = XuiImageBrush_LoadPath(brush, ip.source, d.path, ip.h)
    if hr < 0: return hr                          // sourceKind stays 1: retried on the next update
    brush.path = copy(d.path)
elif kind == 3: XuiImageBrush_AttachElement(brush, ip.h, d.element)
brush.sourceKind = kind; brush.dirty = 1
```

`XuiImageBrush_LoadPath(brush, baseElem, path, owner)` (0x9215ebd0):

```
brush.hr = 0
hr = XuiResolveElementPath(&full, baseElem, path)          // 0x9215cde8
if hr < 0: brush.hr = hr; return hr
if wcslen(full) > 4 and _wcsnicmp(full + len - 4, ".xu", 3) == 0:
    hr = XuiLoadFromBinary(full, &h, bInit=1, flags=0)     // 0x9214ab00 -> 0x9214a9f8
    if hr >= 0: XuiImageBrush_AttachElement(brush, owner, h)    // contentKind 2, element added as child of owner
else:
    hr = texture load (0x9217bfc8, full, &brush.object); if hr >= 0: brush.contentKind = 1
brush.hr = hr; free full; return hr
```

`XuiResolveElementPath(&out, baseElem, path)`: if `path` is absolute (`0x9216ba10`) copy it; else combine (`0x9216bac0`) with `XuiElementGetBasePath(baseElem)` (bubbled 0x18) or, when `baseElem` is 0, `XuiSkin_GetSourcePath(NULL)`.

`XuiImageBrush_Release`: content 1: release the texture unless source kind 4; content 2: detach the element (`0x9214d478`) and destroy it unless source kind 3. Then content kind 0, source kind 1.

The image presenter resolves relative paths against its source control's base path, not its own.

#### XuiImage (impl 0x921631b8)

Instance: +0 handle, +4 image brush (ImagePath is brush +0), +8 SizeMode, +0x84 BrushFlags.

ImagePath setter `XuiImage_ImagePath_Set` 0x921436a8 -> `XuiImage_SetPath` 0x9213f638: dirty, `XuiImageBrush_Release`, store the path. `XuiImage_LoadIfNeeded` (0x9215f560): if a path is set and source kind is 1: release, `XuiImageBrush_LoadPath(brush, self, path, self)`; on success source kind 2 and dirty. Dispatch: 0x0 load if needed (failure returns the error, no draw) then render (`0x92162fd8`); 0x10 dirty; 0x21 handled, out 1; 0x28 bit0 with content 1: release, dirty; 0x29 load, handled.

A XuiImage inside a visual resolves relative paths against the skin file of the visual (XuiVisual answers 0x18).

Runtime (`control/image.rs`): a scene path loads on 0x29 and, for shown images, in the pre-draw pass. The file root becomes a child of the image and is placed once at attach by the size-mode rules of 0x9215f198, the `XuiImage_ComputeLayout` rules (xui-render.md) applied to the root's size. An ImagePath store that bypasses the setter leaves the old content attached until the next load. XuiImagePresenter data-bound scene paths are not loaded.

### 7. Resource release and prefetch

`XuiFreeResources(root, flags)` (0x921508b8): if root, broadcast 0x28 `{flags}`; then bit0 flushes the texture cache (`0x9217a400(0)`), bit2 the font cache (`0x921824d0`). Handlers related to visuals: XuiControl destroys its visual on bit 0x10 (recreated on the next render or 0x29); XuiTextPresenter releases its font on bit 0x4; XuiImage and XuiImagePresenter release a texture on bit 0x1; XuiNineGrid releases its texture on every 0x28 regardless of flags and marks itself for reload. `DashSetLanguageAndFonts` 0x921ca820 calls it with 4 or with -1 (all bits).

0x29 creates control visuals (XuiControl), text presenter fonts, and loads images (XuiImage, XuiImagePresenter) and the XuiNineGrid texture before the first render. XuiText, XuiFigure and XuiTextureEffect also handle 0x29; their handlers were not decoded here.

### 8. Other classes checked

- XuiGroup (0x9215cc68): no binding. 0xb/0xc focus through a tab-stop query, 0x10 re-anchors children.
- XuiCanvas (0x9215ce58): base path and skin prefix holder (section 2); 0x1f as XuiScene.
- XuiNineGrid (0x921635f8): no binding. Loads TextureFileName itself (`0x92160700`, resolved by `XuiResolveElementPath` relative to itself).

### Nuances

- Visual creation is lazy: a control has no visual after load and init. It is created on the first render message that reaches it, on 0x29, or on an explicit 0x9. A control the parent render loop skips (hidden) gets no visual until 0x29 or 0x9.
- When no visual is found, flag 0x4 stays clear and every later render message repeats the full lookup (name and every class name).
- The visual is inserted as the first child. `XuiControlGetVisual` only checks the first child, and `XuiControl_CreateVisual` destroys an authored XuiVisual that happens to be the control's first child.
- Lookup order is `prefix||name`, then `name`, with plain concatenation (`dashSkinXuiScene`). `XuiFreeVisuals(prefix)` removes every key that starts with the prefix, including unprefixed names that happen to begin with it.
- Registration is first-wins; loading a skin whose keys exist fails with 0x80004005 at the first duplicate, leaving earlier keys of that file registered and its source record replaced.
- A visual found by the unprefixed fallback answers 0x22 with NULL, so controls inside it never see the scene's blade skin prefix.
- The recursion flag is tested before the instantiation lock is taken.
- If inserting the visual fails, the created visual is leaked (not destroyed) and the error returned.
- Class-name fallback walks the control's own class chain, including application classes: a `CGamesBladeNavButton` without Visual tries `CGamesBladeNavButton`, `XuiNavButton`, `XuiButton`, `XuiControl`, `XuiElement`.
- XuiNineGrid drops its texture on every 0x28, even when the flags do not include the texture bit.
- Visual timelines autoplay unless the control has element flag 0x2000.
- Unknown ClassOverride classes are ignored; ClassOverride to a class not derived from the authored class fails the whole load (0x80300016).
- Presenters poll: the text presenter queries its source on every render (and 0x29); the image presenter on every render and 0x29. Setting Text or ImagePath sends no notification.
- A failed image load leaves source kind 1, so the load is retried on every update (every frame while visible).
- Image paths ending in `.xu` plus any fourth character (case-insensitive) load a XUR scene as the image content.
- Text point size -1.0 from the source means "use the presenter's PointSize"; XuiControl's PointSize defaults to -1.0.
- The image presenter's source control cache is never invalidated; the text presenter's is cleared on 0x5.
- Message 0x8 sent to children carries the new size in both pairs (+0/+4 and +8/+0xc); 0x10 to self carries old and new.

### Open questions

- Sender of message 0x21 (XuiControl answers "data+0's parent is me and it is a XuiVisual"; XuiImage always answers 1) was not found by builder or inline-constant search.
- Meaning of 0x7df data +0x14 (set to 1 by the text presenter), +0x18 (float out) and the run array semantics; see the XuiTextPresenter section below for what is known.
- `XuiSkin_ApplyStringTable` key format was read from the format strings and loop structure; the string table lookup (`0x92170c90`) and value replacement (`0x921710f0`) were not decoded.
- Whether the disc module's base-skin load ever runs in the same process as the main dashboard skin (it would fail as described) [INFERENCE: not determined].
- Purpose of skin root flag bit3 (set by `XuiLoadVisualFromBinary`) and the extra reference taken on each top-level child before registration.

## Lists, scrolling and range controls

Scope: XuiList, XuiCommonList, XuiListItem, XuiScrollEnd, XuiScrollBar, XuiSlider, XuiProgressBar. Facts are read from disassembly unless marked [INFERENCE].

Conventions used in this section:
- `this` = per-class instance (userdata) passed to the impl. `this+0` is always the element handle.
- `msg` = XUIMessage (+4 id, +8 bHandled, +0x10 pvData). `d` = msg pvData.
- "send" = XuiSendMessage 0x92144370 (class levels of one element). "bubble" = XuiBubbleMessage 0x92144500.
- Notify = message 0xe, data 0x10 bytes: {+0 u32 code, +4 hSource, +8 u32 cbExtra, +0xc ptr extra}. Built by 0x921af9b8.
  Notify helpers used here bubble starting at the PARENT of the source unless stated.
- Element instance fields: +4 Id, +0xc Width, +0x10 Height, +0x30 Position(x,y,z), +0x3c Scale, +0xe8 Anchor, +0xf0 flags (bit0 Show).
  Tree links: +0x18 first child, +0x1c last child, +0x20 next sibling, +0x24 prev sibling, +0x28 parent.
- Control instance fields: +0xc item data index (not a property; -1 = none; set by XuiControlSetIndex 0x9214ce38, read by XuiControlGetIndex 0x9214cdf8), +0x10 bit0 Enabled.

Shared helpers (renamed):

| Address | Name | Behavior |
| --- | --- | --- |
| 0x925958a0 | crt_ceil | double ceil |
| 0x925958e0 | crt_floor | double floor |
| 0x92595ea8 | crt_modf | modf(x, &ipart) -> frac |
| 0x92595880 | (unnamed) | integer to decimal wide string (value, buf, 12 chars, radix 10) |
| 0x92157f60 | XuiElementGetStateFrameRange | (elem, enabled, focused, &start, &end): focused -> Focus/EndFocus, or FocusDisable/EndFocusDisable if !enabled and FocusDisable exists; unfocused -> Normal/EndNormal or NormalDisable/EndNormalDisable if !enabled and NormalDisable exists; if either frame missing -> Normal/EndNormal. |
| 0x92157eb8 | XuiElementSetFlag20Recursive | sets element +0xf0 bit 0x20 on element and whole subtree. Element SetSize 0x921452c0 then scales children of such parents proportionally using parent +0x104/+0x108 (anchor reference size) [INFERENCE: proportional anchoring]. |
| 0x9214d350 | (unnamed) | returns TLS flag (set/restored with 0x9214d2f8). List init and child unlink notifications are suppressed while set. |


### 1. XuiList

#### Entry points

| Address | Name | Notes |
| --- | --- | --- |
| 0x92141948 | XuiList_CreateInstance | XuiAlloc(0x9c), XuiList_Construct |
| 0x9213eb78 | XuiList_Construct | see struct defaults |
| 0x92166b28 | XuiList_OnMessage | impl. Jump table 0x9200ba58 (u16 offsets from 0x921676f4) for ids 0x7da..0x7e9. BN split the jump-table cases into separate functions (named XuiList_OnMessage_*_7xx; there r25 = msg, r31 = this). |
| 0x921666b8 | XuiList_InitFromVisual | template discovery, property read |
| 0x92166040 | XuiList_CreateItems | measure + item control creation |
| 0x9215dfe0 | XuiList_CreateItem | one slot |
| 0x9215d930 | XuiList_GetItemRect | slot -> size, position |
| 0x9215dea8 | XuiList_PositionItem | apply rect, show/hide (variable size) |
| 0x92161fd8 | XuiList_LayoutItems | PositionItem for all slots, stops on first failure |
| 0x9215e120 | XuiList_GetVisibleCount | |
| 0x9215e460 | XuiList_GetMaxTop | |
| 0x9215e230 | XuiList_IsCurSlotValid | |
| 0x9215e298 | XuiList_IsCurOnFirstSlot | |
| 0x9215e2e0 | XuiList_IsCurOnLastVisibleSlot | |
| 0x9215e338 | XuiList_IsAtEnd | |
| 0x92165548 | XuiList_RefreshItems | requery count, rebind |
| 0x92165968 | XuiList_OnSizeChanged | msg 0x10 |
| 0x92157d70 | XuiList_ReleaseItems | |
| 0x92157e40 | XuiList_AdjustTemplateAnchor | |
| 0x92156f40 | XuiList_FocusSelf | |
| 0x9215e3f8 | XuiList_GetSize | |

Message senders (all renamed): XuiListSetCurSel 0x92158df8 (0x7d8), XuiListSetTopItem 0x921cbf10 (0x7db), XuiListGetTopItem 0x921bbb50 (0x7da, -1 on failure), XuiControlGetCurItem 0x921b39b8 (0x7d7), XuiListGetItemCount 0x921da7e0 (0x7dc kind 0, 0 on failure or negative), XuiListGetVisibleItemCount 0x92221578 (0x7dc kind 1, 0 on failure or negative), XuiListIsItemVisible 0x9224cdf8, XuiListSetCurSelVisible 0x921c4f50, XuiListInsertItems 0x92218aa0 (0x7dd), XuiListDeleteItems 0x921da840 (0x7de), XuiListGetItemCheck 0x92736cd8 (0x7e3), XuiListSetItemCheck 0x92158e90 (0x7e4), XuiListGetItemEnable 0x92736d20 (0x7e8), XuiListSetItemEnable 0x92250a80 (0x7e9), XuiScrollEndSetState 0x92159178 (0x7e7), XuiScrollBarSetRange 0x921591c0 (0x808), XuiControlUpdateState 0x92158f20 (0x7d6 zero data), XuiControlUpdateStateFocus 0x92158f60 (0x7d6 {1, focused}), XuiControlSetEnable 0x92238b78 (0x7d3), XuiControlIsEnabled 0x922215d8 (0x7d2), XuiControlGetValue 0x92736c90 (0x7d0), XuiControlSetValue 0x92158fa8 (0x7d1), XuiElementSetShow 0x9221f848 (0x12), XuiElementIsShown 0x9226b048 (0x11), XuiNotifySelChanged 0x92158e40.

XuiListIsItemVisible(h, i): top = 0x7da; vis = 0x7dc kind 1; true iff i >= 0 && top >= 0 && vis > 0 && i >= top && i < top + vis.
XuiListSetCurSelVisible(h, i): send 0x7d8(i); if it failed return; if !XuiListIsItemVisible(h, i) send 0x7db(i).

#### Properties

| Property | Owner | Storage | Effect |
| --- | --- | --- | --- |
| Wrap | XuiList | list +0x98 bit0 | wrap navigation; spinner (Layout 2/3) ScrollEnd states |
| Layout | XuiListItem (template) | item +4, setter 0x9213ea40 accepts 0..5 | copied to list +0x6c |
| Checkable | XuiListItem | item +8 bit1 (mask 2) | copied to list +4; read by name "Checkable" |
| SelectedSize | XuiListItem | item +0xc (vec3) | list +0x34 (y), +0x38 (x) |
| KeepSizeUnfocused | XuiListItem | item +8 bit2 (mask 4) | copied to list +8 |
| InterItemSpacing | XuiListItem | item +0x18 (vec3) | list +0x44 = max(y, 0), +0x48 = max(x, 0) |

The list reads the template properties by name with 0x9213d0b0 (name -> property id) and 0x92140c30 (get value) on the template ListItem instance, not from the ListItem struct directly.

Layout values (confirmed against skin usage: 4 on picture grids, 3 on "btn_itemSpinner"):
0 vertical list, 1 horizontal list, 2 vertical spinner (one item control), 3 horizontal spinner (one item control), 4 grid filled row by row (scrolls vertically), 5 grid filled column by column (scrolls horizontally).
"Horizontal" layouts in code = {1, 3, 5}.

#### Instance struct (0x9c bytes)

| Off | Ctor | Meaning |
| --- | --- | --- |
| +0x00 | handle | element handle |
| +0x04 | 0 | Checkable (from template) |
| +0x08 | 1 | KeepSizeUnfocused (from template) |
| +0x0c | 0 | list has focus within (set by 0xc and notify 2, cleared by 0xd and notify 3) |
| +0x10 | 0 | varH: selected height differs from item height (layouts 0,1 only) |
| +0x14 | 0 | varW: selected width differs from item width (layouts 0,1 only) |
| +0x18 | 0 | "next" XuiScrollEnd (Direction 1 or 3) |
| +0x1c | 0 | "prev" XuiScrollEnd (Direction 0 or 2) |
| +0x20 | 0 | XuiScrollBar |
| +0x24, +0x28 | 0.0 | item w, h (fresh instance of template record, min 1.0) |
| +0x2c, +0x30 | 0.0 | item current w, h (template element as placed in visual; changed by anchors on resize) |
| +0x34, +0x38 | 0.0 | selected h, w (SelectedSize, at least item h, w) |
| +0x3c, +0x40 | 0.0 | selected current h, w |
| +0x44, +0x48 | 0.0 | spacing y, x (InterItemSpacing clamped >= 0) |
| +0x4c, +0x50 | 0.0 | unused in observed code |
| +0x54, +0x58 | 0.0 | right and bottom margin of template inside designed visual |
| +0x5c, +0x60, +0x64 | 0.0 | template position x, y, z |
| +0x68 | 0 | template Anchor, with scroll-axis bits cleared |
| +0x6c | 0 | Layout |
| +0x70 | 0 | multi-item layout flag (1 for layouts 0,1,4,5) |
| +0x74 | 0 | pointer to template node record (XUR child record of the visual definition) |
| +0x78 | 0 | number of item controls (ctrls) |
| +0x7c | 0 | item control handle array (XuiAlloc) |
| +0x80 | -1 | current slot (slot of current item if visible, else -1) |
| +0x84 | 0 | rows (lines along vertical axis) |
| +0x88 | 0 | cols |
| +0x8c | -1 | top item index |
| +0x90 | -1 | current item index |
| +0x94 | 0 | item count (from 0x7dc kind 0) |
| +0x98 | 0 | bit0 Wrap (property), bit1 FocusSelf in progress, bit2 use margins (+0x54/+0x58); bit2 is never set in observed XUI code |

Derived values used below: perLine = (Layout in {1,3,5}) ? rows(+0x84) : cols(+0x88). count = +0x94, ctrls = +0x78, top = +0x8c, cur = +0x90, slot = +0x80.

#### Named child elements (inside the list's Visual)

Scanned children of the XuiVisual (first to last):
- first child whose class derives from XuiListItem: the template (only the first one is used).
- XuiScrollEnd children: Direction 0 or 2 -> +0x1c ("prev"), 1 or 3 -> +0x18 ("next"); first of each kind wins.
- XuiScrollBar child: first one -> +0x20.
No names are used; matching is by class. Note XuiScrollBar and XuiScrollEnd children are moved from the visual to be direct children of the list.

#### Message table

| Id | Data | Behavior |
| --- | --- | --- |
| 0x9 init | - | handled. If TLS flag (0x9214d350) set: return. Control-level init via 0x9214ccf0 (creates visual); on failure ReleaseItems and return. visual = XuiControlGetVisual; none -> return. InitFromVisual; on failure ReleaseItems. Then send 0x7d6 (zero data) to self. |
| 0xa destroy | - | ReleaseItems (not handled; reaches all levels). |
| 0xc set focus | {+0 hOther, +4 u8 user} | handled. Bubble notify 2 {hOther} from parent (source = list). If ctrls && slot != -1 && !(+0x98 bit1): SetFocus(items[slot], user); if next end: 0x7e7(next, IsAtEnd ? ((Layout 2/3 && Wrap) ? 1 : 0) : 1). Clear +0x98 bit1. If hOther == 0 or hOther not inside list: send 0x7d6 {1,1} to self. +0xc = 1. |
| 0xd kill focus | {+0 hOther} | if hOther is a descendant of list: handled, bubble notify 3 {hOther} from parent. Always +0xc = 0. Not handled when focus leaves the list (Control level plays kill focus). |
| 0xe notify | see sub table | |
| 0x10 size changed | {oldW, oldH, newW, newH} floats | handled, XuiList_OnSizeChanged. |
| 0x21 | {+0 hChild, +4 out bool} | if parent(hChild) == list and hChild is prev/next end, scrollbar or any item control: handled, +4 = 1. [INFERENCE: "child belongs to control template" query; Control answers it for its XuiVisual] |
| 0x24 key down | key | only page keys: XuiKeyToNavDirection 5 (0x5806 or PageUp) and 6 (0x5807 or PageDown). Pseudocode below. Other keys not handled. |
| 0x7d3 set enabled | {+0 bool} | not handled (Control level stores bit and refreshes). For each slot i with idx = top + i < count: en = flag ? 0x7e8(list, idx) : 0; if bool(IsEnabled(item)) != bool(en): 0x7d3(item, en). |
| 0x7d7 get cur sel | {+0 out idx, +4 out hItem} | handled. +0 = cur, +4 = slot != -1 ? items[slot] : 0. |
| 0x7d8 set cur sel | {+0 idx} | pseudocode below. |
| 0x7da get top | {+0 out} | handled, +0 = top. |
| 0x7db set top | {+0 in/out top} | pseudocode below. |
| 0x7dc get count | {+0 kind, +4 out} | handled for kind 1 (GetVisibleCount), 3 (ctrls), 4 (lines: Layout in {1,3,5} ? cols : rows), 5 (perLine). Kinds 0 and 2 are NOT answered by XuiList (answered by XuiCommonList or dash subclasses; else +4 stays 0). |
| 0x7dd insert items | {+0 idx, +4 count} | handled. If no visual: send 0x29 to self (Control turns it into 9 unless Control flag 4). RefreshItems. idx/count ignored. |
| 0x7de delete items | {+0 idx, +4 count} | handled. RefreshItems. idx/count ignored. |
| 0x7e4 set item check | {+0 idx, +4 bool} | handled. If !Checkable return. If idx outside [0, count) return. If count && ctrls && top >= 0 && idx in [top, top+VisibleCount): if bool(0x7d0(item)) != bool(value): 0x7d1(item, value). Bubble notify 6 {idx, value} starting at the list itself (source = list). |
| 0x7e6 navigate | {+0 dir, +4 hSource, +8 out hTarget, +0xc out noFocusChange} | pseudocode below. |
| 0x7e8 get item enable | {+0 idx, +4 out} | handled, +4 = 1 (default: every item enabled). |
| 0x7e9 set item enable | {+0 idx, +4 bool} | handled. If count && ctrls && top >= 0 && idx visible: if bool(0x7d2(item)) != bool(value): 0x7d3(item, value). |
| 0x7df,0x7e0,0x7e1,0x7e2,0x7e3,0x7e5,0x7e7 | | not handled at List level. |
| 0x800 find tab stop | {+0 kind, +4 h, +8 out, +0xc out flag} | kind 2 (first) or 3 (last): idx = GetCurItem; if idx >= 0: XuiListSetCurSelVisible(list, idx), +0xc = 1, handled. Effect: tab search stops at the list itself (FindFirst/LastTabStop then returns the list). |

Notify (0xe) handling at List level (d.code):

| Code | Source | Behavior |
| --- | --- | --- |
| 1 press | item | handled. extra {+0 u8 user}. If Checkable: idx = XuiControlGetIndex(source); listCheck = 0x7e3(list, idx); itemCheck = 0x7d0(source); if bool differs: 0x7e4(list, idx, itemCheck). Then XuiControlNotifyPress(list, user): bubble notify 1 from list's parent with source = list. |
| 2 set focus | descendant | not handled. extra {+0 hOther}. If source is a descendant of list: if hOther == 0, or (hOther not a descendant and hOther != list): Control focus-state frames on list with focused = 1 (0x9214cd88). Always +0xc = 1; if varH or varW: LayoutItems. |
| 3 kill focus | descendant | not handled. extra {+0 hOther}. If hOther != 0 and (hOther descendant of list or hOther == list): focus stays inside. Else: focus-state frames on list with focused = 0, and "left" = 1. +0xc = 0; if varH or varW LayoutItems. If left and next end: 0x7e7(next, IsAtEnd ? ((Layout 2/3 && Wrap) ? 1 : 0) : 1). |
| 5 state request | ScrollEnd | handled. If source == prev end: 0x7e7(prev, (top > 0 or (Layout 2/3 and Wrap)) ? 1 : 0). If source == next end: 0x7e7(next, !IsAtEnd ? 1 : ((Layout 2/3 and Wrap) ? 1 : 0)). |
| 7 | child | handled. extra {+0 u32 a, +8 u8 b}. Re-bubbles notify 7 from list's parent with source = list (0x9215a6e0, builder 0x92158898); return value written to msg d+4. [meaning unknown] |
| 0xe scroll | scrollbar | handled. If source == +0x20: 0x7db(list, extra[0]); if !XuiElementHasFocus(list): XuiElementSetFocus(list, 0xff). The base XuiScrollBar never sends it. |

#### Template discovery and item creation

```
InitFromVisual(list, visual):                      // 0x921666b8
  ReleaseItems(list)
  tmpl = 0
  for child in children(visual):
    if child is XuiScrollEnd: dir = ScrollEnd.Direction
        if dir in {0,2}: if !list.prev: list.prev = child
        else:            if !list.next: list.next = child
    elif child is XuiScrollBar: if !list.bar: list.bar = child   (Direction read, unused)
    elif child derives XuiListItem and tmpl == 0:
        tmpl = child
        +0x5c..+0x64 = Position(tmpl)
        (+0x2c, +0x30) = Size(tmpl)         // via msg 6 (XuiElementGetSize)
        +0x68 = Anchor(tmpl); +0x6c = tmpl.Layout (ListItem +4)
        list.Checkable = prop "Checkable"
        list.KeepSizeUnfocused = prop "KeepSizeUnfocused"
        +0x34 = +0x28; +0x38 = +0x24; +0x3c = +0x30; +0x40 = +0x2c     // +0x24/+0x28 are 0 on first init
        sel = prop "SelectedSize"; if sel.y > +0x28: +0x34 = sel.y; if sel.x > +0x24: +0x38 = sel.x
        sp = prop "InterItemSpacing"; +0x44 = max(sp.y, 0.0); +0x48 = max(sp.x, 0.0)
        AdjustTemplateAnchor(list)
  if tmpl == 0: return 0x80300017
  with TLS flag set: unlink prev, next, bar, tmpl from the visual
  XuiDestroyObject(tmpl)
  id = visual.Id (Element +4)
  ctx = 0x9214f2a0(list)        // msg 0x22 bubbled from list: skin prefix (see "Skin prefix and base path propagation")
  visRec = 0x92152368(id, ctx, &rec, ...)   // visual definition record by name
  +0x74 = first entry of visRec children whose class (entry +0) derives XuiListItem
  if none: return E_UNEXPECTED (0x8000ffff)
  return CreateItems(list, visRec)

AdjustTemplateAnchor(list):                         // 0x92157e40
  +0x70 = 0
  Layout 0: anchor &= ~0xa ; Layout 1: anchor &= ~0x5 ; Layout 4,5: anchor &= ~0xf ; others: return
  +0x70 = 1
// Anchor bits: 1 left, 2 top, 4 right, 8 bottom [INFERENCE from the 5/0xa stretch tests]

CreateItems(list, visRec):                          // 0x92166040
  clone = InstantiateNodeTree(+0x74)  (0x92153050, args 1,1,0)
  (+0x24, +0x28) = Size(clone)        // fail -> destroy clone, return
  VW = visRec property "Width"  if present else list Width
  VH = visRec property "Height" if present else list Height
  if +0x24 < 1.0 or +0x28 < 1.0: clamp each to 1.0  (debug id lookups only)
  XuiDestroyObject(clone)
  w = +0x24; h = +0x28
  +0x34 = max(+0x34, h); +0x38 = max(+0x38, w)
  +0x54 = VW - (w + posX); +0x58 = VH - (h + posY)
  varH = Layout in {0,1} && |+0x34 - h| >= 1.1920929e-7
  varW = Layout in {0,1} && |+0x38 - w| >= 1.1920929e-7
  if !varH: +0x34 = h ; if !varW: +0x38 = w
  (W, H) = Size(list)
  if +0x70 == 0: rows = cols = ctrls = 1
  else:
    mx, my = (+0x98 bit2) ? (+0x54, +0x58) : (0, 0)
    Fit(avail, size, sp): q = avail / (size + sp); f = modf(q)
                          return (int)(f * (size + sp) >= size ? ceil(q) : floor(q))   // fctiwz truncation
    Layout 0: n = Fit(H - posY - my, h, spY); ctrls = rows = n; cols = 1
              +0x3c = +0x34; +0x40 = floor(+0x2c / w * +0x38 + 0.5)
    Layout 1: n = Fit(W - posX - mx, w, spX); ctrls = cols = n; rows = 1
              +0x40 = +0x38; +0x3c = floor(+0x30 / h * +0x34 + 0.5)
    Layout 4,5: rows = Fit(H - posY - my, h, spY); cols = Fit(W - posX - mx, w, spX); ctrls = rows * cols
  rows = max(rows, 1); cols = max(cols, 1)
  if ctrls < 1: ctrls = (Layout 4) ? cols : (Layout 5) ? rows : 1
  if slot >= ctrls: cur = slot - ctrls + cur - 1          // odd adjustment, kept as is
  items = XuiAlloc(ctrls * 4)
  for i in 0..ctrls-1: CreateItem(list, i)    // abort on failure
  with TLS flag set: append prev, next, bar as children of list (in that order: prev(+0x1c), next(+0x18), bar)
  RefreshItems(list)

CreateItem(list, i):                                // 0x9215dfe0
  e = InstantiateNodeTree(+0x74)
  PositionItem(list, i, e); SetShow(e, 0); AppendChild(list, e); items[i] = e
```

Items are therefore children of the list element itself (not of its visual), z-ordered before the scroll ends and scrollbar.

CreateItems does not reset the slot: when the old slot is past the new controls and RefreshItems does not find the current item on a slot, slot still indexes past `items` and the next 0xc reads `items[slot]` out of bounds. The reimplementation clears the slot in that case instead (not 1:1).

#### Item rectangle (XuiList_GetItemRect 0x9215d930)

```
GetItemRect(list, i) -> size{w,h}, pos{x,y,z}
  s = slot
  if (varW or varH) and s == -1 and cur >= 0 and top >= 0:
      d = cur - top; s = (d >= 0 and d < ctrls) ? d : -1
  big = (s >= 0) and (+0xc or +0x8)        // focused within, or KeepSizeUnfocused
  pos = (posX, posY, posZ)
  Layout 0:
    if i > s and s >= 0 and big: pos.y = (spY + h) * (i - 1) + (+0x3c + spY) + posY
    else:                        pos.y = (spY + h) * i + posY
    if i == s and big: pos.x = floor(posX - (+0x40 - +0x2c) * 0.5 + 0.5); size = (+0x40, +0x3c)
    else: size = (+0x2c, h)
  Layout 1:
    if i > s and s >= 0 and big: pos.x = (spX + w) * (i - 1) + (+0x40 + spX) + posX
    else:                        pos.x = (spX + w) * i + posX
    if i == s and big: pos.y = floor(posY - (+0x3c - +0x30) * 0.5 + 0.5); size = (+0x40, +0x3c)
    else: size = (w, +0x30)
  Layout 2,3: size = (+0x2c, +0x30)
  Layout 4: c = i % cols, r = i / cols (each clamped to 0 when negative for the spacing term)
    pos.x = c * w + spX * c + posX ; pos.y = r * h + spY * r + posY ; size = (w, h)
  Layout 5: c = i / rows, r = i % rows
    pos.x = c * w + spX * c + posX ; pos.y = r * h + spY * r + posY ; size = (w, h)
  Layout > 5: all zero
```

PositionItem(list, i, e) (0x9215dea8): rect = GetItemRect; SetPosition(e, pos); SetSize(e, size) (0x921507d8). Then:
- Layout 0 and varH: (W,H) = Size(list); shown = IsShown(e); if pos.y + size.h > H: if shown and i != 0: SetShow(e, 0). Else (fits): if !shown and i < count - top: SetShow(e, 1).
- Layout 1 and varW: same with pos.x + size.w against W.
Fixed-size layouts never move items after creation; scrolling only rebinds data indexes.

#### Visible count, max top, end test

```
GetVisibleCount(list):                              // 0x9215e120
  if !varH and !varW: return min(count, ctrls)
  (W, H) = Size(list)
  for s = ctrls-1 downto 0:
    rect = GetItemRect(s)
    if Layout 0 and pos.y + size.h <= H: return s + 1
    if Layout 1 and pos.x + size.w <= W: return s + 1
  r = min(count, ctrls); return r > 1 ? 1 : r
  // the variable-size result is not clamped by count

GetMaxTop(list):                                    // 0x9215e460
  m = count - ctrls
  if Layout 0 and varH: (W,H) = Size(list); n = (int)floor((H - posY - +0x3c) / (spY + h)); m = count - (n + 1)
  if Layout 1 and varW: n = (int)floor((W - posX - +0x40) / (spX + w)); m = count - (n + 1)
  if perLine != 0 and m % perLine != 0: m += perLine - m % perLine       // round up
  return max(m, 0)

IsAtEnd(list):                                      // 0x9215e338
  if !varH and !varW: return top + ctrls >= count
  s = count - top - 1; if s < 0 or s >= ctrls: return 0
  (W,H) = Size(list); rect = GetItemRect(s)
  return pos.y >= posY and pos.y + size.h <= H        // vertical test only, for any layout

IsCurSlotValid: cur >= 0 and cur < count and slot >= 0 and slot < GetVisibleCount
IsCurOnFirstSlot: IsCurSlotValid and slot == 0
IsCurOnLastVisibleSlot: IsCurSlotValid and slot + 1 == GetVisibleCount
```

#### Selection (0x7d8 set cur sel)

```
d.idx < 0 -> d.idx = -1 ; d.idx >= count -> d.idx = count - 1      (written back to d)
old = cur; if d.idx == old: return (handled)
oldSlot = slot; cur = d.idx
slot = (cur >= top and cur < top + GetVisibleCount) ? cur - top : -1
if ctrls:
  if varH or varW: LayoutItems
  f = focus owner within list (XuiElementGetFocusOwner 0x92150078)
  if f == list:            if slot != -1: SetFocus(items[slot], user of f)
  elif f inside list:      if slot != -1: SetFocus(items[slot], user of f)
                           else: r = FocusSelf(list, f); if r < 0 or r == 0x80300014: slot = oldSlot; cur = old
  else (focus elsewhere):  if oldSlot != -1: 0x7d6(items[oldSlot]); if slot != -1: 0x7d6(items[slot])
bubble notify 4 {+0 cur, +4 old} starting at the list (source = list)   // even if revert happened
```
FocusSelf(list, f) (0x92156f40): +0x98 |= 2; SetFocus(list, user of f); +0x98 &= ~2. Bit 2 stops the 0xc handler from forwarding focus to an item.

#### Scrolling (0x7db set top)

```
handled
if d.top < 0: return E_INVALIDARG
if count == 0: d.top = -1; return
if d.top == top: return
d.top = min(d.top, GetMaxTop())
if perLine != 0: d.top -= d.top % perLine          // round down to line start
if d.top == top: return
if Layout in {2,3} and Wrap:
    if top == count-1 and d.top == 0: fwd = 1
    elif top == 0 and d.top == count-1: fwd = 0
    else: fwd = top < d.top
else: fwd = top < d.top
top = d.top
slot = (cur >= top and cur < top + GetVisibleCount) ? cur - top : -1
en = IsEnabled(list)
for i in 0..ctrls-1: idx = top + i
    if idx >= count: SetIndex(items[i], -1); SetShow(items[i], 0)
    else: SetIndex(items[i], idx); SetShow(items[i], 1)
          if Checkable: 0x7d1(items[i], 0x7e3(list, idx))
          0x7d3(items[i], en ? 0x7e8(list, idx) : 0)      // unconditional
f = focus owner
if slot == -1: if f inside list: FocusSelf(list, f)
else: if (f == list or f inside list) and f != items[slot]: SetFocus(items[slot], user of f)
if varH or varW: LayoutItems
next end: if (Layout 2/3 and Wrap) or !IsAtEnd: state = fwd ? 2 : 1 else 0
prev end: if (Layout 2/3 and Wrap) or top > 0:  state = fwd ? 1 : 2 else 0
bar: 0x808(count, top, ctrls)
```
No smooth scrolling exists in list code. Scrolling is an instant rebind of data indexes to fixed item controls; the only animation is the ScrollEnd "Scrolling" state.

#### Data refresh (XuiList_RefreshItems 0x92165548)

Called on create, 0x7dd, 0x7de and from OnSizeChanged.
```
oldCount = count
count = XuiListGetItemCount(list)              // 0x7dc kind 0 sent to the list element (subclasses answer)
if count == 0:
    top = cur = slot = -1
    for each item: SetIndex(-1); SetShow(0)
    prev, next: 0x7e7(0); bar: 0x808(0, 0, 0)
elif oldCount == 0:
    0x7db(list, 0); 0x7d8(list, 0); bar: 0x808(count, 0, 1)
else:
    if cur >= count: cur = -1; slot = -1
    mt = GetMaxTop()
    if top > mt: 0x7db(list, mt)
    else:
      f = focus owner; en = IsEnabled(list)
      for i in 0..ctrls-1: idx = top + i
        if idx >= count: SetIndex(-1); if IsShown(item): SetShow(0)
        else:
          SetIndex(item, idx)
          if !IsShown(item): SetShow(1); 0x7d6(item)
          if Checkable: 0x7d1(item, 0x7e3(list, idx))
          e = en ? 0x7e8(list, idx) : 0; if bool(e) != bool(0x7d2(item)): 0x7d3(item, e)
          if idx == cur:
            os = slot; slot = i
            if f == list: SetFocus(items[i], user of f)
            elif f not inside list: if os != i: 0x7d6(items[i]); if 0 <= os < ctrls: 0x7d6(items[os])
      if varH or varW: LayoutItems
      next end: IsAtEnd ? ((Layout 2/3 and Wrap) ? 1 : 0) : 1
      prev end: top > 0 ? 1 : ((Layout 2/3 and Wrap) ? 1 : 0)
    bar: 0x808(count, top, ctrls)
```

#### Navigation between items (0x7e6)

Origin: XuiControl key handler (0x9215aa68) maps XuiKeyToNavDirection 1 left, 2 right, 3 up, 4 down, 7 tab to 0x7e6 dir 2, 3, 0, 1, 4|(key flags bit1 >> 1), sends 0x7e6 {dir, self} to itself. The Control level of the item forwards the message to its parent (the list) when d.hSource's Control node is itself. If still unhandled, the parent's Control level resolves d+8 = XuiControlGetExplicitNav(parent, dir, 0, 1) and sets +0xc = (target == source). Back in the key handler: if d+8 != 0 the key is handled and, when d+0xc == 0, focus moves to d+8.

```
List 0x7e6:
  if dir in {4,5}: d.hSource = list; return (not handled)  // tab moves relative to the list
  if Control node of d.hSource == list: return
  prevDir, nextDir = (Layout in {1,3,5}) ? (2,3) : (0,1)
  crossPrev, crossNext = Layout 4 ? (2,3) : Layout 5 ? (0,1) : (none)
  s = slot index of d.hSource in items, else return
  top0 = top; changed = 0
  if dir == prevDir:
     if s >= perLine: SetCur(cur - perLine)
     elif top > 0:
        SetCur(cur - perLine)
        if (new cur == cur - perLine) and top == top0: SetTop(top0 - perLine)
        changed = 1
     elif top == 0 and Wrap:
        rem = count % perLine
        n = rem == 0 ? cur - perLine + count : (cur >= rem ? count - 1 : cur - rem + count)
        XuiListSetCurSelVisible(list, n); changed = 1
  elif dir == nextDir:
     n = min(cur + perLine, count - 1)
     lastLine = count - (count % perLine ? count % perLine : perLine)    // first index of last line
     v = max(GetVisibleCount, 1)
     lim = min(lastLine, ((v - 1) / perLine) * perLine)
     if s < lim: SetCur(n)
     elif (cur - cur % perLine) + perLine < count:
        SetCur(n)
        if (new cur == n) and top == top0: SetTop(top0 + perLine)
        changed = 1
     elif Wrap: XuiListSetCurSelVisible(list, cur % perLine); changed = 1
  elif dir == crossPrev: if s % perLine != 0: SetCur(cur - 1)
  elif dir == crossNext: if s % perLine < perLine - 1 and cur < count: SetCur(cur + 1)
  if slot != s or changed: handled = 1; d.noFocusChange = 1
```
lastLine is an item index while s is a slot index; they are compared directly.
SetCur = 0x7d8 via XuiListSetCurSel, SetTop = 0x7db. "new cur" is re-read from +0x90 after the send.
When the move fails (SetCur clamped to the same item, no change) the message stays unhandled and normal control navigation (NavUp/NavDown/...) of the list applies, so focus leaves the list.

#### Page keys (0x24 at List level)

```
dir = XuiKeyToNavDirection(key, 0); only 5 (page up) and 6 (page down); handled = 1
if top < 0: return
ok = IsCurSlotValid
page up:
  if count > ctrls and top != 0:
     if !IsCurOnFirstSlot and ok: SetCur(top)
     else: nt = max(top - GetVisibleCount, 0)
           if ok: SetCur(max(cur - top + nt, 0))
           SetTop(nt)
  else: if !IsCurOnFirstSlot and ok: SetCur(0)
page down:
  if count > ctrls and GetVisibleCount + top < count:
     if !IsCurOnLastVisibleSlot and ok: SetCur(GetVisibleCount + top - 1)
     else: v = GetVisibleCount; nt = top + v; if v + nt >= count: nt = count - v
           if perLine != 0 and nt % perLine != 0: nt += perLine - nt % perLine
           if ok: SetCur(min(cur - top + nt, count - 1))
           SetTop(nt)
  else: if !IsCurOnLastVisibleSlot and ok: SetCur(count - 1)
```
[INFERENCE] The list sees 0x24 when it has focus itself or when an item leaves the key unhandled and the key message bubbles up to it (`XuiProcessKeystroke`).

#### Resize (msg 0x10, XuiList_OnSizeChanged 0x92165968)

```
if ctrls == 0: return
for e in (prev, next, bar) if present: 0x92157570(e, Anchor(e), newW, newH, 0)   // re-anchor in list
if Layout in {4,5}: LayoutItems
else: for each item: 0x92157570(item, +0x68, newW, newH, 0)
if !varH and !varW: (+0x2c, +0x30) = Size(items[0])
else: a = Anchor(items[0]); sc = Scale(items[0]) (0x9214d8f8)
      if (a & 5) == 5:  +0x2c += (newW - oldW) / sc.x
      if (a & 0xa) == 0xa: +0x30 += (newH - oldH) / sc.y
if +0x70 == 0: return
recount exactly as in CreateItems with (W,H) = (newW,newH) giving n, rows, cols, +0x3c, +0x40
   (Layout 2,3 cannot reach here; other values return E_UNEXPECTED)
if n < ctrls:
   if slot >= n: cur = slot - n + cur - 1; f = focus owner; if f inside list: FocusSelf(list, f)
   destroy items[n..ctrls-1]; ctrls = n; if slot >= n: slot = n - 1
   LayoutItems; RefreshItems
elif n > ctrls:
   grow array (0x92155fe0); ref = Element instance of items[0]
   for i in ctrls..n-1: CreateItem(i); copy Element +0x104..+0x110 (4 floats) from ref to new item
   ctrls = n; LayoutItems; with TLS flag set: move prev, next, bar to end of child list; RefreshItems
else: if varH or varW: LayoutItems
```

#### Destroy (XuiList_ReleaseItems 0x92157d70)
ctrls = 0. If the focus owner is inside the list: unlink it from its parent (0x9214d478) and FocusSelf(list, it). Destroy prev, next, bar (fields cleared). Destroy all old item controls, XuiFree(items), +0x7c = 0, +0x74 = 0.

#### Named frames used by the list
None directly. The list sends 0x7d6 to itself; Control level plays focus state frames. Item and scroll-end frames are listed in their sections.

#### RE status
complete for every handler and helper above. Unknowns: 0x92157570 anchor math (element layout). Notify 7 is the cancelable "pressing" notify of XuiControl (see "Press"); msg 0x22 returns the skin prefix (see "Visual and skin resolution").


### 2. XuiCommonList

Base XuiList. Holds an item store and answers the data-source messages.

| Address | Name |
| --- | --- |
| 0x9214b1c8 | XuiCommonList_CreateInstance (XuiAlloc 0x40, zero fill) |
| 0x9216aec0 | XuiCommonList_OnMessage (jump table 0x9200ba78, base 0x9216af1c, ids 0x7dc..0x7e9) |
| 0x9216ac80 | XuiCommonList_ApplyItemProps |
| 0x9216abc8 | XuiSplitCrLf |

#### Instance struct (0x40)

| Off | Meaning |
| --- | --- |
| +0x00 | handle |
| +0x04 | ItemsText (wchar*), setter 0x9213ec28 |
| +0x08 | ItemsImage (wchar*), setter 0x9213eca0 |
| +0x0c | ItemsNavPath (wchar*), setter 0x9213ed18 |
| +0x10 | array T: text strings, elements 0xc bytes (+0 wchar*); +0x14 count; +0x18 [INFERENCE capacity] |
| +0x1c | array I: image path strings, 0xc bytes each; +0x20 count; +0x24 |
| +0x28 | array F: u32 per item; bit0 disabled, bit1 checked; +0x2c count; +0x30 |
| +0x34 | array N: pointers to nav records (0x14 bytes: +4 wchar* path, +8, +0xc, +0x10); +0x38 count; +0x3c |

Every setter stores the string and calls ApplyItemProps.

```
ApplyItemProps(cl):                                  // 0x9216ac80
  t = SplitCrLf(ItemsText); i = SplitCrLf(ItemsImage); p = SplitCrLf(ItemsNavPath)
  n = max(len t, len i, len p); cur = XuiListGetItemCount(h)
  if n <= cur: XuiListDeleteItems(h, n, cur - n) else XuiListInsertItems(h, cur, n - cur)
  XuiListSetCurSel(h, 0); if XuiControlGetCurItem(h) == 0: XuiListSetTopItem(h, 0)
  for k < n: 0x7e0(h, k, bItemData=1, text = k < len t ? t[k] : null)
  for k < n: 0x7e2(h, k, bItemData=1, path = k < len i ? i[k] : null)
  for k < n: 0x804(h, k, path = k < len p ? p[k] : null, 0, 0, 0)
SplitCrLf(s): segments terminated by L"\r\n"; a final segment without "\r\n" is dropped.
```

#### Message table

| Id | Data | Behavior |
| --- | --- | --- |
| 0x7dc | {kind, out} | kind 0: +4 = T.count, handled. kind 2: +4 = number of F entries with bit1, handled. Others pass to XuiList. |
| 0x7dd insert | {idx, count} | if count > 0 and 0 <= idx <= T.count: insert count empty entries at idx into T, I, N, F (rollback and E_OUTOFMEMORY on failure). Not handled: XuiList then refreshes. Invalid args: return without change. |
| 0x7de delete | {idx, count} | if count > 0, idx >= 0, idx + count <= T.count: remove range from T, I, F; else result E_INVALIDARG. In both cases: free every N record from idx to N.count-1 (string and record, slot set to 0), then remove range [idx, idx+count) from N. Not handled. |
| 0x7df get text | 0x28 bytes | if d+8 (bItemData) != 0: handled, d+0x10 = -1.0f, if 0 <= idx < T.count: d+0xc = T[idx] else E_INVALIDARG. |
| 0x7e0 set text | {idx, +8 bItemData, +0xc text} | if bItemData: handled; idx in T range: assign string (E_OUTOFMEMORY on failure) else E_INVALIDARG. |
| 0x7e1 get image | 0x1c bytes | if bItemData: handled; idx in I range: d+0x10 = I[idx] else E_INVALIDARG. |
| 0x7e2 set image | {idx, +8 bItemData, +0x10 path} | if bItemData: handled; idx in I range: assign. |
| 0x7e3 get check | {idx, out} | handled; idx < I.count (bound uses I.count): +4 = F[idx] bit1. |
| 0x7e4 set check | {idx, val} | idx < I.count: set or clear F bit1. Not handled (XuiList updates item, notify 6). |
| 0x7e8 get enable | {idx, out} | handled; +4 = !(F[idx] bit0). |
| 0x7e9 set enable | {idx, val} | val != 0 clears bit0, else sets bit0. Not handled (XuiList updates item). |
| 0x803 get nav data | {idx, out x4} 0x14 | handled; idx < N.count: copy record +4..+0x10 (zeros if record null); else E_INVALIDARG. |
| 0x804 set nav data | {idx, path, a, b, c} | handled; allocate record if null, copy path, store a, b, c. |

Nav data layout {+0 idx, +4 path, +8 stayVisible, +0xc srcTransIndex, +0x10 destTransIndex} (field order matches XuiNavButton; Scene impl 0x92169720 uses it: on notify 1 from an XuiList it reads XuiControlGetCurItem, then XuiControlGetNavData 0x921590a8, and if path != null creates the scene and navigates forward with those values).

RE status: complete.


### 3. XuiListItem

Base XuiCheckbox. Instance 0x24: +4 Layout, +8 flags (bit1 Checkable, bit2 KeepSizeUnfocused), +0xc SelectedSize (3 floats), +0x18 InterItemSpacing (3 floats). Create 0x921417d8. Impl 0x9215d148 (XuiListItem_OnMessage).

The list item uses none of its own geometry properties; the list reads them from the template.

#### Message table

| Id | Condition | Behavior |
| --- | --- | --- |
| 0x24 key down | not Checkable | if press slot of key user (0x9214c7d8(d+0xc)) != 0: handled (swallowed). Else falls through to Checkbox level (press handling). |
| 0x7d0 get check | not Checkable | handled, d+0 = 0. |
| 0x7d1 set check | not Checkable | handled, ignored. |
| 0x7d6 update state | always | handled, XuiListItem_UpdateState(d). |
| 0x7df get source text | always | if d+0 != 0: return 0x80300021 (not handled). parent = GetParent; none: 0x8030000b. idx = XuiControlGetIndex(self). Build 0x7df {+0 idx, +4 d+4, +8 1, +0x10 -1.0f}; if idx != -1 send to parent. msg.bHandled = sent bHandled; copy +0xc, +0x10 (float), +0x20, +0x24 back. |
| 0x7e1 get source image | always | same pattern with 0x7e1 {+0 idx, +4 d+4, +8 1}; copy bHandled, +0xc, +0x10, +0x14, +0x18 back. |
| 0x800 find tab stop | d+0 in {0,1} | parent = GetParent; if parent: send 0x800 {kind, parent} to parent and copy 16 data bytes back. handled. Tab next/prev from an item is answered relative to the list. |

#### Visual state (XuiListItem_UpdateState 0x9215cf10), data {+0 bFocusArg, +4 focused}

```
if no visual: return
focused = d.bFocusArg ? d.focused : XuiElementHasFocus(self)
enabled = IsEnabled(self)
if !Checkable and XuiControlIsPressed(self):
    if enabled: play focused ? (Press, EndPress) : (NormalPress, EndNormalPress)
    else: if visual has PressDisable and EndPressDisable: play them (else nothing)
    return
if !focused and parent and XuiControlGetCurItem(parent).hItem == self:
    chk = 0x7d0(self)
    play enabled ? (chk ? NormalSelCheck : NormalSel) : (chk ? NormalSelCheckDisable : NormalSelDisable)  with End* pair
    return
base = next class level (XuiObjectGetBaseLevel); d.bFocusArg ? XuiControlUpdateStateFocus(base, d.focused) : XuiControlUpdateState(base)
```
Playback uses XuiControlPlayStateFrames (fallback table in foundation: NormalSel -> Normal, NormalSelCheck -> NormalSel, NormalSelDisable -> NormalSel, NormalSelCheckDisable -> NormalSelDisable, PressDisable -> NormalPress).

Named frames: Press, EndPress, NormalPress, EndNormalPress, PressDisable, EndPressDisable, NormalSel, EndNormalSel, NormalSelCheck, EndNormalSelCheck, NormalSelDisable, EndNormalSelDisable, NormalSelCheckDisable, EndNormalSelCheckDisable, plus the Checkbox/Control frames (Normal, Focus, NormalCheck, FocusCheck, ...).

RE status: complete (Checkbox level behavior owned by the Checkbox notes).


### 4. XuiScrollEnd

Instance 0x14 (generic create 0x92141650 shared with NavButton and SoundXAudio): +4 Direction (setter 0x9213eaa8 accepts 0..3), +8 state. Impl 0x9215d4c8.
Direction: 0 up, 1 down, 2 left, 3 right [INFERENCE from list usage: 0/2 = toward start, 1/3 = toward end, and skin visuals scr_ScrollEndUp/Down].

| Id | Behavior |
| --- | --- |
| 0xf timeline end | not handled; if state == 2: notify 5 bubbled from parent (0x9219aea8), source = self. |
| 0x7d6 | handled; XuiScrollEnd_PlayState(state). |
| 0x7e7 set state {+0 state} | handled; if state == 2 or state != current: current = state; PlayState(state). |

XuiScrollEnd_PlayState (0x9215d3e0): if no visual return. en = IsEnabled(self).
- 0: Control focus-state frames with {0,0} (0x9214cd30): Normal/Focus/Disable selection.
- 1: en ? (ScrollMore, EndScrollMore) : (ScrollMoreDisable, EndScrollMoreDisable).
- 2: if en: (Scrolling, EndScrolling); disabled: nothing.
- >= 3: nothing.

Cycle: owner sends state 2 when a scroll happens in this end's direction; when the Scrolling segment ends (0xf) the end asks its parent with notify 5; the owner answers with 0x7e7(1) or 0x7e7(0).

RE status: complete.


### 5. XuiScrollBar

Instance 0x30, create 0x92141878 (XuiScrollBar_CreateInstance; was misnamed DashMain), impl 0x92161ec0.

| Off | Ctor | Meaning |
| --- | --- | --- |
| +0x04 | 0 | Direction (setter 0x9213eb10 accepts 0..1): 0 vertical, 1 horizontal |
| +0x08, +0x0c | 0 | unused |
| +0x18 | 0 | item count |
| +0x1c | 0 | top |
| +0x20 | 0 | visible count |
| +0x24 | 10 | MinThumbSize |
| +0x28 | 0 | ScrollBarBody element |
| +0x2c | 0 | ScrollBarThumb element |

| Id | Behavior |
| --- | --- |
| 0x9 | clear +0x28/+0x2c; forward msg to base class level (Control init); handled; visual children by Id: "ScrollBarBody" -> +0x28, "ScrollBarThumb" -> +0x2c; UpdateThumb. |
| 0x7d6 | handled; plays Normal/EndNormal only (never shows focus). |
| 0x808 {count, top, visible} | handled; store; UpdateThumb. |
| 0x80d | handled; UpdateThumb (sent by element SetSize after 0x10). |

```
UpdateThumb (0x9215d880):
  if !thumb: return
  if count == 0 or visible == 0: SetShow(thumb, 0); return
  SetShow(thumb, 1); r = CalcThumbRect(); SetPosition(thumb, (r.x, r.y, 0)); SetSize(thumb, r.w, r.h)

CalcThumbRect (0x9215d578):
  if body: track.xy = Position(body).xy; track.wh = Size(body)
  else:    track.xy = (0,0);              track.wh = Size(scrollbar)
  t = track
  if thumb: tp = Position(thumb); ts = Size(thumb)
     vertical:   t.x = tp.x; t.w = ts.w
     horizontal: t.y = tp.y; t.h = ts.h
  (all counts converted as unsigned 32-bit to float)
  vertical:   len = track.h / count * visible; t.h = min(track.h, max(MinThumbSize, len))
  horizontal: len = track.w / count * visible; t.w = min(track.w, max(MinThumbSize, len))
  if count > 1 and count > visible:
     vertical:   ppi = (track.h - t.h) / (count - visible); t.y = top * ppi + t.y
                 if t.y + t.h > track.y + track.h: t.y = track.y + track.h - t.h
     horizontal: same on x/w
```
Named children: ScrollBarBody (optional), ScrollBarThumb (required for display). The scrollbar has no SmoothScroll property and handles no input; base XuiScrollBar never sends notify 0xe.

RE status: complete.


### 6. XuiSlider

Instance 0x4c, create 0x92141a38, construct 0x9215e6a8, impl 0x92162358.

| Off | Ctor | Meaning |
| --- | --- | --- |
| +0x04 | 0 | SliderBody element |
| +0x08 | 0 | ScrollMin element (XuiScrollEnd expected; receives 0x7e7) |
| +0x0c | 0 | ScrollMax element |
| +0x10 | 0 | RangeMin (setter 0x9213ee98, plain store) |
| +0x14 | 100 | RangeMax (setter 0x9213eef0, plain store) |
| +0x18 | 0 | Value (setter 0x9213ef48 -> SetValue) |
| +0x1c | 1 | Step (setter 0x9213efa0) |
| +0x20 | 0 | AccelInc |
| +0x24 | 0 | AccelTime (ms) |
| +0x28 | 0 | accel amount |
| +0x2c | 0 | accel sign (+1 / -1) |
| +0x30 | L"0" | value text, 12 wchar |
| +0x48 | 0 | bit0 Vertical (getter 0x9213f068 / setter 0x9213eff8), bit1 accelerating |

API: XuiSliderSetRange 0x9214e7e0 (min < max else E_INVALIDARG; stores both then SetValue(Value)), XuiSliderGetRange 0x9214e858, XuiSliderSetValue 0x9214e8d8, XuiSliderGetValue 0x9214e930, XuiSliderSetStep 0x9214e9a0 (store only).

#### Message table

| Id | Behavior |
| --- | --- |
| 0x9 init | clear +4/+8/+0xc; forward to base level; handled; visual children by Id "SliderBody", "ScrollMin", "ScrollMax"; set flag 0x20 on body subtree; UpdateVisual(1, 0, 0); UpdateScrollEnds. |
| 0x13 post-load | SetValue(Value). |
| 0xd kill focus | if accelerating: StopAccel. Not handled. |
| 0xe notify code 5 | if source is ScrollMin or ScrollMax: handled; UpdateScrollEnds. |
| 0x24 key down | see below. |
| 0x25 key up | if accelerating and the key is on the slider axis (horizontal: dir 1/2; vertical: dir 3/4): handled, StopAccel. |
| 0x27 timer {+0 id} | if id == 0xaabbccdd: handled; amount += AccelInc; SetValue(Value + sign * amount). |
| 0x7d6 | forward to base level (Control focus frames); handled; UpdateVisual(1, d+0, d+4); UpdateScrollEnds. |
| 0x7df get text | if d+0 == 0 and d+4 == 1: d+0xc = &text (+0x30), handled. TextPresenter with DataAssociation 1 shows the value. |

```
0x24: if !IsEnabled(self): not handled
  dir = XuiKeyToNavDirection(key, 0)
  dec = Vertical ? dir == 4 : dir == 1 ; inc = Vertical ? dir == 3 : dir == 2
  dec: if (Step > 0 and Value != Min) or (Step < 0 and Value != Max):
          if StartAccel(-1) == 0: SetValue(Value - Step)
          handled
  inc: if (Step > 0 and Value != Max) or (Step < 0 and Value != Min):
          if StartAccel(+1) == 0: SetValue(Value + Step)
          handled
  // at the bound, or Step == 0: not handled -> Control navigation moves focus

StartAccel(sign) (0x92156fa8):
  if AccelTime == 0: return 0
  if accelerating: return 1                      // key repeat consumed, no step
  sign = sign; amount = Step; flags |= 2; XuiSetTimer(h, 0xaabbccdd, AccelTime); return 0
StopAccel (0x92157018): if AccelTime != 0: XuiKillTimer(h, 0xaabbccdd); flags &= ~2

SetValue(v) (0x921621c0):
  if v < Min: v = Min ; if v > Max: v = Max        // Max wins if Min > Max
  old = Value
  if v < old and ScrollMin: if old == Max and ScrollMax: 0x7e7(ScrollMax, 0); 0x7e7(ScrollMin, 2)
  if v > old and ScrollMax: if old == Min and ScrollMin: 0x7e7(ScrollMin, 0); 0x7e7(ScrollMax, 2)
  if v != old: bubble notify 0xa {+0 old, +4 v} starting at self; v = extra+4 (handler may change it; not re-clamped)
  changed = v != Value; Value = v; text = decimal(v)
  UpdateVisual(0, 0, 0)
  if changed: bubble notify 0xb {+0 Value} starting at self

UpdateScrollEnds (0x9215e6f8):
  ScrollMin: 0x7e7(Value > Min ? 1 : 0); ScrollMax: 0x7e7(Value < Max ? 1 : 0)

UpdateVisual(playThumb, focusArg, focused) (0x9215e770):
  if !body: return
  focused = focusArg ? focused : XuiElementHasFocus(self); en = IsEnabled(self)
  if body has a timeline:
     (s, e) = XuiElementGetStateFrameRange(body, en, focused)
     if s >= 0 and e >= 0:
        f = e
        if Max > Min: f = (Value - Min) * (e - s) / (Max - Min) + s      // 32-bit signed, truncating
        XuiElementPlayTimeline(body, f, f, f, 1, 0)
  if playThumb: thumb = child of body with Id "SliderThumb"
     if thumb: (s, e) = XuiElementGetStateFrameRange(thumb, en, focused)
        if s >= 0 and e >= 0: XuiElementPlayTimeline(thumb, s, s, e, 1, 1)
```
The slider has no thumb position math: the designer animates the thumb inside SliderBody's state segment (Normal..EndNormal etc.), and the value seeks that timeline.

Named children: SliderBody (body timeline), SliderThumb (child of SliderBody), ScrollMin, ScrollMax.
Named frames: Normal, EndNormal, Focus, EndFocus, NormalDisable, EndNormalDisable, FocusDisable, EndFocusDisable (on SliderBody and SliderThumb); ScrollEnd frames on ScrollMin/ScrollMax.

RE status: complete.


### 7. XuiProgressBar

Instance 0x2c, create 0x921419c0, construct 0x9215e5d0, impl 0x921620a0.

| Off | Ctor | Meaning |
| --- | --- | --- |
| +0x04 | 0 | ProgressBody element |
| +0x08 | 0 | RangeMin (setter 0x9213ed90, store) |
| +0x0c | 100 | RangeMax (setter 0x9213ede8, store) |
| +0x10 | 0 | Value (setter 0x9213ee40 -> SetValue) |
| +0x14 | L"0" | value text, 12 wchar |

API: XuiProgressBarSetRange 0x9214e6b8 (min < max else E_INVALIDARG; stores only, no clamp and no visual update), XuiProgressBarSetValue 0x9214e730, XuiProgressBarGetValue 0x9214e788.

| Id | Behavior |
| --- | --- |
| 0x9 | clear +4; forward to base level; handled; visual child "ProgressBody" -> +4; set flag 0x20 on its subtree; UpdateVisual. |
| 0xb | handled, d+0 = 0 (never focusable) [INFERENCE: 0xb = can-focus query, Control answers shown and not disabled]. |
| 0x13 | SetValue(Value). |
| 0x7d6 | handled, UpdateVisual (no focus frames). |
| 0x7df | if d+0 == 0 and d+4 == 1: d+0xc = &text, handled. |

```
SetValue(v) (0x92162038): v = max(v, Min); v = min(v, Max); Value = v; text = decimal(v); UpdateVisual
   (no notifications)
UpdateVisual (0x9215e5f8):
  if !body or body has no timeline: return
  n = timeline +0x34 (0x92370258) [INFERENCE: last frame / frame count]
  f = n; if Max > Min: f = (Value - Min) * n / (Max - Min)     // 32-bit signed, truncating; start frame 0
  XuiElementPlayTimeline(body, f, f, f, 1, 1)
```
Named children: ProgressBody. No named frames; the fill is the ProgressBody timeline position.

RE status: complete. Timeline +0x34 is the timeline length in frames (xui-timelines.md section 1.3).


### Message and notify ids confirmed here

| Id | Data | Meaning |
| --- | --- | --- |
| 0x10 | {oldW, oldH, newW, newH} f32 | size changed (sent by element SetSize 0x921452c0 before 0x8 to children and 0x80d) |
| 0x13 | {u32} | post-load pass, after children (0x9214fb40) |
| 0x25 | key | key up |
| 0x27 | {+0 timer id} | timer |
| 0x7d0 / 0x7d1 | {u32} | get / set check state |
| 0x7d2 / 0x7d3 | {u32} | get / set enabled (Control +0x10 bit0) |
| 0x7d6 | {+0 bFocusArg, +4 focused} | refresh visual state |
| 0x7d7 | {out idx, out hItem} | get current item |
| 0x7d8 | {idx} | set current item |
| 0x7d9 | {idx, out h} | child control with data index idx (Control level) |
| 0x7da / 0x7db | {top} | get / set top item |
| 0x7dc | {kind, out} | counts: 0 items, 1 visible, 2 checked, 3 item controls, 4 lines, 5 per line |
| 0x7dd / 0x7de | {idx, count} | insert / delete items |
| 0x7df / 0x7e0 | 0x28 bytes | get / set item text {+0 idx, +4 assoc, +8 bItemData, +0xc text, +0x10 f32 point size (-1 = none), +0x20, +0x24 runs} |
| 0x7e1 / 0x7e2 | 0x1c bytes | get / set item image {+0 idx, +4, +8 bItemData, +0x10 path, +0x14, +0x18} |
| 0x7e3 / 0x7e4 | {idx, val} | get / set item check |
| 0x7e6 | {dir, hSource, out hTarget, out noFocusChange} | navigation query |
| 0x7e7 | {state} | scroll end state |
| 0x7e8 / 0x7e9 | {idx, val} | get / set item enable |
| 0x800 | {kind, h, out h, out flag} | tab stop search (0 next, 1 prev, 2 first, 3 last) |
| 0x803 / 0x804 | 0x14 bytes | get / set item nav data |
| 0x808 | {count, top, visible} | scrollbar range |
| 0x80d | - | post size change |
| notify 1 | {u8 user} | press |
| notify 2 / 3 | {hOther} | focus set / killed |
| notify 4 | {new, old} | selection changed (from list) |
| notify 5 | - | scroll end asks owner for steady state |
| notify 6 | {idx, checked} | item check changed (from list) |
| notify 0xa | {old, new} | value changing (slider), new may be replaced by a handler |
| notify 0xb | {value} | value changed (slider) |
| notify 0xe | {top} | scrollbar scrolled (handled by list; no base sender) |


### Nuances

- A plain XuiList has no item store: 0x7dc kind 0 is unanswered, so count stays 0 and every item control is hidden. Items come only from a subclass (XuiCommonList or dash classes) answering 0x7dc/0x7df/0x7e1/0x7e3/0x7e8. ListItem children placed directly under an XuiList in a scene are ordinary children, not items.
- Item controls are clones of the first XuiListItem in the list's Visual definition. The live template child is destroyed after its properties are read. Additional ListItem children in the visual are ignored.
- ScrollEnd and ScrollBar parts are reparented from the visual to the list itself and kept last in z-order (also after growth on resize).
- Selected-item enlargement only applies when the list has focus within or KeepSizeUnfocused is set, and only for Layout 0/1. The enlarged item is centered on the cross axis with floor(x + 0.5) rounding; following items are shifted by the extra size.
- Fit(): a trailing partial item counts as visible only if the leftover length is at least one full item (spacing excluded). Item size below 1.0 is clamped to 1.0.
- VisibleCount for variable sizes is not clamped by count; when nothing fits it returns min(count, ctrls) limited to 1.
- IsAtEnd for variable sizes always tests the y axis, even for Layout 1.
- SetTop writes the clamped value back into the message data, rounds down to a line start, and rejects negative values with E_INVALIDARG while still marking the message handled.
- MaxTop rounds UP to a line multiple; SetTop then rounds down. In a grid with a partial last row the last row is reachable.
- Page down rounds the new top up to a line multiple; page up does not round.
- 0x7e6 compares a slot index with an item index (lastLine) when deciding to scroll; preserve the comparison.
- Cross-axis right move in a grid calls SetCur(cur + 1) even at the last item; the clamp makes it a no-op and the key falls back to normal control navigation.
- After an item move that changed nothing, 0x7e6 stays unhandled and the list's own NavUp/NavDown/NavLeft/NavRight decide where focus goes.
- 0x7d8 always bubbles notify 4 when the index changed, even if focus could not move and cur was reverted.
- In SetTop, enabled state is pushed to every bound item unconditionally; in RefreshItems only on mismatch.
- On first population (old count 0) the scrollbar receives visible = 1, later updates use visible = ctrls (item control count, not the fitted visible count).
- When 0xc (set focus) reaches a list that has a current slot, focus is immediately forwarded to that item, unless FocusSelf is in progress.
- The kill-focus path of the list bubbles notify 3 when focus moves into its own item.
- Odd adjustment kept from the binary: when the item control count shrinks below the current slot, cur = slot - n + cur - 1.
- XuiCommonList delete frees nav records from idx to the end of the array (not only the deleted range) and does so even when the arguments are invalid. Index bounds for check and enable use the image array count.
- ItemsText/ItemsImage/ItemsNavPath segments must end with CR LF; text after the last CR LF is ignored.
- Non-checkable ListItems answer 0x7d0 with 0 and drop 0x7d1, and swallow key down while the user's press slot is set.
- ListItem "Press" frames are used only for non-checkable items; checkable items get Checkbox behavior.
- Slider notify 0xa lets a handler replace the new value without re-clamping. Notify 0xb is sent only if the stored value changed.
- Slider at a range bound (or Step 0) leaves the key unhandled, so the direction key navigates focus away.
- Slider acceleration: first press applies one Step immediately; each AccelTime tick adds AccelInc to the step amount (first tick moves Step + AccelInc). While accelerating, repeated key-down messages are consumed without a step.
- Slider and ProgressBar express value by seeking their body timeline. Slider maps into the current state segment (start..end frames), ProgressBar maps into 0..timeline+0x34.
- XuiProgressBarSetRange does not re-clamp or redraw; XuiSliderSetRange does both.
- ProgressBar answers 0xb with 0, so it never takes focus. ScrollBar plays only Normal for any state.

### Open questions

- Notify code 7 (extra {+0 u32, +8 u8}) is XuiControl's cancelable "pressing" notify ({+0 repeat, +4 out cancel, +8 user}, see "Press"); the list re-bubbles it with itself as source and copies the cancel result back to d+4.
- The exact contract of 0x92152368 when resolving the visual definition.
- Exact semantics of element +0xf0 bit 0x20 and of the +0x104..+0x110 reference block used by proportional anchoring (element layout).
- XuiElementPlayTimeline arguments (1,0) and (1,1) are loop = 1 with recurse = 0 or 1 (xui-timelines.md section 3).
- Who sends notify 0xe to a list (no sender in the base runtime; possibly dash classes).
- Bit 2 of list +0x98 (margin use) is never set in the observed XUI code; a dash subclass might set it.

## Text, edit, message box and sound classes

Scope: XuiEdit, XuiCaret, XuiLabel, XuiText, XuiTextPresenter, XuiMessageBox, XuiSound, XuiSoundXAudio. Everything below was read from disassembly unless marked [INFERENCE]. Message ids that are shared with other classes are described only as far as these classes use them. Functions listed here are renamed in the Binary Ninja database with an `RE status` comment.

Conventions: `S_OK` 0, `E_INVALIDARG` 0x80070057, `E_UNEXPECTED` 0x8000FFFF, `E_NOTIMPL` 0x80004001, `E_OUTOFMEMORY` 0x8007000E. "handled" means XUIMessage +8 is set to 1. "super" means the class forwards the message to its base-class level first (0x9213ced8 `XuiObjectGetBaseLevel` returns the next class-level node, and XuiSendMessage on it runs only the base procs).

### Shared facilities used by these classes

| Address | Name | Behavior | Status |
| --- | --- | --- | --- |
| 0x92155d48 | XuiSetTimer(h, id, periodMs) | Locked list of timer records (0x18 bytes: +0 h, +4 id, +8 period, +0xc start tick, +0x10 flags bit0 deleted, +0x14 next). Existing (h, id) record is reused: flags cleared, start tick = now, period replaced. One record per (h, id). | complete |
| 0x92155e00 | XuiKillTimer(h, id) | Sets deleted bit on the (h, id) record. | complete |
| 0x92155e90 | XuiMessageInitTimer | Message 0x27, data 4 bytes {+0 u32 timer id}. | complete |
| 0x925900a8 | tick count | Returns `*(KeTimeStampBundle + 0x10)` (ms tick). [INFERENCE: import at 0x92000c04 is the time stamp bundle] | partial |
| 0x9214ef78 | XuiMessageInitKey | id = 0x24 (key down) or 0x25 (key up, when arg r5 != 0). Data 0x10: +0 u32 virtual key, +4 u16 unicode char, +8 u32 flags, +0xc u8 user index. | complete |
| 0x9214efd0 | XuiMessageInitChar | id 0x26. Data 0xc: +0 u16 char, +4 u32 flags, +8 u8 user. XuiProcessKeystroke (0x92150120) bubbles 0x24 then, on key down only, 0x26 when the unicode field is nonzero. | complete |
| 0x9214c920 | XuiKeyToNavDirection(keyData, allowKeyboard) | Returns 1 left (0x5823/0x5812, VK 0x25), 2 right (0x5822/0x5813, VK 0x27), 3 up (0x5820/0x5810, VK 0x26), 4 down (0x5821/0x5811, VK 0x28), 5 (0x5806, VK 0x21), 6 (0x5807, VK 0x22), 7 (VK 0x09); 0 if flags bit 0x8 set. Keyboard VKs count only when `allowKeyboard` or global 0x92809cc8 bit0 clear. | complete |
| 0x921b5508 | XuiMessageInitGetSourceText(msg, data, idx, dataAssoc, arg8) | Message 0x7df, data 0x28 (layout below), +0x10 preset to -1.0. | complete |
| 0x9224a278 | XuiMessageInitSetSourceText(msg, data, a0, a4, a8, text) | Message 0x7e0, data 0x28: +0 a0, +4 a4, +8 a8, +0xc wchar* text. | complete |
| 0x921451b0 | XuiControlGetText(h) | Sends 0x7df {0,0,0} to h, returns data +0xc. Used by XuiControl.Text getter 0x92145a88. | complete |
| 0x92145208 / 0x921c8598 | XuiControlSetTextById / XuiControlSetText(h, text) | Send 0x7e0 {0,0,0,text} to h. 0x92145208 is used by the XuiControl.Text setter 0x92145a60. | complete |
| 0x921af9b8 | XuiMessageInitNotify | Message 0xe, data 0x10: +0 u32 notify code, +4 source handle, +8 extra. Code 1 = press (0x9215a758, extra u8 user, bubbled from the parent of the source). ScrollEnd sends a notify through 0x9219aea8 [INFERENCE: code 5, the code XuiEdit tests]. | partial |
| 0x92159178 | XuiScrollEndSetState(h, state) | Sends 0x7e7 {+0 u32 state} to a XuiScrollEnd. States used by XuiEdit: 0 none, 1 more content, 2 scrolling. | complete |

Message 0x7df (get source text) data layout, 0x28 bytes:

| Off | Dir | Meaning |
| --- | --- | --- |
| 0x00 | in | u32 index (0 from text presenters) |
| 0x04 | in | u32 DataAssociation of the requesting presenter |
| 0x08 | in | u32 (0) |
| 0x0c | out | wchar* text |
| 0x10 | out | f32 point size override; -1.0 (preset) means "use presenter PointSize" |
| 0x14 | in | u32 extended request; XuiTextPresenter sets 1 |
| 0x18 | out | f32 vertical offset passed to the text draw |
| 0x1c | in/out | u32 TextStyle (presenter puts its own; owner may replace) |
| 0x20 | out | u32 run count |
| 0x24 | out | pointer to runs, 16 bytes each {i32 start, i32 end, u32, u32} |

XuiControl (0x9215aa68) answers 0x7df only when +0 == 0 and +4 == 0: +0xc = Control.Text (+0x2c), +0x10 = Control.PointSize (+0x30, default -1.0). It answers 0x7e0 only when +0 == 0 and +4 == 0: compares with current text (wcscmp), on change increments a counter at +0x3c (skips 0 on wrap) and stores a copy.

### XuiText

Registration 0x92147f28, base XuiElement. ObjectProc thunk 0x9213d968 -> impl 0x9213c1a0 `XuiText_ObjectProcImpl`. Create 0x92144898 (0x24 bytes), destroy 0x921413d8.

Instance (0x24): +0x00 hObj; +0x04 text property block (shared layout with XuiTextPresenter, constructed by 0x92141ee8 `XuiTextProps_Construct`): +0x04 TextColor, +0x08 DropShadowColor, +0x0c PointSize f32, +0x10 Font wchar*, +0x14 cached font object, +0x18 TextStyle, +0x1c LineSpacingAdjust; +0x20 Text wchar*.

Property setters (all others are plain field writes): PointSize 0x9213e8d0 -> 0x9213c078, Font 0x9213e0d0 -> 0x9213e040, TextStyle 0x9213e130 -> 0x9213c100. Each recreates the font object only if one already exists: PointSize and Font always, TextStyle only when bits 0xe change. The new font is created with 0x92182608(fontName, style, 0, &out, pointSize); the old one is released with 0x92181e68. On create failure the property is not stored.

| Msg | Behavior |
| --- | --- |
| 0x0 (render) | handled. If no font object and Text != null: create font (0x92182608). On success or if font exists: draw with 0x9215c080(&props, hObj, Text, TextStyle, 0 runs, renderData = msg data, yOffset 0.0). |
| 0x28 | If data bit 0x4: release font object. Not handled. |
| 0x29 | If no font object and Text != null: create font. Not handled. |

Text has no message to change Text; it is a property only. A font is never created while Text is null.

RE status: complete (draw internals belong to the renderer notes).

### XuiTextPresenter

Registration 0x921485e8, base XuiElement. Thunk 0x9213d9d0 -> impl 0x9215c9f0 `XuiTextPresenter_ObjectProcImpl`. Create 0x921449e0 (0x2c bytes, ctor 0x92142f88), destroy 0x92144a58.

Instance (0x2c): +0x00 hObj; +0x04..+0x1f text property block as XuiText (no Text field); +0x20 f32 point size used for the cached font (init -1.0); +0x24 cached owner control (element record, 0 = not looked up); +0x28 DataAssociation.

Helpers used by other classes: 0x9214cec0 `XuiTextPresenterGetFont(h)` (creates the font from the presenter's own Font/TextStyle/PointSize if missing), 0x9214cf78 GetTextStyle, 0x9214cf28 SetTextStyle (field write, no font rebuild), 0x9214cfb0 GetLineSpacingAdjust.

Source text lookup, 0x9215c820 `XuiTextPresenter_GetSourceText(tp, &text, &pointSize, &style, &yOffset, &runCount, &runs)`:

```
if yOffset: *yOffset = 0.0
if style: *style = tp.TextStyle
if tp.owner == 0: tp.owner = XuiFindOwnerControl(tp.hObj)   // 0x92157c60: nearest ancestor that has a XuiControl level
if tp.owner == 0:
    resolver = XuiGetResolver(); if resolver: resolver->vtbl[2](resolver, tp.hObj, &text)
    *pointSize = tp.PointSize; return S_OK
msg = 0x7df {idx 0, dataAssoc tp.DataAssociation, 0}; data.style = tp.TextStyle; data.extended = 1
r = XuiSendMessage(owner, msg); if r < 0 return r
*text = data.text
*pointSize = (data.pointSize == -1.0) ? tp.PointSize : data.pointSize
*yOffset = data.yOffset ; *style = data.style
if data.runCount != 0 or data.runs != 0:
    if text == 0 or runCount == 0 or runs == 0: return E_INVALIDARG
    for i: require runs[i].end >= runs[i].start and (i == last or runs[i+1].start > runs[i].end) else E_INVALIDARG
    *runCount = data.runCount ; *runs = data.runs
```

| Msg | Behavior |
| --- | --- |
| 0x0 (render) | handled. GetSourceText. If no font, or cached size (+0x20) != pointSize, or (TextStyle ^ style) & 0xe: if text non-empty, store TextStyle = (style & 0xe) \| (TextStyle & ~0xe), +0x20 = pointSize, create font, replace cached font. Then if font and text non-empty: draw 0x9215c080(&props, hObj, text, style, ?, runCount, runs, renderData, yOffset). |
| 0x5 (init) | Clear cached owner (+0x24). Not handled. |
| 0x28 | Data bit 0x4: release font. |
| 0x29 | Same font refresh as render without drawing. |

Push model: nothing pushes text into a presenter. The presenter pulls text from its owner control with 0x7df on every render. Controls change text by handling 0x7e0. A presenter with DataAssociation != 0 receives nothing from a plain XuiControl (list classes answer those). There are no MaxFlowLines or flow properties; wrapping and alignment are TextStyle bits (renderer domain). Bits 0xe of TextStyle select the font variant (passed to 0x92182608).

RE status: complete for behavior; draw (0x9215c080) not analyzed here.

### XuiLabel

Registration 0x92147fe8, base XuiControl, no properties. ObjectProc 0x9213d970 `XuiLabel_ObjectProc` (inline, no thunk). Create 0x92144900 (4 bytes {hObj}), destroy 0x9213da38 (free).

| Msg | Behavior |
| --- | --- |
| 0xb (query focusable) | handled, data +0 (BOOL) = 0. |

All other behavior is XuiControl. Text reaches the visual through the generic path: the skin visual `XuiLabel` holds a XuiTextPresenter `Text` (TextStyle 16385 in dashuisk/skin.xui) which pulls Control.Text via 0x7df.

RE status: complete.

### XuiCaret

Registration 0x92148d78, base XuiControl, no properties. Thunk 0x9213da30 -> impl 0x92158398 `XuiCaret_ObjectProcImpl`. Create 0x92144900, destroy 0x9213da38.

| Msg | Behavior |
| --- | --- |
| 0xb | handled, data +0 = 0 (not focusable). |

There is no blink timer anywhere in the caret or edit code. Visibility is set by XuiEdit (Show = !ReadOnly && Enabled, see below) and XuiEdit forwards its control-state message 0x7d6 to the caret, so any blink must come from the caret visual's timeline named frames. dashuisk/skin.xui has no `XuiCaret` visual; its only XuiCaret element is `Caret` (10x28) inside the `XuiEdit` visual.

RE status: complete.

### XuiEdit

Registration 0x92148cb8, base XuiControl. Thunk 0x9213da28 -> impl 0x9216a298 `XuiEdit_ObjectProcImpl`. Create 0x92144a98 (0xd0 bytes, ctor 0x9213f0a8), destroy 0x92144b10 (dtor 0x9213f1d0 frees +0x5c, +0x64, +0x68, +0x78, +0x4c and the range list +0x98; it does not free +0x44 and does not kill the timer). Exported helpers without internal callers: 0x92168ce0 `XuiEditSetTextLimit(h, n)`, 0x92158360 `XuiEditGetTextLimit(h)`.

#### Instance (0xd0)

| Off | Type | Meaning |
| --- | --- | --- |
| 0x00 | handle | hObj |
| 0x04 | arrow rec | ScrollLeft: {+0 handle, +4 current state, +8 requested state, +0xc u8 ready} (0x10 bytes) |
| 0x14 | arrow rec | ScrollRight |
| 0x24 | arrow rec | ScrollUp |
| 0x34 | arrow rec | ScrollDown |
| 0x44 | ptr | source highlight runs (16 bytes each); no writer found in the binary, zeroed by ctor |
| 0x48 | u32 | source run count |
| 0x4c | ptr | display runs (copy of source runs shifted by -firstChar) |
| 0x50 | u32 | display run capacity; reported as the run count in 0x7df |
| 0x54 | handle | child `Caret` |
| 0x58 | handle | child `Text` (must be a XuiTextPresenter) |
| 0x5c | wchar* | text buffer; holds 2*capacity wchars: text at [0], display copy at [capacity] |
| 0x60 | u32 | TextStyle read from `Text` |
| 0x64 | wchar* | AllowedChars string |
| 0x68 | wchar* | PasswordChar {char, 0} |
| 0x6c | u32 | caret offset relative to firstChar |
| 0x70 | u32 | first visible line |
| 0x74 | u32 | firstChar: index of the first displayed character |
| 0x78 | u32* | line start indices (from wrap) |
| 0x7c | u32 | line count |
| 0x80 | i32 | LineSpacingAdjust of `Text` |
| 0x84 | u32 | font line height (0x92180208) |
| 0x88 | u32 | line pitch = 0x80 + 0x84 |
| 0x8c | u32 | buffer capacity in wchars |
| 0x90 | u32 | TextLimit (default 0xfffe) |
| 0x94 | u32 | flags: bit0 ReadOnly, bit1 Multiline, bit2 SmoothScroll |
| 0x98 | ptr | AllowedChars range list, nodes {u16 lo, u16 hi, next*} (8 bytes) |
| 0x9c | f32 | scroll position (px) |
| 0xa0 | f32 | scroll target (px) |
| 0xa4 | f32 | maximum scroll position |
| 0xa8 | f32 | scroll velocity (signed) |
| 0xac | f32 | start speed, lines per second (3.0) |
| 0xb0 | f32 | acceleration per step (0.02) |
| 0xb4 | f32 | maximum speed, lines per second (20.0) |
| 0xb8 | u32 | last step tick |
| 0xbc | u8 | scroll timer active |
| 0xc0, 0xc4 | f32 | `Text` width, height |
| 0xc8, 0xcc | f32 | measured text height, width |

Arrow records are ready (byte = 1) after construction.

#### Properties

| Property | Field | Setter | Effect |
| --- | --- | --- | --- |
| TextLimit | 0x90 | 0x9213f228 -> 0x921683b0 | Stores limit. If limit < capacity: buffer[limit] = 0 (display copy not rebuilt). If limit < caret: caretRel = limit - firstChar. UpdateView. |
| AllowedChars | 0x64 | 0x9213f280 -> 0x92158088 | Frees string. If new != null: parse into range list (0x92156de0); on success copy string. Null keeps the old range list. Parse failure leaves the list empty (all chars allowed) and the string null. |
| PasswordChar | 0x68 | 0x9213f2d8 -> 0x92168340 | Frees old; if new != null stores {new[0], 0}. UpdateView only; display copy is rebuilt at the next rebuild. |
| ReadOnly | 0x94 bit0 | 0x9213f330 -> 0x9215e8f0 | E_NOTIMPL when clearing while Multiline or SmoothScroll set. Caret Show = !value. |
| Multiline | 0x94 bit1 | 0x9213f388 -> 0x92168218 | E_NOTIMPL when clearing while SmoothScroll set. Setting also sets ReadOnly. Text style: set -> (s & ~0x30) \| 0x2000; clear -> (s \| 0x30) & ~0x2000 (clearing leaves ReadOnly). UpdateView. |
| SmoothScroll | 0x94 bit2 | 0x9213f3e0 -> 0x921682d8 | Set: ReadOnly(1), Multiline(1), bit2 = 1. Clear: bit2 = 0. UpdateView. |

Property application order (table order) means a XUR that sets SmoothScroll and then nothing else is consistent; clearing ReadOnly after Multiline fails silently with E_NOTIMPL.

AllowedChars syntax (0x92156de0): characters are literal; `a-b` adds range a..b. A `-` as the last character is literal. `-` after another `-`, at the start, or a range with lo > hi frees the list and returns E_INVALIDARG.

```
pending = 0; inRange = false
for i, c in str:
    if c == '-' and i != len-1:
        if inRange or pending == 0: fail
        inRange = true
    elif inRange:
        if pending > c: fail
        add(pending, c); pending = 0; inRange = false
    else:
        if pending: add(pending, pending)
        pending = c
if pending: add(pending, pending)
```

#### Editing model

Text is one wchar buffer. SetText (0x92168c30) allocates `(len+50)*4` bytes, capacity = len + 50, copies text, resets caretRel, first line, firstChar to 0, runs Layout then UpdateView. Text longer than TextLimit returns E_INVALIDARG without change. Null text frees the buffer.

The display copy (0x921580f8 `XuiEdit_BuildDisplayText`) is written at buffer[capacity]: text[firstChar..] or, with PasswordChar, the password char repeated (len - firstChar) times, then 0. Single-line horizontal scrolling is in steps of 5 characters by moving firstChar.

Caret index = firstChar + caretRel. All editing operations require `XuiControlIsEnabled` (0x922215d8, message 0x7d2) and fail silently otherwise; insert, delete, set caret also require !ReadOnly.

```
InsertText(idx, s, n):           // 0x92169fa0; no bound check on idx
    if ReadOnly or !Enabled: return E_UNEXPECTED
    len = buf ? wcslen(buf) : 0 ; newLen = len + n
    if newLen > TextLimit: return E_INVALIDARG
    if capacity <= newLen: buf = realloc(buf, (newLen+50)*4); capacity = newLen+50; buf[len] = 0
    move buf[idx..len] (with terminator) up by n ; copy s into buf[idx..]
    BuildDisplayText
    caret = firstChar + caretRel ; if idx <= caret: SetCaretPosition(caret + n)
    UpdateView

DeleteText(idx, n):              // 0x9216a100
    if ReadOnly or !Enabled: return
    if idx + n <= len: move buf[idx+n..len] down to idx; BuildDisplayText
        caret = firstChar + caretRel ; if caret > idx: SetCaretPosition(caret - n)   // unsigned; fails when caret was inside the range
        UpdateView

Backspace (0x9216a1d0): if caret != 0 and Enabled: DeleteText(caret-1, 1)
InsertChar (0x9216a240): if Enabled: InsertText(caret, &ch, 1)

SetCaretPosition(pos):           // 0x92169518
    ReadOnly or !Enabled or no Text child: E_UNEXPECTED ; no buffer or pos > len: E_INVALIDARG
    while pos < caret: MoveLeft(animate=1) (stop on error)
    while pos > caret: MoveRight(animate=1)

MoveLeft(animate):               // 0x92168a00
    fail E_UNEXPECTED if no buffer, !Enabled, or caretRel == 0 and firstChar == 0
    if ReadOnly:
        if Multiline or firstChar == 0: fail
        ShiftViewLeft(animate)
    else:
        if caretRel == 1 and firstChar > 0: ShiftViewLeft(animate)
        caretRel -= 1
    UpdateView

MoveRight(animate):              // 0x92168ae0
    fail if no buffer, !Enabled, caret >= wcslen(buf), or no font
    disp = buf + capacity ; dlen = wcslen(disp)
    if ReadOnly:
        if Multiline: fail
        if measure(disp, all).right > TextWidth: ShiftViewRight(animate) else fail
    else:
        caretRel += 1
        if measure(disp, min(dlen, caretRel+1)).right > TextWidth: ShiftViewRight(animate)
    UpdateView

ShiftViewLeft (0x921683f8):  if Enabled: firstChar -= 5 ; caretRel = ReadOnly ? 0 : caretRel + 5 ; rebuild; UpdateView ; if animate: ScrollLeft.req = 2, UpdateScrollArrow
ShiftViewRight (0x92168498): firstChar += 5 ; caretRel = ReadOnly ? 0 : caretRel - 5 ; same, using ScrollRight
```

There is no text-changed notification. No message is sent by insert, delete, backspace or SetText.

#### Layout and view

`XuiEdit_FindVisualChildren` (0x9215eaa0): looks in the control's visual (first child, must be XuiVisual) for direct children by Id: `Caret` (+0x54), `Text` (+0x58), `ScrollLeft` (+0x04), `ScrollRight` (+0x14), `ScrollUp` (+0x24), `ScrollDown` (+0x34). Applies the Multiline TextStyle rule to `Text`. Caret Show = !ReadOnly && Enabled.

`XuiEdit_Layout` (0x92162a18):

```
font = XuiTextPresenterGetFont(Text)
if Text and font: style = Text.TextStyle; spacing = Text.LineSpacingAdjust; fontH = lineHeight(font); pitch = spacing + fontH; (w, h) = size(Text)
else: w = h = 0; style = 0; fontH = 0; spacing = 0          // pitch keeps its old value
BuildDisplayText
if buf and Text and font: WrapLines -> lineStarts, lineCount   // 0x92184048 on the raw text, rect (0,0,w,h)
maxScroll = pitch ? max(0, lineCount*pitch - pitch*floor(h/pitch)) : 0
if buf and Text and font: r = measure(display, all, rect (0,0,w,h)); textH = r.bottom - r.top; textW = r.right - r.left
else textH = textW = 0
if Multiline: show = textH > h ; Show(ScrollUp, show); Show(ScrollDown, show); Show(ScrollLeft, 0); Show(ScrollRight, 0)
UpdateView
if Multiline and maxScroll > 0:
    ScrollDown.ready = 1; ScrollDown.req = !(|pos - maxScroll| < 0.001); UpdateScrollArrow
    ScrollUp.ready = 1;   ScrollUp.req   = !(|pos| < 0.001);          UpdateScrollArrow
```

Line starts (`XuiEdit_WrapLines` 0x9215ea08): when buffer, `Text` and font exist it frees the old list and calls `XuiFontGetLineStarts` 0x92184048(font, text, -1, style, spacing, 1, rect (0,0,w,h), &count, &starts). That locked wrapper adds the default language flags when `style & 0x3f0000` is 0 and calls `XuiFont_BuildLineStarts` 0x92183cb8: the count comes from the 0x2000 measurer (0x92182f30, every line regardless of the rect height), then `XuiFont_NextLine` runs count times and each start index is stored; a line that does not advance moves on by one character. Otherwise the old list and count stay, also when the text is set to null. The line height is `XuiFontGetLineHeight` 0x92180208: `(u32)ceil(font line height)`. pitch = LineSpacingAdjust + that height, without the clamp of `XuiFont_LinePitch`.

`XuiEdit_UpdateView` (0x921626f0):

```
font = XuiTextPresenterGetFont(Text); if none return
disp = buf ? buf + capacity : null
if Caret and !ReadOnly and Enabled:
    caretPos = position(Caret)
    r = measure(disp, caretRel chars, style, spacing)
    caretPos.x = r.right ; setPosition(Caret, caretPos)      // Text position is read but not added
firstLine = (u32)(i32)pos / pitch                             // trap if pitch == 0
if Multiline and lineStarts and lineStarts[firstLine] != firstChar: firstChar = lineStarts[firstLine]; BuildDisplayText
if !Multiline:
    r = measure(disp, all, rect (0,0,w,h)) ; overflow = r.right > w
    vis = overflow or firstChar > 0
    Show(ScrollLeft, vis);  ScrollLeft.req  = firstChar != 0; UpdateScrollArrow
    Show(ScrollRight, vis); ScrollRight.req = overflow;       UpdateScrollArrow
    Show(ScrollUp, 0); Show(ScrollDown, 0)
if source runs (+0x44): grow +0x4c to count entries when count > +0x50 (+0x50 = count); copy each run with start and end reduced by firstChar
else free +0x4c, zero +0x4c and +0x50
```

`firstLine` is stored at +0x70. Positions convert as `fctiwz` (truncation to i32) followed by an unsigned divide (`divwu`); the same conversion is used by 0x7ed and 0x7ef mode 1. SetText resets +0x6c, +0x70 and +0x74 but not the scroll position +0x9c, so a shorter text with an old position reads `lineStarts[firstLine]` past the list (exui-runtime leaves firstChar unchanged then).

`XuiEdit_UpdateScrollArrow(rec)` (0x9215e970):

```
if rec.h == 0 or rec.cur == rec.req: return
if req == 2: cur = 2; ready = 0; XuiScrollEndSetState(h, 2)
elif req in (0, 1): if ready: cur = req; ready = 1; XuiScrollEndSetState(h, req)
```

Message 0xe code 5 whose source equals an arrow handle sets that arrow's ready byte to 1 and calls UpdateScrollArrow, so states requested during a "scrolling" pulse wait until the arrow reports.

#### Vertical scrolling (ReadOnly + Multiline only)

All of ScrollUp, ScrollDown, ScrollToLine return E_UNEXPECTED unless buffer exists and ReadOnly and Multiline; ScrollUp/Down also need Enabled.

```
ScrollUp(animate):    // 0x92168638 ; ScrollDown 0x92168820 is the mirror (bound maxScroll, sign +, ScrollDown arrow)
    if |pos| < 0.001: return 0x80070019
    if !animate:
        pos = target = max(pos - pitch, 0) ; v = -1.0
        ScrollUp.req = !(|pos| < 0.001)
    else:
        if SmoothScroll: target = 0 ; if v >= 0: v = -(pitch * startSpeed)
        else: target = max(target - pitch, 0) ; v = -pitch
        StartScrollTimer ; ScrollUp.req = 2
    UpdateScrollArrow(ScrollUp); ScrollDown.req = 1; UpdateScrollArrow(ScrollDown); UpdateView

ScrollToLine(line, animate):   // 0x92168538 ; no clamp to maxScroll
    if line >= lineCount: E_INVALIDARG
    target = line * pitch
    if animate: v = (target >= pos ? +1 : -1) * startSpeed * pitch ; StartScrollTimer
    else pos = target
    UpdateView

StartScrollTimer (0x921581d8): if !active: lastTick = now; XuiSetTimer(h, 0xaabbccdd, 16); active = 1

SmoothScrollStep (0x92162d58), on timer 0xaabbccdd:
    if SmoothScroll and v != 0:
        dt = now - lastTick (if now < lastTick: now - lastTick - 1) ; dt = min(dt, 33.333) ; lastTick = now
        pos += dt * 0.001 * v
    else pos += v
    if v > 0 and pos >= target: pos = target; v = 0; KillTimer; active = 0; ScrollDown.req = !(|pos - maxScroll| < 0.001); UpdateScrollArrow
    if v < 0 and pos <= target: pos = target; v = 0; KillTimer; active = 0; ScrollUp.req = !(|pos| < 0.001); UpdateScrollArrow
    if v was nonzero: UpdateView
    if SmoothScroll and v != 0: v = v * (1 + accel) clamped to +-(pitch * maxSpeed)
```

With SmoothScroll the velocity is pixels per second (3 lines/s start, +2% per 16 ms step, 20 lines/s cap). Without it the velocity is pixels per step. Key up (0x25) of up/down and message 0x7e6 snap the target to a line boundary: moving up (v < 0) target = floor(pos/pitch)*pitch; moving down (v > 0) target = min(maxScroll, (floor(pos/pitch)+1)*pitch).

#### Message table

| Msg | Data | Behavior |
| --- | --- | --- |
| 0x9 | none | super; FindVisualChildren; Layout; UpdateView; handled. (broadcast by dash code, e.g. DashApp_ApplyProfileStyle) |
| 0xe | notify {code, hSource} | code 5 and hSource is an arrow: ready = 1, UpdateScrollArrow, handled. |
| 0x13 (init complete) | u32 | If the control has no visual: send 0x29 to self and to the stored Caret handle (+0x54; still 0 unless an earlier 0x9 or 0x13 ran FindVisualChildren). FindVisualChildren; Layout. Not handled. |
| 0x24 (key down) | key | Unicode != 0: handled, nothing else. Buffer null: not handled. Direction 1 MoveLeft(1), 2 MoveRight(1), 3 ScrollUp(1), 4 ScrollDown(1): handled when result >= 0, else falls through to XuiControl navigation. VK 0x0c: SetText(null), handled. |
| 0x25 (key up) | key | Unicode != 0: handled. Buffer null: not handled. Direction 3/4: target snap (above), handled. |
| 0x26 (char) | char | Always handled. 0x08 Backspace. 0x09, 0x0d, 0x1b and < 0x20 ignored. If a range list exists and the char is in no range: ignored. Else InsertChar. |
| 0x27 (timer) | {id} | id 0xaabbccdd: SmoothScrollStep, handled. |
| 0x2c | {+0 line, +4 out} | handled. line >= lineCount: E_INVALIDARG. +4 = Multiline ? lineStarts[line] : firstChar. |
| 0x2d | {+0 idx, +4 wchar*} | handled. null text: E_INVALIDARG; else InsertText(idx, s, wcslen(s)). |
| 0x2e | {+0 idx, +4 count} | handled. DeleteText. |
| 0x7d6 (state frames) | foundation | super, then the same message is sent to Caret; handled. |
| 0x7df (get source text) | above | handled. +0xc = extended ? (buf ? buf + capacity : 0) : buf. If extended: +0x18 = pos - pitch*firstLine; if !ReadOnly: style &= ~0x4000; +0x20 = +0x50; +0x24 = +0x4c. Does not check +0/+4. Point size stays -1.0. |
| 0x7e0 (set source text) | +0xc text | SetText(+0xc); handled only on success, so a text over TextLimit falls through to XuiControl, which stores it in Control.Text. |
| 0x7e6 (get nav) | foundation | Target snap; not handled. |
| 0x7ea | {+0 out} | handled. +0 = firstChar + caretRel. |
| 0x7eb | {+0 pos} | handled. SetCaretPosition. |
| 0x7ed | {+0 out, +4 BOOL current} | handled. +0 = (u32)fctiwz(current ? pos : target) / pitch (trap on a zero pitch). Dash helper `XuiEditGetTopLine` 0x92272f90. |
| 0x7ee | {+0 line, +4 BOOL animate} | handled. ScrollToLine. |
| 0x7ef | {+0 mode, +4 out, +8 BOOL current} | handled. mode 0: lineCount. mode 1: signed min(fctiwz(h/pitch), lineCount - (u32)fctiwz(current ? pos : target)/pitch), trap on a zero pitch. mode 2: fctiwz(h/pitch). Else E_INVALIDARG. Dash helpers `XuiEditGetLineCount` 0x92272ee0 (mode 0), `XuiEditGetPageLines` 0x92272f38 (mode 2). |
| 0x7f2 | {+0 0, +4 out BOOL, +8, +0xc, +0x10 out f32} | handled. +4 = SmoothScroll; +8 start speed; +0xc accel; +0x10 max speed. +0 != 0: E_INVALIDARG. |
| 0x7f3 | same, in | handled. Requires +0 == 0, +0x10 > 0, +8 > 0, +0xc >= 0 else E_INVALIDARG. SetSmoothScroll(+4), then store the three values. |
| 0x80d (size changed) | none | Layout; UpdateView. Not handled. |

Virtual keyboard: no XUI library code opens one. XamShowKeyboardUI (0x927397e4, thunk 0x921147d0) is called only from dash application functions (0x921bc7a0, 0x921def38, 0x921e2c70, 0x921e69f0, 0x92254db0, 0x9225f4e0, 0x92263338, 0x922704d8, 0x92276bb8, 0x92297858). In the dash, every XuiEdit in the XUI files is ReadOnly + Multiline + SmoothScroll (visuals `edit_NoCaret`, `edit_NoCaret_Focus`, `edit_AutoScroll`).

RE status: partial. Behavior, layout, messages and state machines are complete. Line starts and line height are traced (above). Not analyzed: the measure wrapper 0x92183eb8, and the writer of the source highlight runs (+0x44/+0x48), for which none was found.

exui-runtime (`control/edit.rs`) implements the buffer, TextLimit, AllowedChars, character input, caret index moves, line starts, the first line and first character of the view, the 0x7df display copy with its vertical offset (drawn as the text scroll of `XuiFont_DrawTextLines`, rebuilt where `XuiEdit_BuildDisplayText` runs), 0x7ed, 0x7ee, 0x7ef, 0x7f2, 0x7f3, 0x80d and the smooth scroll step on the XuiEdit level timer. AllowedChars of a loaded XUR is parsed on 0x5, since the loader stores properties without their setters. Not implemented: caret placement, horizontal view shifts, scroll arrows, password characters, up and down key scrolling and the 0x2c to 0x2e messages.

### XuiMessageBox

Registration 0x92148e28, base XuiScene, required-children string 0x9200baa0: `[element][default]|MessageText|XuiEdit;[element][default]|Button0|XuiButton;[element]|Button1|XuiButton;[element]|Button2|XuiButton;[element]|Button3|XuiButton;[element]|Button4|XuiButton;`. Thunk 0x9213da60 -> impl 0x9215f5e8. Create 0x92144b50 (0x10 bytes), destroy 0x9213da38.

Instance (0x10): +0x00 hObj; +0x04 owner handle (receives the result); +0x08 element focused before the box opened (0 = none); +0x0c u8 user index of that focus (0..3, 0xff any user, 0xfe not focused).

API `XuiShowMessageBoxEx` (0x9215f8e8), args r3 visualName, r4 hParent, r5 hOwner, r6 title, r7 message text, r8 button count, r9 wchar*[] button texts, r10 default button index, stack +0x194 flags, stack +0x19c out handle. `XuiShowMessageBox` (0x9215fb38, no callers) = Ex(null, null, r3 owner, r4 title, r5 text, r6 count, r7 texts, 0, flags 0, out null).

```
if out: *out = 0
if count - 1 > 8 (unsigned): E_INVALIDARG          // 1..9 buttons
if default < 0 or default > count - 1: E_INVALIDARG
if hParent == 0: hParent = topmost ancestor of hOwner
if visualName == 0: visualName = "XuiMessageBox" + digit(count)     // "XuiMessageBox " with the last char replaced
look up the visual (0x92152368 with context from bubbling 0x22 from hParent)    [INFERENCE: 0x22 returns the skin context]
XuiCreateObject("XuiMessageBox") ; set property "Visual" = visualName ; add as child of hParent (0x9214d428)
broadcast 0x29 to the box
Setup ; on any failure destroy the box
if out: *out = box
```

`XuiMessageBox_Setup` (0x9215f6d8):

```
visual = XuiControlGetVisual(box) ; body = first child of visual ; if none: 0x80300028
for i in 0..count-1:
    b = direct child of body with Id "Button<i>" ; if none: return 0x80300028 (message text not set)
    XuiControlSetText(b, texts[i]) ; if i == default: defButton = b
if body has child "MessageText": XuiControlSetText(it, text)
XuiControlSetText(body, title)
if flags & 1: position(box) = ((parentW - bodyW) * 0.5, (parentH - bodyH) * 0.5, 0)
box.owner = hOwner
box.prevFocus = XuiElementGetFocusOwner(hOwner)   // focus element that is hOwner or inside it, any-user slot first, then users 0..3
box.user = prevFocus ? XuiElementGetFocusUser(prevFocus) : 0xff
focus first focusable element in defButton's subtree for box.user (0x92152ca0)
```

| Msg | Behavior |
| --- | --- |
| 0xe (notify) | code 1 (press) and source Id is exactly "Button" + one digit 0..9: handled; XuiDestroyObject(box); restore focus to prevFocus for the saved user (0x921515a0); send 0x7ec {+0 u32 digit} to owner with XuiSendMessage (not bubbled). |

Extra ButtonN children beyond `count` are not hidden. The result index is taken from the Id digit, not from the order used at setup. Destroy happens before the result is sent, and the owner handle is not validated.

RE status: complete for the proc and the API. The visual lookup 0x92152368/0x92150f70 is not analyzed.

### XuiSound

Registration 0x92148758, base XuiElement. Thunk 0x9213d9e0 -> impl 0x9216fc30 `XuiSound_ObjectProcImpl`. Create 0x92141760 (0x10 bytes, ctor 0x9216fbe0), destroy 0x9213da38.

Instance (0x10): +0x00 hObj; +0x04 State (u32, default 2); +0x08 flags: bit0 state apply pending, bit1 inside an XuiVisual, bit2 Loop, bit3 Finish, bit5 playing; +0x0c Volume f32 dB (default -12.0).

| Property | Field | Setter | Effect |
| --- | --- | --- | --- |
| State | +0x04 | 0x9216f940 | Type Unsigned required. On change: store and ScheduleStateApply. |
| Loop | +0x08 mask 0x4 | none | plain bit |
| Finish | +0x08 mask 0x8 | none | plain bit. No code reads it (no reference to the bit in XuiSound or XuiSoundXAudio). |
| Volume | +0x0c | 0x92146148 | Sends 0x7fa {f32} to the element. |

State bits: 0x2 play, 0x4 stop, 0x8 wait until current playback is done. The dash XUI files use 4 (with File set) or the default 2.

`XuiSound_ScheduleStateApply` (0x9216f610): if !pending: XuiSetTimer(h, 0xaabbccdd, 0); pending = 1. `XuiSoundScheduleStateApply(h)` (0x9216f678) finds the XuiSound level and calls it.

| Msg | Behavior |
| --- | --- |
| 0x5 (init) | handled. Clear bit1; walk parents; set bit1 if any ancestor's class name is "XuiVisual". |
| 0x13 | handled (nothing else). |
| 0x27, id 0xaabbccdd | handled. State machine below. |
| 0x7f9 | handled. data f32 = Volume. |
| 0x7fa | handled. If -96.0 <= v <= 6.0: Volume = v, else E_INVALIDARG. |

```
on timer:
    if pending:
        if State & 8 and !(QueryDone() & 1): return           // keep pending, keep timer
        if State & 2:   r = send 0x7f4 to self ; if r >= 0: playing = 1 ; if r >= 0: pending = 0
        elif State & 4: r = send 0x7f5 to self ; if r >= 0: playing = 0 ; if r >= 0: pending = 0
        else pending = 0
    if Loop and playing:
        if QueryDone() & 1: send 0x7f4 ; playing = 1 on success
    elif !pending: XuiKillTimer(h, 0xaabbccdd)
QueryDone = send 0x7f6 {u32 0} to self, return data
```

API: 0x9216f840 `XuiSoundPlay(h)` (0x7f4; called by dash 0x922198b8), 0x9216f888 `XuiSoundStop(h)` (0x7f5), 0x9216fb70 `XuiSoundGetVolume` (0x7f9), 0x9216fb00 `XuiSoundGetMute` (0x7fc), 0x9216faa0 `XuiSoundSetMute(h, v)` (0x7fd, no callers), 0x9216f668 `XuiSoundSetGlobalMute(v)` (writes 0x9280b408; called by DashInitAudioAndLanguage 0x921bf610 and 0x921cb238). Show/visibility has no effect on sounds.

RE status: complete.

### XuiSoundXAudio

Registration 0x9214c5b8 (called by XuiSoundXAudioRegister 0x9214c668), base XuiSound. Thunk 0x9214c560 -> impl 0x9214c2f8 `XuiSoundXAudio_ObjectProcImpl`. Create 0x92141650 (0x14 zeroed bytes, shared with other classes), destroy 0x9214c568 (free File, ReleaseVoice, free).

Instance (0x14): +0x00 hObj; +0x04 File wchar*; +0x08 XAudio source voice; +0x0c sound buffer cache entry (refcount at entry +8); +0x10 u32 mute.

File setter 0x9214b7e0: type String required. If the new value equals the old (null-safe compare 0x9214b6f0; both null equal): no action. Else store a copy. Then if element flag +0xf0 bit 0x2000 is clear (0x9214e1b8 `XuiElementIsTimelineAutoplayDisabled`; the bit marks elements instantiated without timeline autoplay, see xui-timelines.md section 1.1) and File != null: XuiSoundScheduleStateApply. This is how timeline keyframes on File (e.g. `sharedres://btn_Select.xma` at the Press frame in dashuisk/skin.xui) start sounds. An empty string keyframe is a change and schedules an apply with an empty path [INFERENCE: StartVoice then fails path resolution or load].

| Msg | Behavior |
| --- | --- |
| 0x0 (render) | handled (draws nothing). |
| 0x27, id 0xaabbccdd | Not handled (XuiSound sees it next). If voice null, or voice state byte bit0 clear: ReleaseVoice. |
| 0x28 | data bit 0x2: ReleaseVoice. |
| 0x7f4 (play) | handled. If element flag bit 0x2000 clear: StartVoice; on success set XuiSound playing bit. Result returned. |
| 0x7f5 (stop) | handled. StopVoiceDeferred; on success clear XuiSound playing bit. |
| 0x7f6 (query done) | If no voice or voice state bit0 clear: data \|= 1. Not handled. |
| 0x7fa (set volume) | ApplyVolume(data f32). Not handled (XuiSound validates and stores next). |
| 0x7fc | handled. data = mute (+0x10). |
| 0x7fd | handled. mute = data; ApplyVolume(FLT_MAX). |
| 0x805 | data != 0: send 0x7f5 to self. data == 0 and File != null: XuiSoundScheduleStateApply. No sender found in the binary. |

```
StartVoice (0x9214c050):
    if File == null: return S_OK
    if voice: StopVoiceDeferred
    path = XuiSoundResolvePath(h, File)   // 0x9216f8d0: resolve relative to the element, or with no element when inside a XuiVisual (0x9215cde8)
    if path == null: E_FAIL
    entry = cache lookup(path) in map 0x9280b40c ; if none: load RIFF/WAVE (0x92171e10) into a 0x48-byte buffer and insert
    free path ; reap stopped voices ; (cache maintenance 0x92170140)
    format tag 0x165 (XMA): voice type 4, stream count, per stream {rate u32, channels u8}
    format tag 1 (PCM): channels, sample rate; bits 16 -> type 1, bits 8 -> type 2 ; other: E_INVALIDARG
    create source voice (0x925a8748) ; submit one packet {data ptr, data size} (0x925a8318)
    XuiSetTimer(h, 0xaabbccdd, 10) ; ApplyVolume(FLT_MAX) ; start voice (0x925a83d0, flags 0)
    on any failure: ReleaseVoice

ApplyVolume(v) (0x9214bb40):
    if |v - FLT_MAX| <= 1.1920929e-7: v = XuiSoundGetVolume(h)
    if v < -96 or v > 6: E_INVALIDARG
    mute = XuiSoundGetMute(h) ; if no voice: S_OK
    amp = (mute or globalMute) ? 0.0 : pow(10.0, v * 0.05)
    voice set volume (0x925a8570, amp)

ReleaseVoice (0x9214bfb8): XuiKillTimer; if voice: stop now (0x925a8378 = voice vtbl+0x3c(1) then packet flush), destroy (0x925a8138); atomic decrement entry refcount; entry = 0; reap stopped voices
StopVoiceDeferred (0x9214bde8): if voice: voice vtbl+0x3c(0) (0x925a8418); append {voice, entry} to the deferred list (0x9214bc80, under lock 0x92809c74); voice = entry = 0; XuiKillTimer
ReapStoppedVoices (0x9214beb8): for each deferred voice whose state bit0 is clear: drop the entry reference and destroy the voice
```

Timer sharing: XuiSound (period 0) and XuiSoundXAudio (period 10) use the same (h, 0xaabbccdd) record. XAudio sees each tick first. After a non-looping play XuiSound kills the shared timer, so a finished voice stays allocated until the next play, stop, 0x28 or destroy. For looping sounds the XAudio level releases the finished voice and the XuiSound level then sees "done" and plays again, so there is a gap of at least one tick between loops.

RE status: partial. Proc, volume, timer and lifecycle are complete. The WAVE loader, cache (0x9216fe50, 0x921700b8, 0x92170140) and XAudio library internals are only identified by role.

### Nuances

- XuiEdit never sends a text-changed notification.
- XuiEdit is not editable when Multiline: Multiline forces ReadOnly, and ReadOnly cannot be cleared while Multiline or SmoothScroll is set.
- Caret visibility uses Enabled, not focus. The caret x is the measured width of the text before the caret, without the `Text` presenter offset.
- AllowedChars set to null keeps the previous range list; a parse error leaves no list (all characters allowed).
- TextLimit lower than the current text writes a terminator into the text but does not rebuild the display copy or layout.
- 0x7e0 with text over TextLimit falls through to XuiControl and changes Control.Text while the edit buffer is unchanged.
- InsertText (0x2d) does not validate the index. DeleteText computes the new caret with unsigned subtraction.
- Single-line horizontal scroll moves firstChar in steps of 5 characters; ShiftViewLeft does not clamp.
- ScrollToLine does not clamp to the maximum scroll position.
- Non-animated ScrollUp/ScrollDown leave velocity at -1.0/+1.0 without a timer.
- 0x7df reports the display run capacity (+0x50) as the run count; it is not reduced when the source count shrinks.
- XuiEdit's 0x13 handler sends 0x29 to the stored caret handle before FindVisualChildren refreshes it; on the first init that handle is 0.
- XuiTextPresenter overwrites its own TextStyle bits 0xe with the owner's bits when it rebuilds the font.
- XuiMessageBox: destroy then result; result index comes from the Id digit; extra ButtonN children stay visible; missing ButtonN aborts setup before MessageText is set.
- XuiSound Finish is stored but never read.
- Sounds inside a XuiVisual resolve File without the element (no scene base path).
- XuiSound and XuiSoundXAudio share one timer record; period changes between 0 and 10 ms depending on which level armed it last.

### Open questions

- Meaning of TextStyle bits 0x30, 0x2000, 0x4000 (renderer).
- Writer of XuiEdit source highlight runs (+0x44/+0x48); none found.
- Sender of message 0x805; none found by constant scan.
- XAudio voice vtable +0x3c argument semantics (0 vs 1) and the voice state byte layout.

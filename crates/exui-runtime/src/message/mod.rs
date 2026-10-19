//! XUI messages, class behaviors and dispatch (xui-controls.md "Messages").

mod dispatch;
mod keys;
mod runtime;
mod timer;

use alloc::boxed::Box;
use alloc::rc::Rc;
use core::any::Any;

use crate::input::Key;
use crate::input::UserIndex;
use crate::resource::TextureObject;
use crate::scene::ElementId;

pub use crate::error::RuntimeError;
pub use dispatch::ClassBehavior;
pub use dispatch::Context;
pub use dispatch::LevelHandle;
pub use dispatch::MessageHook;
pub use keys::KeyFlags;
pub use keys::NavKey;
pub use keys::key_to_nav;
pub use runtime::Runtime;
pub use runtime::SoundEvent;
pub use runtime::SoundSink;
pub use runtime::TraceEvent;
pub use timer::TimerId;
pub use timer::Timers;

/// Numeric message id as used by the original.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MessageId(pub u32);

impl From<BuiltinMessage> for MessageId {
    fn from(message: BuiltinMessage) -> Self {
        Self(message as u32)
    }
}

/// Ids of the messages defined by the XUI library.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum BuiltinMessage {
    Render = 0x0,
    ChildAdded = 0x4,
    Init = 0x5,
    ParentSizeChanged = 0x8,
    CreateVisual = 0x9,
    Destroy = 0xa,
    QueryFocusable = 0xb,
    SetFocus = 0xc,
    KillFocus = 0xd,
    Notify = 0xe,
    TimelineEnd = 0xf,
    SizeChanging = 0x10,
    SetShow = 0x12,
    InitComplete = 0x13,
    TimelineStart = 0x14,
    NavigatedBack = 0x1a,
    GetBackScene = 0x1b,
    SetBackScene = 0x1c,
    StartTransition = 0x1d,
    TransitionEnded = 0x1e,
    ChildRemoved = 0x20,
    GetSkinPrefix = 0x22,
    SetSkinPrefix = 0x23,
    KeyDown = 0x24,
    KeyUp = 0x25,
    Char = 0x26,
    Timer = 0x27,
    ReleaseResources = 0x28,
    CreateResources = 0x29,
    InterruptTransition = 0x2a,
    NavigateForwardQuery = 0x2b,
    BeforeUnlink = 0x2f,
    GetValue = 0x7d0,
    SetValue = 0x7d1,
    GetEnabled = 0x7d2,
    SetEnabled = 0x7d3,
    GetSelected = 0x7d4,
    SetSelected = 0x7d5,
    UpdateState = 0x7d6,
    GetCurItem = 0x7d7,
    SetCurItem = 0x7d8,
    GetChildByIndex = 0x7d9,
    GetTop = 0x7da,
    SetTop = 0x7db,
    GetCount = 0x7dc,
    InsertItems = 0x7dd,
    DeleteItems = 0x7de,
    GetSourceText = 0x7df,
    SetSourceText = 0x7e0,
    GetSourceImage = 0x7e1,
    SetSourceImage = 0x7e2,
    GetItemCheck = 0x7e3,
    SetItemCheck = 0x7e4,
    Press = 0x7e5,
    GetNavTarget = 0x7e6,
    ScrollEndSetState = 0x7e7,
    GetItemEnabled = 0x7e8,
    SetItemEnabled = 0x7e9,
    MessageBoxResult = 0x7ec,
    EditGetTopLine = 0x7ed,
    EditScrollToLine = 0x7ee,
    EditLineInfo = 0x7ef,
    TabActivated = 0x7f0,
    TabDeactivated = 0x7f1,
    EditGetSmoothScroll = 0x7f2,
    EditSetSmoothScroll = 0x7f3,
    SoundPlay = 0x7f4,
    SoundStop = 0x7f5,
    SoundQueryDone = 0x7f6,
    GetPressed = 0x7f7,
    SetPressed = 0x7f8,
    SoundGetVolume = 0x7f9,
    SoundSetVolume = 0x7fa,
    GetTextVersion = 0x7fb,
    SoundGetMute = 0x7fc,
    SoundSetMute = 0x7fd,
    FindTabStop = 0x800,
    GetNavData = 0x803,
    SetNavData = 0x804,
    GetFocusDisabled = 0x806,
    SetFocusDisabled = 0x807,
    ScrollBarSetRange = 0x808,
    SizeChanged = 0x80d,
    TabSceneInitQuery = 0xbb8,
}

/// Resource kinds of the release resources (0x28) and before unlink (0x2f)
/// messages.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ResourceMask(pub u32);

impl ResourceMask {
    pub const NONE: Self = Self(0);
    pub const TEXTURES: Self = Self(0x1);
    pub const SOUNDS: Self = Self(0x2);
    pub const FONTS: Self = Self(0x4);
    pub const CONTROL_VISUALS: Self = Self(0x10);
    /// Initial before unlink request (-1): release every kind.
    pub const ALL: Self = Self(u32::MAX);

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// Focus slot owner: one of four users or the shared slot (user 0xff).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FocusUser {
    User(UserIndex),
    Shared,
}

impl FocusUser {
    pub fn from_raw(raw: u8) -> Option<Self> {
        if raw == 0xff {
            return Some(Self::Shared);
        }

        UserIndex::new(raw).map(Self::User)
    }

    pub fn raw(self) -> u8 {
        match self {
            Self::User(user) => user.get(),
            Self::Shared => 0xff,
        }
    }
}

/// Key message data (0x24, 0x25).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyData {
    pub key: Key,
    pub character: Option<u16>,
    pub flags: KeyFlags,
    pub user: UserIndex,
}

/// Character message data (0x26).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CharData {
    pub character: u16,
    pub flags: KeyFlags,
    pub user: UserIndex,
}

/// Scene transition kinds (message 0x1d data +0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionKind {
    To,
    From,
    BackTo,
    BackFrom,
}

/// What happens to a scene after its transition (message 0x1d data +8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransitionAction {
    Keep,
    Remove,
    Destroy,
}

/// Navigation direction of 0x7e6.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavDirection {
    Up,
    Down,
    Left,
    Right,
    TabForward,
    TabBackward,
}

/// Search kind of 0x800.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabStopKind {
    Next,
    Previous,
    First,
    Last,
}

/// Target answer of 0x7e6 and 0x800.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NavQuery<D> {
    pub direction: D,
    pub source: ElementId,
    pub target: Option<ElementId>,
    /// The target is the source: consume the key without a focus change.
    pub target_is_source: bool,
}

/// Get source text (0x7df) data. Only the binding fields are modeled.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceTextQuery {
    /// Item index; 0 for the control itself. XuiControl answers index 0
    /// without testing `item_data`.
    pub item: u32,
    pub association: u32,
    pub item_data: bool,
    pub text: Option<Rc<[u16]>>,
    /// `None` keeps the presenter's point size (the -1.0 preset).
    pub point_size: Option<f32>,
    pub extended: bool,
    pub y_offset: f32,
    pub style: u32,
}

/// Get source image (0x7e1) data.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceImageQuery {
    /// Item index; 0 for the control itself.
    pub item: u32,
    pub association: u32,
    pub item_data: bool,
    pub force_reload: bool,
    pub path: Option<Rc<[u16]>>,
    /// +0x14: an image the presenter shows instead of a path.
    pub texture: Option<TextureObject>,
}

/// Navigation data of XuiNavButton and list items (0x803, 0x804).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NavData {
    pub path: Option<Rc<[u16]>>,
    pub stay_visible: bool,
    pub src_trans_index: u32,
    pub dest_trans_index: u32,
}

/// Count kinds of 0x7dc.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CountKind {
    Items,
    Visible,
    Checked,
    ItemControls,
    Lines,
    PerLine,
}

/// Scroll end states (0x7e7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollEndState {
    Normal,
    More,
    Scrolling,
}

/// Line counts of the XuiEdit line info message (0x7ef, data +0).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditLineInfoKind {
    /// 0: wrapped lines of the whole text.
    Lines,
    /// 1: lines shown from the top line to the end, at most a page.
    ShownLines,
    /// 2: whole lines that fit the `Text` height.
    PageLines,
}

/// Scroll speed in text lines per second.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LinesPerSecond(pub f32);

/// Smooth scroll settings of XuiEdit (0x7f2 and 0x7f3, data +4 to +0x10).
/// Data +0 must be 0; no sender passes anything else.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SmoothScrollParams {
    pub enabled: bool,
    pub start_speed: LinesPerSecond,
    /// Fraction the speed grows by at each 16 ms scroll step.
    pub acceleration: f32,
    pub max_speed: LinesPerSecond,
}

/// Built-in notify codes of message 0xe.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum BuiltinNotify {
    Press = 0x1,
    SetFocus = 0x2,
    KillFocus = 0x3,
    SelectionChanged = 0x4,
    ScrollEndStateRequest = 0x5,
    ItemCheckChanged = 0x6,
    Pressing = 0x7,
    ValueChanging = 0xa,
    ValueChanged = 0xb,
    ScrollBarScroll = 0xe,
}

/// Numeric notify code as used by the original.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NotifyCode(pub u32);

impl From<BuiltinNotify> for NotifyCode {
    fn from(code: BuiltinNotify) -> Self {
        Self(code as u32)
    }
}

/// Notify payloads of message 0xe.
#[derive(Clone, Debug)]
pub enum NotifyKind {
    Press {
        user: FocusUser,
    },
    SetFocus {
        previous: Option<ElementId>,
    },
    KillFocus {
        new: Option<ElementId>,
    },
    SelectionChanged {
        new: Option<u32>,
        old: Option<u32>,
    },
    ScrollEndStateRequest,
    ItemCheckChanged {
        index: u32,
        checked: bool,
    },
    /// Handlers may set `cancel`.
    Pressing {
        repeat: bool,
        cancel: bool,
        user: FocusUser,
    },
    /// Handlers may replace `new`.
    ValueChanging {
        old: i32,
        new: i32,
    },
    ValueChanged {
        value: i32,
    },
    ScrollBarScroll {
        top: u32,
    },
    /// Codes defined by the application.
    Application {
        code: NotifyCode,
        data: Option<Rc<dyn Any>>,
    },
}

impl NotifyKind {
    pub fn code(&self) -> NotifyCode {
        let code = match self {
            Self::Press { .. } => BuiltinNotify::Press,
            Self::SetFocus { .. } => BuiltinNotify::SetFocus,
            Self::KillFocus { .. } => BuiltinNotify::KillFocus,
            Self::SelectionChanged { .. } => BuiltinNotify::SelectionChanged,
            Self::ScrollEndStateRequest => BuiltinNotify::ScrollEndStateRequest,
            Self::ItemCheckChanged { .. } => BuiltinNotify::ItemCheckChanged,
            Self::Pressing { .. } => BuiltinNotify::Pressing,
            Self::ValueChanging { .. } => BuiltinNotify::ValueChanging,
            Self::ValueChanged { .. } => BuiltinNotify::ValueChanged,
            Self::ScrollBarScroll { .. } => BuiltinNotify::ScrollBarScroll,
            Self::Application { code, .. } => return *code,
        };

        code.into()
    }
}

#[derive(Clone, Debug)]
pub struct Notify {
    pub kind: NotifyKind,
    pub source: ElementId,
}

/// Message body with its data block. Out fields are written by handlers.
#[derive(Clone, Debug)]
pub enum Body {
    /// Only used to create control visuals before drawing.
    Render,
    ChildAdded {
        child: ElementId,
    },
    /// Pre-order over a new subtree.
    Init {
        parent: Option<ElementId>,
    },
    /// Both pairs carry the new size.
    ParentSizeChanged {
        width: f32,
        height: f32,
    },
    CreateVisual,
    /// Reaches every class level.
    Destroy,
    QueryFocusable {
        focusable: bool,
    },
    SetFocus {
        previous: Option<ElementId>,
        user: FocusUser,
    },
    KillFocus {
        new: Option<ElementId>,
        user: FocusUser,
    },
    Notify(Notify),
    TimelineEnd {
        owner: ElementId,
    },
    SizeChanging {
        old: (f32, f32),
        new: (f32, f32),
    },
    /// Data is the new state; the element's Show flag is set by the
    /// base level.
    SetShow {
        show: bool,
    },
    /// Post-order after `Init`.
    InitComplete,
    TimelineStart {
        owner: ElementId,
    },
    NavigatedBack {
        from: ElementId,
    },
    GetBackScene {
        scene: Option<ElementId>,
    },
    SetBackScene {
        new: Option<ElementId>,
        old: Option<ElementId>,
    },
    StartTransition {
        kind: TransitionKind,
        deferred: bool,
        action: TransitionAction,
    },
    TransitionEnded {
        kind: TransitionKind,
        action: TransitionAction,
    },
    ChildRemoved {
        child: ElementId,
    },
    /// Bubbled.
    GetSkinPrefix {
        prefix: Option<Rc<[u16]>>,
    },
    SetSkinPrefix {
        prefix: Option<Rc<[u16]>>,
    },
    KeyDown(KeyData),
    KeyUp(KeyData),
    Char(CharData),
    Timer {
        id: TimerId,
    },
    ReleaseResources {
        mask: ResourceMask,
    },
    /// Subtree broadcast.
    CreateResources,
    InterruptTransition,
    /// Handlers may replace or clear `next`.
    NavigateForwardQuery {
        next: Option<ElementId>,
    },
    BeforeUnlink {
        flags: ResourceMask,
    },
    GetValue {
        value: u32,
    },
    SetValue {
        value: u32,
    },
    GetEnabled {
        enabled: bool,
    },
    SetEnabled {
        enabled: bool,
    },
    GetSelected {
        selected: Option<ElementId>,
    },
    SetSelected {
        selected: Option<ElementId>,
    },
    UpdateState {
        explicit: bool,
        focused: bool,
    },
    GetCurItem {
        index: Option<u32>,
        item: Option<ElementId>,
    },
    /// Written back clamped.
    SetCurItem {
        index: Option<u32>,
    },
    GetChildByIndex {
        index: u32,
        child: Option<ElementId>,
    },
    GetTop {
        top: Option<u32>,
    },
    /// Written back clamped; `None` input is rejected.
    SetTop {
        top: Option<u32>,
    },
    GetCount {
        kind: CountKind,
        count: u32,
    },
    InsertItems {
        index: u32,
        count: u32,
    },
    DeleteItems {
        index: u32,
        count: u32,
    },
    GetSourceText(SourceTextQuery),
    SetSourceText {
        item: u32,
        association: u32,
        item_data: bool,
        text: Option<Rc<[u16]>>,
    },
    GetSourceImage(SourceImageQuery),
    SetSourceImage {
        item: u32,
        association: u32,
        item_data: bool,
        path: Option<Rc<[u16]>>,
    },
    GetItemCheck {
        index: u32,
        checked: bool,
    },
    SetItemCheck {
        index: u32,
        checked: bool,
    },
    Press {
        accepted: bool,
        user: FocusUser,
        from_input: bool,
    },
    GetNavTarget(NavQuery<NavDirection>),
    ScrollEndSetState {
        state: ScrollEndState,
    },
    GetItemEnabled {
        index: u32,
        enabled: bool,
    },
    SetItemEnabled {
        index: u32,
        enabled: bool,
    },
    /// Sent to the message box owner. `None`: the box failed or was
    /// cancelled (button -1 of the original).
    MessageBoxResult {
        button: Option<u32>,
    },
    /// Line at the top of the view. `current` reads the animated position
    /// instead of the scroll target. `None` while unanswered.
    EditGetTopLine {
        current: bool,
        line: Option<u32>,
    },
    EditScrollToLine {
        line: u32,
        animate: bool,
    },
    EditLineInfo {
        kind: EditLineInfoKind,
        current: bool,
        value: Option<u32>,
    },
    TabActivated,
    TabDeactivated,
    EditGetSmoothScroll {
        params: Option<SmoothScrollParams>,
    },
    EditSetSmoothScroll {
        params: SmoothScrollParams,
    },
    SoundPlay,
    SoundStop,
    SoundQueryDone {
        done: bool,
    },
    GetPressed {
        pressed: bool,
    },
    SetPressed {
        pressed: bool,
    },
    SoundGetVolume {
        db: f32,
    },
    SoundSetVolume {
        db: f32,
    },
    GetTextVersion {
        version: u32,
        item: u32,
        association: u32,
        text: Option<Rc<[u16]>>,
    },
    SoundGetMute {
        mute: bool,
    },
    SoundSetMute {
        mute: bool,
    },
    FindTabStop(NavQuery<TabStopKind>),
    GetNavData {
        item: Option<u32>,
        data: NavData,
    },
    SetNavData {
        item: Option<u32>,
        data: NavData,
    },
    GetFocusDisabled {
        disabled: bool,
    },
    SetFocusDisabled {
        disabled: bool,
    },
    ScrollBarSetRange {
        count: u32,
        /// `None`: the list has no top item (-1 in the original).
        top: Option<u32>,
        visible: u32,
    },
    SizeChanged,
    TabSceneInitQuery,
    /// Messages defined by the application.
    Application {
        id: MessageId,
        data: Option<Box<dyn AnyClone>>,
    },
}

/// Application message payload.
pub trait AnyClone: Any + core::fmt::Debug {
    fn clone_box(&self) -> Box<dyn AnyClone>;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<T> AnyClone for T
where
    T: Any + Clone + core::fmt::Debug,
{
    fn clone_box(&self) -> Box<dyn AnyClone> {
        Box::new(self.clone())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

impl Clone for Box<dyn AnyClone> {
    fn clone(&self) -> Self {
        (**self).clone_box()
    }
}

impl Body {
    pub fn id(&self) -> MessageId {
        let message = match self {
            Self::Render => BuiltinMessage::Render,
            Self::ChildAdded { .. } => BuiltinMessage::ChildAdded,
            Self::Init { .. } => BuiltinMessage::Init,
            Self::ParentSizeChanged { .. } => BuiltinMessage::ParentSizeChanged,
            Self::CreateVisual => BuiltinMessage::CreateVisual,
            Self::Destroy => BuiltinMessage::Destroy,
            Self::QueryFocusable { .. } => BuiltinMessage::QueryFocusable,
            Self::SetFocus { .. } => BuiltinMessage::SetFocus,
            Self::KillFocus { .. } => BuiltinMessage::KillFocus,
            Self::Notify(_) => BuiltinMessage::Notify,
            Self::TimelineEnd { .. } => BuiltinMessage::TimelineEnd,
            Self::SizeChanging { .. } => BuiltinMessage::SizeChanging,
            Self::SetShow { .. } => BuiltinMessage::SetShow,
            Self::InitComplete => BuiltinMessage::InitComplete,
            Self::TimelineStart { .. } => BuiltinMessage::TimelineStart,
            Self::NavigatedBack { .. } => BuiltinMessage::NavigatedBack,
            Self::GetBackScene { .. } => BuiltinMessage::GetBackScene,
            Self::SetBackScene { .. } => BuiltinMessage::SetBackScene,
            Self::StartTransition { .. } => BuiltinMessage::StartTransition,
            Self::TransitionEnded { .. } => BuiltinMessage::TransitionEnded,
            Self::ChildRemoved { .. } => BuiltinMessage::ChildRemoved,
            Self::GetSkinPrefix { .. } => BuiltinMessage::GetSkinPrefix,
            Self::SetSkinPrefix { .. } => BuiltinMessage::SetSkinPrefix,
            Self::KeyDown(_) => BuiltinMessage::KeyDown,
            Self::KeyUp(_) => BuiltinMessage::KeyUp,
            Self::Char(_) => BuiltinMessage::Char,
            Self::Timer { .. } => BuiltinMessage::Timer,
            Self::ReleaseResources { .. } => BuiltinMessage::ReleaseResources,
            Self::CreateResources => BuiltinMessage::CreateResources,
            Self::InterruptTransition => BuiltinMessage::InterruptTransition,
            Self::NavigateForwardQuery { .. } => BuiltinMessage::NavigateForwardQuery,
            Self::BeforeUnlink { .. } => BuiltinMessage::BeforeUnlink,
            Self::GetValue { .. } => BuiltinMessage::GetValue,
            Self::SetValue { .. } => BuiltinMessage::SetValue,
            Self::GetEnabled { .. } => BuiltinMessage::GetEnabled,
            Self::SetEnabled { .. } => BuiltinMessage::SetEnabled,
            Self::GetSelected { .. } => BuiltinMessage::GetSelected,
            Self::SetSelected { .. } => BuiltinMessage::SetSelected,
            Self::UpdateState { .. } => BuiltinMessage::UpdateState,
            Self::GetCurItem { .. } => BuiltinMessage::GetCurItem,
            Self::SetCurItem { .. } => BuiltinMessage::SetCurItem,
            Self::GetChildByIndex { .. } => BuiltinMessage::GetChildByIndex,
            Self::GetTop { .. } => BuiltinMessage::GetTop,
            Self::SetTop { .. } => BuiltinMessage::SetTop,
            Self::GetCount { .. } => BuiltinMessage::GetCount,
            Self::InsertItems { .. } => BuiltinMessage::InsertItems,
            Self::DeleteItems { .. } => BuiltinMessage::DeleteItems,
            Self::GetSourceText(_) => BuiltinMessage::GetSourceText,
            Self::SetSourceText { .. } => BuiltinMessage::SetSourceText,
            Self::GetSourceImage(_) => BuiltinMessage::GetSourceImage,
            Self::SetSourceImage { .. } => BuiltinMessage::SetSourceImage,
            Self::GetItemCheck { .. } => BuiltinMessage::GetItemCheck,
            Self::SetItemCheck { .. } => BuiltinMessage::SetItemCheck,
            Self::Press { .. } => BuiltinMessage::Press,
            Self::GetNavTarget(_) => BuiltinMessage::GetNavTarget,
            Self::ScrollEndSetState { .. } => BuiltinMessage::ScrollEndSetState,
            Self::GetItemEnabled { .. } => BuiltinMessage::GetItemEnabled,
            Self::SetItemEnabled { .. } => BuiltinMessage::SetItemEnabled,
            Self::MessageBoxResult { .. } => BuiltinMessage::MessageBoxResult,
            Self::EditGetTopLine { .. } => BuiltinMessage::EditGetTopLine,
            Self::EditScrollToLine { .. } => BuiltinMessage::EditScrollToLine,
            Self::EditLineInfo { .. } => BuiltinMessage::EditLineInfo,
            Self::TabActivated => BuiltinMessage::TabActivated,
            Self::TabDeactivated => BuiltinMessage::TabDeactivated,
            Self::EditGetSmoothScroll { .. } => BuiltinMessage::EditGetSmoothScroll,
            Self::EditSetSmoothScroll { .. } => BuiltinMessage::EditSetSmoothScroll,
            Self::SoundPlay => BuiltinMessage::SoundPlay,
            Self::SoundStop => BuiltinMessage::SoundStop,
            Self::SoundQueryDone { .. } => BuiltinMessage::SoundQueryDone,
            Self::GetPressed { .. } => BuiltinMessage::GetPressed,
            Self::SetPressed { .. } => BuiltinMessage::SetPressed,
            Self::SoundGetVolume { .. } => BuiltinMessage::SoundGetVolume,
            Self::SoundSetVolume { .. } => BuiltinMessage::SoundSetVolume,
            Self::GetTextVersion { .. } => BuiltinMessage::GetTextVersion,
            Self::SoundGetMute { .. } => BuiltinMessage::SoundGetMute,
            Self::SoundSetMute { .. } => BuiltinMessage::SoundSetMute,
            Self::FindTabStop(_) => BuiltinMessage::FindTabStop,
            Self::GetNavData { .. } => BuiltinMessage::GetNavData,
            Self::SetNavData { .. } => BuiltinMessage::SetNavData,
            Self::GetFocusDisabled { .. } => BuiltinMessage::GetFocusDisabled,
            Self::SetFocusDisabled { .. } => BuiltinMessage::SetFocusDisabled,
            Self::ScrollBarSetRange { .. } => BuiltinMessage::ScrollBarSetRange,
            Self::SizeChanged => BuiltinMessage::SizeChanged,
            Self::TabSceneInitQuery => BuiltinMessage::TabSceneInitQuery,
            Self::Application { id, .. } => return *id,
        };

        message.into()
    }
}

/// `XUIMessage`: a body and the handled flag.
#[derive(Clone, Debug)]
pub struct Message {
    pub body: Body,
    pub handled: bool,
}

impl Message {
    pub fn new(body: Body) -> Self {
        Self {
            body,
            handled: false,
        }
    }

    pub fn id(&self) -> MessageId {
        self.body.id()
    }
}

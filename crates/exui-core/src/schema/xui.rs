//! Classes registered by the XUI library: the `XuiInit` built-ins and the
//! optional `XuiSoundXAudioRegister` and `XuiHtmlRegister` groups.
//!
//! Generated from the dash 6717 class registration functions; addresses are
//! the registering functions.

use crate::error::RegisterError;
use crate::schema::ClassDef;
use crate::schema::DefaultValue;
use crate::schema::PropDef;
use crate::schema::PropFlags;
use crate::schema::PropType;
use crate::schema::Registry;
use crate::value::Color;
use crate::value::Quaternion;
use crate::value::Vector3;

/// Classes registered by `XuiInit` (0x9214b368), in its order.
pub static BUILTIN: [&ClassDef; 41] = [
    &XUI_ELEMENT,
    &XUI_CANVAS,
    &XUI_FIGURE,
    &XUI_CONTROL,
    &XUI_SCENE,
    &XUI_BUTTON,
    &XUI_BACK_BUTTON,
    &XUI_NAV_BUTTON,
    &XUI_CHECKBOX,
    &XUI_RADIO_BUTTON,
    &XUI_RADIO_GROUP,
    &XUI_TEXT,
    &XUI_TEXT_PRESENTER,
    &XUI_VISUAL,
    &XUI_GROUP,
    &XUI_SOUND,
    &XUI_TRANSITION,
    &XUI_LIST_ITEM,
    &XUI_SCROLL_END,
    &XUI_SCROLL_BAR,
    &XUI_LIST,
    &XUI_COMMON_LIST,
    &XUI_PROGRESS_BAR,
    &XUI_SLIDER,
    &XUI_CARET,
    &XUI_EDIT,
    &XUI_IMAGE_PRESENTER,
    &XUI_IMAGE,
    &XUI_MESSAGE_BOX,
    &XUI_LABEL,
    &XUI_TAB_SCENE,
    &XUI_NINE_GRID,
    &XUI_EFFECT,
    &XUI_TEXTURE_EFFECT,
    &XUI_GRAYSCALE_EFFECT,
    &XUI_RECOLOR_EFFECT,
    &XUI_BRIGHT_PASS_EFFECT,
    &XUI_EDGE_EFFECT,
    &XUI_HV_BLUR_EFFECT,
    &XUI_H_BLUR_EFFECT,
    &XUI_V_BLUR_EFFECT,
];

/// Classes registered by `XuiSoundXAudioRegister`.
pub static SOUND_XAUDIO: [&ClassDef; 1] = [&XUI_SOUND_X_AUDIO];

/// Classes registered by `XuiHtmlRegister`, in its order.
pub static HTML: [&ClassDef; 3] = [&XUI_HTML_ELEMENT, &XUI_HTML_PRESENTER, &XUI_HTML_CONTROL];

/// Registers the `XuiSoundXAudioRegister` (0x9214c668) group.
pub fn register_sound_xaudio(registry: &mut Registry) -> Result<(), RegisterError> {
    registry.register_all(&SOUND_XAUDIO)
}

/// Registers the `XuiHtmlRegister` (0x92199a20) group.
pub fn register_html(registry: &mut Registry) -> Result<(), RegisterError> {
    registry.register_all(&HTML)
}

static XUI_ELEMENT_PROPS: [PropDef; 17] = [
    PropDef::new(
        "Id",
        PropType::String,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "Width",
        PropType::Float,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Float(60.0)),
    ),
    PropDef::new(
        "Height",
        PropType::Float,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Float(30.0)),
    ),
    PropDef::new(
        "Position",
        PropType::Vector,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Vector(Vector3::new(0.0, 0.0, 0.0))),
    ),
    PropDef::new(
        "Scale",
        PropType::Vector,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Vector(Vector3::new(1.0, 1.0, 1.0))),
    ),
    PropDef::new(
        "Rotation",
        PropType::Quaternion,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Quaternion(Quaternion::new(
            0.0, 0.0, 0.0, 1.0,
        ))),
    ),
    PropDef::new(
        "Opacity",
        PropType::Float,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Float(1.0)),
    ),
    PropDef::new(
        "Anchor",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(0)),
    ),
    PropDef::new(
        "Pivot",
        PropType::Vector,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Vector(Vector3::new(0.0, 0.0, 0.0))),
    ),
    PropDef::new(
        "Show",
        PropType::Bool,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Bool(true)),
    ),
    PropDef::new(
        "BlendMode",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(0)),
    ),
    PropDef::new(
        "DisableTimelineRecursion",
        PropType::Bool,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Bool(false)),
    ),
    PropDef::new(
        "DesignTime",
        PropType::Bool,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::Bool(false)),
    ),
    PropDef::new(
        "ColorWriteFlags",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(0)),
    ),
    PropDef::new(
        "ClipChildren",
        PropType::Bool,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Bool(false)),
    ),
    PropDef::new(
        "EnableEffects",
        PropType::Bool,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Bool(false)),
    ),
    PropDef::new(
        "DisableFocusRecursion",
        PropType::Bool,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Bool(false)),
    ),
];

/// Registered at 0x9214ab08.
pub static XUI_ELEMENT: ClassDef = ClassDef::new("XuiElement", None, &XUI_ELEMENT_PROPS);

/// Registered at 0x92147e78.
pub static XUI_CANVAS: ClassDef = ClassDef::new("XuiCanvas", Some("XuiElement"), &[]);

static XUI_FIGURE_STROKE_PROPS: [PropDef; 2] = [
    PropDef::new(
        "StrokeWidth",
        PropType::Float,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Float(0.0)),
    ),
    PropDef::new(
        "StrokeColor",
        PropType::Color,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Color(Color::from_argb(0xff0f0feb))),
    ),
];

static XUI_FIGURE_FILL_GRADIENT_PROPS: [PropDef; 4] = [
    PropDef::new(
        "Radial",
        PropType::Bool,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Bool(false)),
    ),
    PropDef::new(
        "NumStops",
        PropType::Integer,
        PropFlags::from_bits(0xa),
        Some(DefaultValue::Integer(0)),
    ),
    PropDef::new(
        "StopColor",
        PropType::Color,
        PropFlags::from_bits(0x1),
        None,
    ),
    PropDef::new("StopPos", PropType::Float, PropFlags::from_bits(0x1), None),
];

static XUI_FIGURE_FILL_PROPS: [PropDef; 11] = [
    PropDef::new(
        "FillType",
        PropType::Unsigned,
        PropFlags::from_bits(0x2),
        Some(DefaultValue::Unsigned(1)),
    ),
    PropDef::new(
        "FillColor",
        PropType::Color,
        PropFlags::from_bits(0x2),
        Some(DefaultValue::Color(Color::from_argb(0xff0f0f80))),
    ),
    PropDef::new(
        "TextureFileName",
        PropType::String,
        PropFlags::from_bits(0x2),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "Gradient",
        PropType::Compound(&XUI_FIGURE_FILL_GRADIENT_PROPS),
        PropFlags::from_bits(0x2),
        None,
    ),
    PropDef::new(
        "Translation",
        PropType::Vector,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Vector(Vector3::new(0.0, 0.0, 0.0))),
    ),
    PropDef::new(
        "Scale",
        PropType::Vector,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Vector(Vector3::new(1.0, 1.0, 1.0))),
    ),
    PropDef::new(
        "Rotation",
        PropType::Float,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Float(0.0)),
    ),
    PropDef::new(
        "WrapX",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(0)),
    ),
    PropDef::new(
        "WrapY",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(0)),
    ),
    PropDef::new(
        "BrushFlags",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(0)),
    ),
    PropDef::new(
        "TransformVersion",
        PropType::Unsigned,
        PropFlags::from_bits(0xa),
        Some(DefaultValue::Unsigned(0)),
    ),
];

static XUI_FIGURE_PROPS: [PropDef; 4] = [
    PropDef::new(
        "Stroke",
        PropType::Compound(&XUI_FIGURE_STROKE_PROPS),
        PropFlags::from_bits(0x2),
        None,
    ),
    PropDef::new(
        "Fill",
        PropType::Compound(&XUI_FIGURE_FILL_PROPS),
        PropFlags::from_bits(0x0),
        None,
    ),
    PropDef::new("Closed", PropType::Bool, PropFlags::from_bits(0x2), None),
    PropDef::new("Points", PropType::Custom, PropFlags::from_bits(0x2), None),
];

/// Registered at 0x9214af10.
pub static XUI_FIGURE: ClassDef = ClassDef::new("XuiFigure", Some("XuiElement"), &XUI_FIGURE_PROPS);

static XUI_CONTROL_PROPS: [PropDef; 13] = [
    PropDef::new(
        "ClassOverride",
        PropType::String,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "Visual",
        PropType::String,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "Enabled",
        PropType::Bool,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Bool(true)),
    ),
    PropDef::new(
        "UnfocusedInput",
        PropType::Bool,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Bool(false)),
    ),
    PropDef::new(
        "NavLeft",
        PropType::String,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "NavRight",
        PropType::String,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "NavUp",
        PropType::String,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "NavDown",
        PropType::String,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "NavTabForward",
        PropType::String,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "NavTabBackward",
        PropType::String,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "Text",
        PropType::String,
        PropFlags::from_bits(0x4),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "PointSize",
        PropType::Float,
        PropFlags::from_bits(0x6),
        Some(DefaultValue::Float(-1.0)),
    ),
    PropDef::new(
        "ImagePath",
        PropType::String,
        PropFlags::from_bits(0x10),
        Some(DefaultValue::String(None)),
    ),
];

/// Registered at 0x9214aba8.
pub static XUI_CONTROL: ClassDef =
    ClassDef::new("XuiControl", Some("XuiElement"), &XUI_CONTROL_PROPS);

static XUI_SCENE_PROPS: [PropDef; 7] = [
    PropDef::new(
        "DefaultFocus",
        PropType::String,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "TransFrom",
        PropType::String,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "TransTo",
        PropType::String,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "TransBackFrom",
        PropType::String,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "TransBackTo",
        PropType::String,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "InterruptTransitions",
        PropType::Unsigned,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::Unsigned(0)),
    ),
    PropDef::new(
        "IgnorePresses",
        PropType::Bool,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Bool(false)),
    ),
];

/// Registered at 0x9214b118.
pub static XUI_SCENE: ClassDef = ClassDef::new("XuiScene", Some("XuiControl"), &XUI_SCENE_PROPS);

static XUI_BUTTON_PROPS: [PropDef; 1] = [PropDef::new(
    "PressKey",
    PropType::Unsigned,
    PropFlags::from_bits(0x0),
    Some(DefaultValue::Unsigned(0x5840)),
)];

/// Registered at 0x92148188.
pub static XUI_BUTTON: ClassDef = ClassDef::new("XuiButton", Some("XuiControl"), &XUI_BUTTON_PROPS);

/// Registered at 0x92148308.
pub static XUI_BACK_BUTTON: ClassDef = ClassDef::new("XuiBackButton", Some("XuiButton"), &[]);

static XUI_NAV_BUTTON_PROPS: [PropDef; 4] = [
    PropDef::new(
        "PressPath",
        PropType::String,
        PropFlags::from_bits(0x10),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "StayVisible",
        PropType::Bool,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Bool(false)),
    ),
    PropDef::new(
        "SrcTransIndex",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(0)),
    ),
    PropDef::new(
        "DestTransIndex",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(0)),
    ),
];

/// Registered at 0x92148248.
pub static XUI_NAV_BUTTON: ClassDef =
    ClassDef::new("XuiNavButton", Some("XuiButton"), &XUI_NAV_BUTTON_PROPS);

static XUI_CHECKBOX_PROPS: [PropDef; 1] = [PropDef::new(
    "PressKey",
    PropType::Unsigned,
    PropFlags::from_bits(0x0),
    Some(DefaultValue::Unsigned(0x5840)),
)];

/// Registered at 0x921483b8.
pub static XUI_CHECKBOX: ClassDef =
    ClassDef::new("XuiCheckbox", Some("XuiControl"), &XUI_CHECKBOX_PROPS);

static XUI_RADIO_BUTTON_PROPS: [PropDef; 1] = [PropDef::new(
    "PressKey",
    PropType::Unsigned,
    PropFlags::from_bits(0x0),
    Some(DefaultValue::Unsigned(0x5840)),
)];

/// Registered at 0x92148478.
pub static XUI_RADIO_BUTTON: ClassDef = ClassDef::new(
    "XuiRadioButton",
    Some("XuiControl"),
    &XUI_RADIO_BUTTON_PROPS,
);

/// Registered at 0x92148538.
pub static XUI_RADIO_GROUP: ClassDef = ClassDef::new("XuiRadioGroup", Some("XuiControl"), &[]);

static XUI_TEXT_PROPS: [PropDef; 7] = [
    PropDef::new(
        "Text",
        PropType::String,
        PropFlags::from_bits(0x4),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "TextColor",
        PropType::Color,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Color(Color::from_argb(0xff000000))),
    ),
    PropDef::new(
        "DropShadowColor",
        PropType::Color,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Color(Color::from_argb(0x80000000))),
    ),
    PropDef::new(
        "PointSize",
        PropType::Float,
        PropFlags::from_bits(0xc),
        Some(DefaultValue::Float(14.0)),
    ),
    PropDef::new(
        "Font",
        PropType::String,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "TextStyle",
        PropType::Unsigned,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::Unsigned(0x10)),
    ),
    PropDef::new(
        "LineSpacingAdjust",
        PropType::Integer,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Integer(0)),
    ),
];

/// Registered at 0x92147f28.
pub static XUI_TEXT: ClassDef = ClassDef::new("XuiText", Some("XuiElement"), &XUI_TEXT_PROPS);

static XUI_TEXT_PRESENTER_PROPS: [PropDef; 7] = [
    PropDef::new(
        "TextColor",
        PropType::Color,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Color(Color::from_argb(0xff000000))),
    ),
    PropDef::new(
        "DropShadowColor",
        PropType::Color,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Color(Color::from_argb(0x80000000))),
    ),
    PropDef::new(
        "PointSize",
        PropType::Float,
        PropFlags::from_bits(0xc),
        Some(DefaultValue::Float(14.0)),
    ),
    PropDef::new(
        "Font",
        PropType::String,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "TextStyle",
        PropType::Unsigned,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::Unsigned(0x10)),
    ),
    PropDef::new(
        "LineSpacingAdjust",
        PropType::Integer,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Integer(0)),
    ),
    PropDef::new(
        "DataAssociation",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(0)),
    ),
];

/// Registered at 0x921485e8.
pub static XUI_TEXT_PRESENTER: ClassDef = ClassDef::new(
    "XuiTextPresenter",
    Some("XuiElement"),
    &XUI_TEXT_PRESENTER_PROPS,
);

/// Registered at 0x921480c8.
pub static XUI_VISUAL: ClassDef = ClassDef::new("XuiVisual", Some("XuiElement"), &[]);

/// Registered at 0x921486a8.
pub static XUI_GROUP: ClassDef = ClassDef::new("XuiGroup", Some("XuiElement"), &[]);

static XUI_SOUND_PROPS: [PropDef; 4] = [
    PropDef::new(
        "State",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(2)),
    ),
    PropDef::new(
        "Loop",
        PropType::Bool,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::Bool(false)),
    ),
    PropDef::new(
        "Finish",
        PropType::Bool,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::Bool(false)),
    ),
    PropDef::new(
        "Volume",
        PropType::Float,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Float(-12.0)),
    ),
];

/// Registered at 0x92148758.
pub static XUI_SOUND: ClassDef = ClassDef::new("XuiSound", Some("XuiElement"), &XUI_SOUND_PROPS);

/// Registered at 0x9214ac58.
pub static XUI_TRANSITION: ClassDef = ClassDef::new("XuiTransition", Some("XuiElement"), &[]);

static XUI_LIST_ITEM_PROPS: [PropDef; 5] = [
    PropDef::new(
        "Layout",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(0)),
    ),
    PropDef::new(
        "Checkable",
        PropType::Bool,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Bool(false)),
    ),
    PropDef::new(
        "SelectedSize",
        PropType::Vector,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Vector(Vector3::new(0.0, 0.0, 0.0))),
    ),
    PropDef::new(
        "KeepSizeUnfocused",
        PropType::Bool,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Bool(false)),
    ),
    PropDef::new(
        "InterItemSpacing",
        PropType::Vector,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Vector(Vector3::new(0.0, 0.0, 0.0))),
    ),
];

/// Registered at 0x92148828.
pub static XUI_LIST_ITEM: ClassDef =
    ClassDef::new("XuiListItem", Some("XuiCheckbox"), &XUI_LIST_ITEM_PROPS);

static XUI_SCROLL_END_PROPS: [PropDef; 1] = [PropDef::new(
    "Direction",
    PropType::Unsigned,
    PropFlags::from_bits(0x0),
    Some(DefaultValue::Unsigned(0)),
)];

/// Registered at 0x921488e8.
pub static XUI_SCROLL_END: ClassDef =
    ClassDef::new("XuiScrollEnd", Some("XuiControl"), &XUI_SCROLL_END_PROPS);

static XUI_SCROLL_BAR_PROPS: [PropDef; 2] = [
    PropDef::new(
        "Direction",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(0)),
    ),
    PropDef::new(
        "MinThumbSize",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(0xa)),
    ),
];

/// Registered at 0x921489a8.
pub static XUI_SCROLL_BAR: ClassDef =
    ClassDef::new("XuiScrollBar", Some("XuiControl"), &XUI_SCROLL_BAR_PROPS);

static XUI_LIST_PROPS: [PropDef; 1] = [PropDef::new(
    "Wrap",
    PropType::Bool,
    PropFlags::from_bits(0x0),
    Some(DefaultValue::Bool(false)),
)];

/// Registered at 0x92148a68.
pub static XUI_LIST: ClassDef = ClassDef::new("XuiList", Some("XuiControl"), &XUI_LIST_PROPS);

static XUI_COMMON_LIST_PROPS: [PropDef; 3] = [
    PropDef::new(
        "ItemsText",
        PropType::String,
        PropFlags::from_bits(0x4),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "ItemsImage",
        PropType::String,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "ItemsNavPath",
        PropType::String,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::String(None)),
    ),
];

/// Registered at 0x9214b2b8.
pub static XUI_COMMON_LIST: ClassDef =
    ClassDef::new("XuiCommonList", Some("XuiList"), &XUI_COMMON_LIST_PROPS);

static XUI_PROGRESS_BAR_PROPS: [PropDef; 3] = [
    PropDef::new(
        "RangeMin",
        PropType::Integer,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::Integer(0)),
    ),
    PropDef::new(
        "RangeMax",
        PropType::Integer,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::Integer(100)),
    ),
    PropDef::new(
        "Value",
        PropType::Integer,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Integer(0)),
    ),
];

/// Registered at 0x92148b38.
pub static XUI_PROGRESS_BAR: ClassDef = ClassDef::new(
    "XuiProgressBar",
    Some("XuiControl"),
    &XUI_PROGRESS_BAR_PROPS,
);

static XUI_SLIDER_PROPS: [PropDef; 7] = [
    PropDef::new(
        "RangeMin",
        PropType::Integer,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::Integer(0)),
    ),
    PropDef::new(
        "RangeMax",
        PropType::Integer,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::Integer(100)),
    ),
    PropDef::new(
        "Value",
        PropType::Integer,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Integer(0)),
    ),
    PropDef::new(
        "Step",
        PropType::Integer,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Integer(1)),
    ),
    PropDef::new(
        "Vertical",
        PropType::Bool,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Bool(false)),
    ),
    PropDef::new(
        "AccelInc",
        PropType::Integer,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Integer(0)),
    ),
    PropDef::new(
        "AccelTime",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(0)),
    ),
];

/// Registered at 0x92148bf8.
pub static XUI_SLIDER: ClassDef = ClassDef::new("XuiSlider", Some("XuiControl"), &XUI_SLIDER_PROPS);

/// Registered at 0x92148d78.
pub static XUI_CARET: ClassDef = ClassDef::new("XuiCaret", Some("XuiControl"), &[]);

static XUI_EDIT_PROPS: [PropDef; 6] = [
    PropDef::new(
        "TextLimit",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(0xfffe)),
    ),
    PropDef::new(
        "AllowedChars",
        PropType::String,
        PropFlags::from_bits(0x4),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "PasswordChar",
        PropType::String,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "ReadOnly",
        PropType::Bool,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Bool(false)),
    ),
    PropDef::new(
        "Multiline",
        PropType::Bool,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Bool(false)),
    ),
    PropDef::new(
        "SmoothScroll",
        PropType::Bool,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Bool(false)),
    ),
];

/// Registered at 0x92148cb8.
pub static XUI_EDIT: ClassDef = ClassDef::new("XuiEdit", Some("XuiControl"), &XUI_EDIT_PROPS);

static XUI_IMAGE_PRESENTER_PROPS: [PropDef; 3] = [
    PropDef::new(
        "SizeMode",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(0)),
    ),
    PropDef::new(
        "DataAssociation",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(0)),
    ),
    PropDef::new(
        "BrushFlags",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(0)),
    ),
];

/// Registered at 0x92148ee8.
pub static XUI_IMAGE_PRESENTER: ClassDef = ClassDef::new(
    "XuiImagePresenter",
    Some("XuiElement"),
    &XUI_IMAGE_PRESENTER_PROPS,
);

static XUI_IMAGE_PROPS: [PropDef; 3] = [
    PropDef::new(
        "SizeMode",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(0)),
    ),
    PropDef::new(
        "ImagePath",
        PropType::String,
        PropFlags::from_bits(0x10),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "BrushFlags",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(0)),
    ),
];

/// Registered at 0x92148fa8.
pub static XUI_IMAGE: ClassDef = ClassDef::new("XuiImage", Some("XuiElement"), &XUI_IMAGE_PROPS);

/// Registered at 0x92148e28.
pub static XUI_MESSAGE_BOX: ClassDef = ClassDef::new("XuiMessageBox", Some("XuiScene"), &[]);

/// Registered at 0x92147fe8.
pub static XUI_LABEL: ClassDef = ClassDef::new("XuiLabel", Some("XuiControl"), &[]);

static XUI_TAB_SCENE_PROPS: [PropDef; 6] = [
    PropDef::new(
        "TabCount",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(1)),
    ),
    PropDef::new(
        "Wrap",
        PropType::Bool,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Bool(false)),
    ),
    PropDef::new(
        "UserInterrupt",
        PropType::Bool,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Bool(false)),
    ),
    PropDef::new(
        "VerticalTabs",
        PropType::Bool,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Bool(false)),
    ),
    PropDef::new(
        "NoAutoHide",
        PropType::Bool,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Bool(false)),
    ),
    PropDef::new(
        "DefaultTab",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(1)),
    ),
];

/// Registered at 0x92149068.
pub static XUI_TAB_SCENE: ClassDef =
    ClassDef::new("XuiTabScene", Some("XuiScene"), &XUI_TAB_SCENE_PROPS);

static XUI_NINE_GRID_PROPS: [PropDef; 6] = [
    PropDef::new(
        "TextureFileName",
        PropType::String,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "LeftOffset",
        PropType::Unsigned,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::Unsigned(0)),
    ),
    PropDef::new(
        "TopOffset",
        PropType::Unsigned,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::Unsigned(0)),
    ),
    PropDef::new(
        "RightOffset",
        PropType::Unsigned,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::Unsigned(0)),
    ),
    PropDef::new(
        "BottomOffset",
        PropType::Unsigned,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::Unsigned(0)),
    ),
    PropDef::new(
        "NoCenter",
        PropType::Bool,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::Bool(false)),
    ),
];

/// Registered at 0x92149128.
pub static XUI_NINE_GRID: ClassDef =
    ClassDef::new("XuiNineGrid", Some("XuiElement"), &XUI_NINE_GRID_PROPS);

static XUI_EFFECT_PROPS: [PropDef; 1] = [PropDef::new(
    "Id",
    PropType::String,
    PropFlags::from_bits(0x0),
    Some(DefaultValue::String(None)),
)];

/// Registered at 0x9214acf8.
pub static XUI_EFFECT: ClassDef = ClassDef::new("XuiEffect", Some("XuiElement"), &XUI_EFFECT_PROPS);

static XUI_TEXTURE_EFFECT_PROPS: [PropDef; 10] = [
    PropDef::new(
        "TextureFileName",
        PropType::String,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::String(None)),
    ),
    PropDef::new(
        "Translation",
        PropType::Vector,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Vector(Vector3::new(0.0, 0.0, 0.0))),
    ),
    PropDef::new(
        "Scale",
        PropType::Vector,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Vector(Vector3::new(1.0, 1.0, 1.0))),
    ),
    PropDef::new(
        "Rotation",
        PropType::Float,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Float(0.0)),
    ),
    PropDef::new(
        "ColorFactor",
        PropType::Quaternion,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Quaternion(Quaternion::new(
            1.0, 1.0, 1.0, 1.0,
        ))),
    ),
    PropDef::new(
        "SrcBlendFactor",
        PropType::Unsigned,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::Unsigned(4)),
    ),
    PropDef::new(
        "DstBlendFactor",
        PropType::Unsigned,
        PropFlags::from_bits(0x8),
        Some(DefaultValue::Unsigned(5)),
    ),
    PropDef::new(
        "WrapX",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(2)),
    ),
    PropDef::new(
        "WrapY",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(2)),
    ),
    PropDef::new(
        "BrushFlags",
        PropType::Unsigned,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Unsigned(0)),
    ),
];

/// Registered at 0x921491f8.
pub static XUI_TEXTURE_EFFECT: ClassDef = ClassDef::new(
    "XuiTextureEffect",
    Some("XuiEffect"),
    &XUI_TEXTURE_EFFECT_PROPS,
);

static XUI_GRAYSCALE_EFFECT_PROPS: [PropDef; 1] = [PropDef::new(
    "GrayAmount",
    PropType::Float,
    PropFlags::from_bits(0x0),
    Some(DefaultValue::Float(1.0)),
)];

/// Registered at 0x921492b8.
pub static XUI_GRAYSCALE_EFFECT: ClassDef = ClassDef::new(
    "XuiGrayscaleEffect",
    Some("XuiEffect"),
    &XUI_GRAYSCALE_EFFECT_PROPS,
);

static XUI_RECOLOR_EFFECT_PROPS: [PropDef; 1] = [PropDef::new(
    "ColorFactor",
    PropType::Quaternion,
    PropFlags::from_bits(0x0),
    Some(DefaultValue::Quaternion(Quaternion::new(
        1.0, 1.0, 1.0, 1.0,
    ))),
)];

/// Registered at 0x92149378.
pub static XUI_RECOLOR_EFFECT: ClassDef = ClassDef::new(
    "XuiRecolorEffect",
    Some("XuiEffect"),
    &XUI_RECOLOR_EFFECT_PROPS,
);

static XUI_BRIGHT_PASS_EFFECT_PROPS: [PropDef; 1] = [PropDef::new(
    "Threshold",
    PropType::Float,
    PropFlags::from_bits(0x0),
    Some(DefaultValue::Float(0.75)),
)];

/// Registered at 0x92149438.
pub static XUI_BRIGHT_PASS_EFFECT: ClassDef = ClassDef::new(
    "XuiBrightPassEffect",
    Some("XuiEffect"),
    &XUI_BRIGHT_PASS_EFFECT_PROPS,
);

/// Registered at 0x921494f8.
pub static XUI_EDGE_EFFECT: ClassDef = ClassDef::new("XuiEdgeEffect", Some("XuiEffect"), &[]);

static XUI_HV_BLUR_EFFECT_PROPS: [PropDef; 2] = [
    PropDef::new(
        "Brightness",
        PropType::Float,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Float(1.0)),
    ),
    PropDef::new(
        "BlurAmount",
        PropType::Float,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Float(0.0)),
    ),
];

/// Registered at 0x9214ada8.
pub static XUI_HV_BLUR_EFFECT: ClassDef = ClassDef::new(
    "XuiHVBlurEffect",
    Some("XuiEffect"),
    &XUI_HV_BLUR_EFFECT_PROPS,
);

static XUI_H_BLUR_EFFECT_PROPS: [PropDef; 2] = [
    PropDef::new(
        "Brightness",
        PropType::Float,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Float(1.0)),
    ),
    PropDef::new(
        "BlurAmount",
        PropType::Float,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Float(0.0)),
    ),
];

/// Registered at 0x921495b8.
pub static XUI_H_BLUR_EFFECT: ClassDef = ClassDef::new(
    "XuiHBlurEffect",
    Some("XuiEffect"),
    &XUI_H_BLUR_EFFECT_PROPS,
);

static XUI_V_BLUR_EFFECT_PROPS: [PropDef; 2] = [
    PropDef::new(
        "Brightness",
        PropType::Float,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Float(1.0)),
    ),
    PropDef::new(
        "BlurAmount",
        PropType::Float,
        PropFlags::from_bits(0x0),
        Some(DefaultValue::Float(0.0)),
    ),
];

/// Registered at 0x92149678.
pub static XUI_V_BLUR_EFFECT: ClassDef = ClassDef::new(
    "XuiVBlurEffect",
    Some("XuiEffect"),
    &XUI_V_BLUR_EFFECT_PROPS,
);

static XUI_SOUND_X_AUDIO_PROPS: [PropDef; 1] = [PropDef::new(
    "File",
    PropType::String,
    PropFlags::from_bits(0x10),
    Some(DefaultValue::String(None)),
)];

/// Registered at 0x9214c5b8.
pub static XUI_SOUND_X_AUDIO: ClassDef =
    ClassDef::new("XuiSoundXAudio", Some("XuiSound"), &XUI_SOUND_X_AUDIO_PROPS);

static XUI_HTML_ELEMENT_PROPS: [PropDef; 1] = [PropDef::new(
    "Text",
    PropType::String,
    PropFlags::from_bits(0x4),
    Some(DefaultValue::String(None)),
)];

/// Registered at 0x92197a48.
pub static XUI_HTML_ELEMENT: ClassDef = ClassDef::new(
    "XuiHtmlElement",
    Some("XuiElement"),
    &XUI_HTML_ELEMENT_PROPS,
);

static XUI_HTML_PRESENTER_PROPS: [PropDef; 1] = [PropDef::new(
    "DataAssociation",
    PropType::Unsigned,
    PropFlags::from_bits(0x0),
    Some(DefaultValue::Unsigned(0)),
)];

/// Registered at 0x92198f30.
pub static XUI_HTML_PRESENTER: ClassDef = ClassDef::new(
    "XuiHtmlPresenter",
    Some("XuiElement"),
    &XUI_HTML_PRESENTER_PROPS,
);

static XUI_HTML_CONTROL_PROPS: [PropDef; 1] = [PropDef::new(
    "LineHeight",
    PropType::Integer,
    PropFlags::from_bits(0xc),
    Some(DefaultValue::Integer(28)),
)];

/// Registered at 0x92199970.
pub static XUI_HTML_CONTROL: ClassDef = ClassDef::new(
    "XuiHtmlControl",
    Some("XuiControl"),
    &XUI_HTML_CONTROL_PROPS,
);

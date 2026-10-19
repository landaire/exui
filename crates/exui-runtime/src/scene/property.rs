//! Property values and the per-property setters and getters.

use alloc::boxed::Box;
use core::ptr;

use exui_core::custom::FigurePoints;
use exui_core::schema::ClassDef;
use exui_core::schema::DefaultValue;
use exui_core::schema::PropDef;
use exui_core::schema::ScalarType;
use exui_core::schema::xui;
use exui_core::value::Color;
use exui_core::value::Quaternion;
use exui_core::value::Vector3;

use crate::scene::Anchor;
use crate::scene::Element;
use crate::scene::ElementFlags;
use crate::scene::PropertyError;
use crate::scene::XuiBlendMode;
use crate::scene::content::Content;
use crate::scene::figure::GradientStops;
use crate::scene::image::SizeMode;
use crate::scene::paint::TextureFlags;

/// An owned property value.
#[derive(Clone, Debug, PartialEq)]
pub enum PropValue {
    Bool(bool),
    Integer(i32),
    Unsigned(u32),
    Float(f32),
    /// `None` is the null string.
    String(Option<Box<[u16]>>),
    Color(Color),
    Vector(Vector3),
    Quaternion(Quaternion),
    /// The decoded `XuiFigure.Points` custom value.
    Points(FigurePoints),
}

impl PropValue {
    pub fn scalar_type(&self) -> ScalarType {
        match self {
            Self::Bool(_) => ScalarType::Bool,
            Self::Integer(_) => ScalarType::Integer,
            Self::Unsigned(_) => ScalarType::Unsigned,
            Self::Float(_) => ScalarType::Float,
            Self::String(_) => ScalarType::String,
            Self::Color(_) => ScalarType::Color,
            Self::Vector(_) => ScalarType::Vector,
            Self::Quaternion(_) => ScalarType::Quaternion,
            Self::Points(_) => ScalarType::Custom,
        }
    }

    pub fn from_default(default: DefaultValue) -> Self {
        match default {
            DefaultValue::Bool(value) => Self::Bool(value),
            DefaultValue::Integer(value) => Self::Integer(value),
            DefaultValue::Unsigned(value) => Self::Unsigned(value),
            DefaultValue::Float(value) => Self::Float(value),
            DefaultValue::String(value) => {
                Self::String(value.map(|text| text.encode_utf16().collect()))
            }
            DefaultValue::Color(value) => Self::Color(value),
            DefaultValue::Vector(value) => Self::Vector(value),
            DefaultValue::Quaternion(value) => Self::Quaternion(value),
        }
    }

    fn mismatch(&self, def: &'static PropDef) -> PropertyError {
        PropertyError::TypeMismatch {
            name: def.name,
            expected: def.ty.scalar(),
            found: self.scalar_type(),
        }
    }

    fn bool(&self, def: &'static PropDef) -> Result<bool, PropertyError> {
        match *self {
            Self::Bool(value) => Ok(value),
            _ => Err(self.mismatch(def)),
        }
    }

    fn integer(&self, def: &'static PropDef) -> Result<i32, PropertyError> {
        match *self {
            Self::Integer(value) => Ok(value),
            _ => Err(self.mismatch(def)),
        }
    }

    fn unsigned(&self, def: &'static PropDef) -> Result<u32, PropertyError> {
        match *self {
            Self::Unsigned(value) => Ok(value),
            _ => Err(self.mismatch(def)),
        }
    }

    fn float(&self, def: &'static PropDef) -> Result<f32, PropertyError> {
        match *self {
            Self::Float(value) => Ok(value),
            _ => Err(self.mismatch(def)),
        }
    }

    fn color(&self, def: &'static PropDef) -> Result<Color, PropertyError> {
        match *self {
            Self::Color(value) => Ok(value),
            _ => Err(self.mismatch(def)),
        }
    }

    fn vector(&self, def: &'static PropDef) -> Result<Vector3, PropertyError> {
        match *self {
            Self::Vector(value) => Ok(value),
            _ => Err(self.mismatch(def)),
        }
    }

    fn quaternion(&self, def: &'static PropDef) -> Result<Quaternion, PropertyError> {
        match *self {
            Self::Quaternion(value) => Ok(value),
            _ => Err(self.mismatch(def)),
        }
    }

    fn string(self, def: &'static PropDef) -> Result<Option<Box<[u16]>>, PropertyError> {
        match self {
            Self::String(value) => Ok(value),
            other => Err(other.mismatch(def)),
        }
    }
}

/// A definition's position within its class or compound, in schema order.
/// Variants are declared in schema order, so `variant as usize` is the
/// position.
pub(crate) trait SchemaPosition: Copy + 'static {
    const ORDER: &'static [Self];

    fn from_position(position: usize) -> Option<Self> {
        Self::ORDER.get(position).copied()
    }
}

/// XuiElement properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ElementProp {
    Id,
    Width,
    Height,
    Position,
    Scale,
    Rotation,
    Opacity,
    Anchor,
    Pivot,
    Show,
    BlendMode,
    DisableTimelineRecursion,
    DesignTime,
    ColorWriteFlags,
    ClipChildren,
    EnableEffects,
    DisableFocusRecursion,
}

impl SchemaPosition for ElementProp {
    const ORDER: &'static [Self] = &[
        Self::Id,
        Self::Width,
        Self::Height,
        Self::Position,
        Self::Scale,
        Self::Rotation,
        Self::Opacity,
        Self::Anchor,
        Self::Pivot,
        Self::Show,
        Self::BlendMode,
        Self::DisableTimelineRecursion,
        Self::DesignTime,
        Self::ColorWriteFlags,
        Self::ClipChildren,
        Self::EnableEffects,
        Self::DisableFocusRecursion,
    ];
}

/// XuiControl properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ControlProp {
    ClassOverride,
    Visual,
    Enabled,
    UnfocusedInput,
    NavLeft,
    NavRight,
    NavUp,
    NavDown,
    NavTabForward,
    NavTabBackward,
    Text,
    PointSize,
    ImagePath,
}

impl SchemaPosition for ControlProp {
    const ORDER: &'static [Self] = &[
        Self::ClassOverride,
        Self::Visual,
        Self::Enabled,
        Self::UnfocusedInput,
        Self::NavLeft,
        Self::NavRight,
        Self::NavUp,
        Self::NavDown,
        Self::NavTabForward,
        Self::NavTabBackward,
        Self::Text,
        Self::PointSize,
        Self::ImagePath,
    ];
}

/// XuiFigure properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FigureProp {
    Stroke,
    Fill,
    Closed,
    Points,
}

impl SchemaPosition for FigureProp {
    const ORDER: &'static [Self] = &[Self::Stroke, Self::Fill, Self::Closed, Self::Points];
}

/// Members of the XuiFigure Stroke compound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StrokeMember {
    StrokeWidth,
    StrokeColor,
}

impl SchemaPosition for StrokeMember {
    const ORDER: &'static [Self] = &[Self::StrokeWidth, Self::StrokeColor];
}

/// Members of the XuiFigure Fill compound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FillMember {
    FillType,
    FillColor,
    TextureFileName,
    Gradient,
    Translation,
    Scale,
    Rotation,
    WrapX,
    WrapY,
    BrushFlags,
    TransformVersion,
}

impl SchemaPosition for FillMember {
    const ORDER: &'static [Self] = &[
        Self::FillType,
        Self::FillColor,
        Self::TextureFileName,
        Self::Gradient,
        Self::Translation,
        Self::Scale,
        Self::Rotation,
        Self::WrapX,
        Self::WrapY,
        Self::BrushFlags,
        Self::TransformVersion,
    ];
}

/// Members of the Fill.Gradient compound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GradientMember {
    Radial,
    NumStops,
    StopColor,
    StopPos,
}

impl SchemaPosition for GradientMember {
    const ORDER: &'static [Self] = &[Self::Radial, Self::NumStops, Self::StopColor, Self::StopPos];
}

/// XuiText properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextProp {
    Text,
    TextColor,
    DropShadowColor,
    PointSize,
    Font,
    TextStyle,
    LineSpacingAdjust,
}

impl SchemaPosition for TextProp {
    const ORDER: &'static [Self] = &[
        Self::Text,
        Self::TextColor,
        Self::DropShadowColor,
        Self::PointSize,
        Self::Font,
        Self::TextStyle,
        Self::LineSpacingAdjust,
    ];
}

/// XuiImage properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImageProp {
    SizeMode,
    ImagePath,
    BrushFlags,
}

impl SchemaPosition for ImageProp {
    const ORDER: &'static [Self] = &[Self::SizeMode, Self::ImagePath, Self::BrushFlags];
}

/// XuiNineGrid properties.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NineGridProp {
    TextureFileName,
    LeftOffset,
    TopOffset,
    RightOffset,
    BottomOffset,
    NoCenter,
}

impl SchemaPosition for NineGridProp {
    const ORDER: &'static [Self] = &[
        Self::TextureFileName,
        Self::LeftOffset,
        Self::TopOffset,
        Self::RightOffset,
        Self::BottomOffset,
        Self::NoCenter,
    ];
}

/// A property without a dedicated field.
#[derive(Clone, Debug)]
pub(crate) struct StoredProperty {
    pub(crate) path: Box<[&'static PropDef]>,
    pub(crate) index: Option<u32>,
    pub(crate) value: PropValue,
}

impl StoredProperty {
    fn matches(&self, path: &[&'static PropDef], index: Option<u32>) -> bool {
        self.index == index
            && self.path.len() == path.len()
            && self.path.iter().zip(path).all(|(a, b)| ptr::eq(*a, *b))
    }
}

/// Follow-up work of a setter that needs the tree.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Effect {
    None,
    /// Width setter: SetBounds with the new width, then design capture.
    Width(f32),
    /// Height setter.
    Height(f32),
    /// Position setter (`XuiElementSetPosition`).
    Position(Vector3),
    /// Scale setter (`XuiElement_SetScaleAnchored`).
    Scale(Vector3),
}

/// Class level and position of a handled top-level definition.
#[derive(Clone, Copy, Debug)]
enum Field {
    Element(ElementProp),
    Control(ControlProp),
    Figure(FigureProp),
    Text(TextProp),
    TextPresenterAssociation,
    Image(ImageProp),
    ImagePresenterAssociation,
    NineGrid(NineGridProp),
}

/// XuiTextPresenter properties: the XuiText properties after Text, then
/// DataAssociation.
const TEXT_PRESENTER_FIELDS: [Field; 7] = [
    Field::Text(TextProp::TextColor),
    Field::Text(TextProp::DropShadowColor),
    Field::Text(TextProp::PointSize),
    Field::Text(TextProp::Font),
    Field::Text(TextProp::TextStyle),
    Field::Text(TextProp::LineSpacingAdjust),
    Field::TextPresenterAssociation,
];

/// XuiImagePresenter properties.
const IMAGE_PRESENTER_FIELDS: [Field; 3] = [
    Field::Image(ImageProp::SizeMode),
    Field::ImagePresenterAssociation,
    Field::Image(ImageProp::BrushFlags),
];

fn position_in(class: &'static ClassDef, def: &PropDef) -> Option<usize> {
    class
        .props
        .iter()
        .position(|candidate| ptr::eq(candidate, def))
}

fn locate(def: &'static PropDef) -> Option<Field> {
    if let Some(index) = position_in(&xui::XUI_ELEMENT, def) {
        return ElementProp::from_position(index).map(Field::Element);
    }
    if let Some(index) = position_in(&xui::XUI_CONTROL, def) {
        return ControlProp::from_position(index).map(Field::Control);
    }
    if let Some(index) = position_in(&xui::XUI_FIGURE, def) {
        return FigureProp::from_position(index).map(Field::Figure);
    }
    if let Some(index) = position_in(&xui::XUI_TEXT, def) {
        return TextProp::from_position(index).map(Field::Text);
    }
    if let Some(index) = position_in(&xui::XUI_TEXT_PRESENTER, def) {
        return TEXT_PRESENTER_FIELDS.get(index).copied();
    }
    if let Some(index) = position_in(&xui::XUI_IMAGE, def) {
        return ImageProp::from_position(index).map(Field::Image);
    }
    if let Some(index) = position_in(&xui::XUI_IMAGE_PRESENTER, def) {
        return IMAGE_PRESENTER_FIELDS.get(index).copied();
    }
    if let Some(index) = position_in(&xui::XUI_NINE_GRID, def) {
        return NineGridProp::from_position(index).map(Field::NineGrid);
    }

    None
}

/// Result of [`store_field`].
#[derive(Debug)]
pub(crate) enum Stored {
    Handled(Effect),
    /// The property has no dedicated field; the value is returned.
    Unhandled(PropValue),
}

/// Writes a property that has a dedicated field.
pub(crate) fn store_field(
    element: &mut Element,
    path: &[&'static PropDef],
    index: Option<u32>,
    value: PropValue,
) -> Result<Stored, PropertyError> {
    let (&top, members) = path.split_first().ok_or(PropertyError::EmptyPath)?;
    let def = path[path.len() - 1];
    if def.ty.compound().is_some() {
        return Err(PropertyError::Compound { name: def.name });
    }
    let Some(field) = locate(top) else {
        return Ok(Stored::Unhandled(value));
    };
    let not_on_class = PropertyError::NotOnClass { name: def.name };

    match field {
        Field::Element(position) => {
            store_element(element, position, def, value).map(Stored::Handled)
        }
        Field::Control(position) => {
            let control = element.control.as_mut().ok_or(not_on_class)?;
            match position {
                ControlProp::Visual => control.visual = value.string(def)?,
                // 0x7e0: an empty string stores NULL.
                ControlProp::Text => {
                    control.text = value.string(def)?.filter(|text| !text.is_empty())
                }
                // -1.0, the XuiControl default, selects the presenter's
                // PointSize.
                ControlProp::PointSize => {
                    let size = value.float(def)?;
                    control.point_size = (size != -1.0).then_some(size);
                }
                ControlProp::ImagePath => control.image_path = value.string(def)?,
                _ => return Ok(Stored::Unhandled(value)),
            }
            Ok(Stored::Handled(Effect::None))
        }
        Field::Figure(position) => {
            let Content::Figure(figure) = &mut element.content else {
                return Err(not_on_class);
            };
            figure.store(position, members, index, def, value)?;
            Ok(Stored::Handled(Effect::None))
        }
        Field::Text(position) => {
            let Content::Text(text) = &mut element.content else {
                return Err(not_on_class);
            };
            match position {
                TextProp::Text => text.text = value.string(def)?,
                TextProp::TextColor => text.color = value.color(def)?,
                TextProp::DropShadowColor => text.shadow_color = value.color(def)?,
                TextProp::PointSize => text.point_size = value.float(def)?,
                TextProp::Font => text.font = value.string(def)?,
                TextProp::TextStyle => text.style = value.unsigned(def)?,
                TextProp::LineSpacingAdjust => text.line_spacing_adjust = value.integer(def)?,
            }
            Ok(Stored::Handled(Effect::None))
        }
        Field::TextPresenterAssociation => {
            let Content::Text(text) = &mut element.content else {
                return Err(not_on_class);
            };
            text.data_association = value.unsigned(def)?;
            Ok(Stored::Handled(Effect::None))
        }
        Field::Image(position) => {
            let Content::Image(image) = &mut element.content else {
                return Err(not_on_class);
            };
            match position {
                ImageProp::SizeMode => image.size_mode = SizeMode(value.unsigned(def)?),
                ImageProp::ImagePath => {
                    // XuiImage_SetPath releases the brush. Scene content is
                    // destroyed by the setter or the next load.
                    image.path = value.string(def)?;
                    image.texture = None;
                    if let Some(scene) = image.scene.take() {
                        image.released_scene = Some(scene);
                    }
                }
                ImageProp::BrushFlags => image.brush_flags = TextureFlags(value.unsigned(def)?),
            }
            Ok(Stored::Handled(Effect::None))
        }
        Field::ImagePresenterAssociation => {
            let Content::Image(image) = &mut element.content else {
                return Err(not_on_class);
            };
            image.data_association = value.unsigned(def)?;
            Ok(Stored::Handled(Effect::None))
        }
        Field::NineGrid(position) => {
            let Content::NineGrid(grid) = &mut element.content else {
                return Err(not_on_class);
            };
            match position {
                NineGridProp::TextureFileName => {
                    grid.path = value.string(def)?;
                    grid.texture = None;
                }
                NineGridProp::LeftOffset => grid.left = value.unsigned(def)?,
                NineGridProp::TopOffset => grid.top = value.unsigned(def)?,
                NineGridProp::RightOffset => grid.right = value.unsigned(def)?,
                NineGridProp::BottomOffset => grid.bottom = value.unsigned(def)?,
                NineGridProp::NoCenter => grid.no_center = value.bool(def)?,
            }
            Ok(Stored::Handled(Effect::None))
        }
    }
}

fn store_element(
    element: &mut Element,
    position: ElementProp,
    def: &'static PropDef,
    value: PropValue,
) -> Result<Effect, PropertyError> {
    let flag = match position {
        ElementProp::Id => {
            element.id = value.string(def)?;
            return Ok(Effect::None);
        }
        ElementProp::Width => return Ok(Effect::Width(value.float(def)?)),
        ElementProp::Height => return Ok(Effect::Height(value.float(def)?)),
        ElementProp::Position => return Ok(Effect::Position(value.vector(def)?)),
        ElementProp::Scale => return Ok(Effect::Scale(value.vector(def)?)),
        ElementProp::Rotation => {
            element.rotation = value.quaternion(def)?;
            element.flags.set(ElementFlags::TRANSFORM_DIRTY, true);
            return Ok(Effect::None);
        }
        ElementProp::Opacity => {
            element.opacity = value.float(def)?;
            return Ok(Effect::None);
        }
        ElementProp::Anchor => {
            element.anchor = Anchor(value.unsigned(def)?);
            return Ok(Effect::None);
        }
        ElementProp::Pivot => {
            element.pivot = value.vector(def)?;
            element.flags.set(ElementFlags::TRANSFORM_DIRTY, true);
            return Ok(Effect::None);
        }
        ElementProp::Show => ElementFlags::SHOW,
        ElementProp::BlendMode => {
            element.blend_mode = XuiBlendMode::from_raw(value.unsigned(def)?);
            return Ok(Effect::None);
        }
        ElementProp::DisableTimelineRecursion => ElementFlags::DISABLE_TIMELINE_RECURSION,
        ElementProp::DesignTime => ElementFlags::DESIGN_TIME,
        ElementProp::ColorWriteFlags => {
            element.flags.set_color_write(value.unsigned(def)?);
            return Ok(Effect::None);
        }
        ElementProp::ClipChildren => ElementFlags::CLIP_CHILDREN,
        ElementProp::EnableEffects => ElementFlags::ENABLE_EFFECTS,
        ElementProp::DisableFocusRecursion => ElementFlags::DISABLE_FOCUS_RECURSION,
    };
    element.flags.set(flag, value.bool(def)?);

    Ok(Effect::None)
}

/// Reads a handled string property; outer `None` when the property has no
/// dedicated field.
pub(crate) fn fetch_string<'e>(
    element: &'e Element,
    path: &[&'static PropDef],
) -> Option<Option<&'e [u16]>> {
    let (&top, members) = path.split_first()?;
    let text = match (locate(top)?, &element.content) {
        (Field::Element(ElementProp::Id), _) => element.id.as_deref(),
        (Field::Control(ControlProp::Visual), _) => element.control.as_ref()?.visual.as_deref(),
        (Field::Control(ControlProp::Text), _) => element.control.as_ref()?.text.as_deref(),
        (Field::Control(ControlProp::ImagePath), _) => {
            element.control.as_ref()?.image_path.as_deref()
        }
        (Field::Text(TextProp::Text), Content::Text(text)) => text.text.as_deref(),
        (Field::Text(TextProp::Font), Content::Text(text)) => text.font.as_deref(),
        (Field::Image(ImageProp::ImagePath), Content::Image(image)) => image.path.as_deref(),
        (Field::NineGrid(NineGridProp::TextureFileName), Content::NineGrid(grid)) => {
            grid.path.as_deref()
        }
        (Field::Figure(FigureProp::Fill), Content::Figure(figure))
            if members.len() == 1
                && ptr::eq(members[0], fill_member(FillMember::TextureFileName)) =>
        {
            figure.fill.texture_path.as_deref()
        }
        _ => return None,
    };

    Some(text)
}

/// Reads a handled non-string property.
pub(crate) fn fetch_value(
    element: &Element,
    path: &[&'static PropDef],
    index: Option<u32>,
) -> Option<PropValue> {
    let (&top, members) = path.split_first()?;
    let flag = |bits| Some(PropValue::Bool(element.flags.contains(bits)));
    match (locate(top)?, &element.content) {
        (Field::Element(position), _) => match position {
            ElementProp::Id => None,
            ElementProp::Width => Some(PropValue::Float(element.size.width)),
            ElementProp::Height => Some(PropValue::Float(element.size.height)),
            ElementProp::Position => Some(PropValue::Vector(element.position)),
            ElementProp::Scale => Some(PropValue::Vector(element.scale)),
            ElementProp::Rotation => Some(PropValue::Quaternion(element.rotation)),
            ElementProp::Opacity => Some(PropValue::Float(element.opacity)),
            ElementProp::Anchor => Some(PropValue::Unsigned(element.anchor.0)),
            ElementProp::Pivot => Some(PropValue::Vector(element.pivot)),
            ElementProp::Show => flag(ElementFlags::SHOW),
            ElementProp::BlendMode => Some(PropValue::Unsigned(element.blend_mode.raw())),
            ElementProp::DisableTimelineRecursion => flag(ElementFlags::DISABLE_TIMELINE_RECURSION),
            ElementProp::DesignTime => flag(ElementFlags::DESIGN_TIME),
            ElementProp::ColorWriteFlags => Some(PropValue::Unsigned(element.flags.color_write())),
            ElementProp::ClipChildren => flag(ElementFlags::CLIP_CHILDREN),
            ElementProp::EnableEffects => flag(ElementFlags::ENABLE_EFFECTS),
            ElementProp::DisableFocusRecursion => flag(ElementFlags::DISABLE_FOCUS_RECURSION),
        },
        (Field::Control(ControlProp::PointSize), _) => Some(PropValue::Float(
            element.control.as_ref()?.point_size.unwrap_or(-1.0),
        )),
        (Field::Text(position), Content::Text(text)) => match position {
            TextProp::TextColor => Some(PropValue::Color(text.color)),
            TextProp::DropShadowColor => Some(PropValue::Color(text.shadow_color)),
            TextProp::PointSize => Some(PropValue::Float(text.point_size)),
            TextProp::TextStyle => Some(PropValue::Unsigned(text.style)),
            TextProp::LineSpacingAdjust => Some(PropValue::Integer(text.line_spacing_adjust)),
            TextProp::Text | TextProp::Font => None,
        },
        (Field::TextPresenterAssociation, Content::Text(text)) => {
            Some(PropValue::Unsigned(text.data_association))
        }
        (Field::Image(ImageProp::SizeMode), Content::Image(image)) => {
            Some(PropValue::Unsigned(image.size_mode.0))
        }
        (Field::Image(ImageProp::BrushFlags), Content::Image(image)) => {
            Some(PropValue::Unsigned(image.brush_flags.0))
        }
        (Field::ImagePresenterAssociation, Content::Image(image)) => {
            Some(PropValue::Unsigned(image.data_association))
        }
        (Field::NineGrid(position), Content::NineGrid(grid)) => match position {
            NineGridProp::LeftOffset => Some(PropValue::Unsigned(grid.left)),
            NineGridProp::TopOffset => Some(PropValue::Unsigned(grid.top)),
            NineGridProp::RightOffset => Some(PropValue::Unsigned(grid.right)),
            NineGridProp::BottomOffset => Some(PropValue::Unsigned(grid.bottom)),
            NineGridProp::NoCenter => Some(PropValue::Bool(grid.no_center)),
            NineGridProp::TextureFileName => None,
        },
        (Field::Figure(position), Content::Figure(figure)) => {
            figure.fetch(position, members, index)
        }
        _ => None,
    }
}

impl Element {
    pub(crate) fn store_extra(
        &mut self,
        path: &[&'static PropDef],
        index: Option<u32>,
        value: PropValue,
    ) {
        if let Some(entry) = self
            .extra
            .iter_mut()
            .find(|entry| entry.matches(path, index))
        {
            entry.value = value;
            return;
        }

        self.extra.push(StoredProperty {
            path: path.into(),
            index,
            value,
        });
    }

    pub(crate) fn extra(
        &self,
        path: &[&'static PropDef],
        index: Option<u32>,
    ) -> Option<&PropValue> {
        self.extra
            .iter()
            .find(|entry| entry.matches(path, index))
            .map(|entry| &entry.value)
    }
}

/// The XuiFigure top-level definition `prop`.
pub(crate) fn figure_prop(prop: FigureProp) -> &'static PropDef {
    &xui::XUI_FIGURE.props[prop as usize]
}

/// Member `member` of the XuiFigure Fill compound.
pub(crate) fn fill_member(member: FillMember) -> &'static PropDef {
    &compound_members(figure_prop(FigureProp::Fill))[member as usize]
}

fn compound_members(def: &'static PropDef) -> &'static [PropDef] {
    def.ty.compound().unwrap_or(&[])
}

pub(crate) fn member_of<M>(compound: &'static PropDef, member: &PropDef) -> Option<M>
where
    M: SchemaPosition,
{
    compound_members(compound)
        .iter()
        .position(|candidate| ptr::eq(candidate, member))
        .and_then(M::from_position)
}

pub(crate) fn stops_value(
    stops: &GradientStops,
    index: Option<u32>,
    color: bool,
) -> Option<PropValue> {
    let stop = stops.get(usize::try_from(index?).ok()?)?;

    Some(if color {
        PropValue::Color(stop.color)
    } else {
        PropValue::Float(stop.position)
    })
}

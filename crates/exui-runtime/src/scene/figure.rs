//! XuiFigure: path flattening, fill triangulation, stroke strips, gradient
//! textures and fill transforms (doc/xui-render.md section 8).

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ptr;

use exui_core::custom::FigurePoint;
use exui_core::schema::PropDef;
use exui_core::value::Color;
use exui_core::value::Vector3;
use xbox360_draw::draw::TextureId;

use crate::math::Matrix4;
use crate::math::unit_to_byte;
use crate::resource::CachedTexture;
use crate::scene::PropertyError;
use crate::scene::Size;
use crate::scene::paint::TextureFlags;
use crate::scene::property::FigureProp;
use crate::scene::property::FillMember;
use crate::scene::property::GradientMember;
use crate::scene::property::PropValue;
use crate::scene::property::StrokeMember;
use crate::scene::property::figure_prop;
use crate::scene::property::fill_member;
use crate::scene::property::member_of;
use crate::scene::property::stops_value;

/// Texels of the gradient texture (`XuiDC_CreateGradientTexture`).
pub const GRADIENT_TEXELS: usize = 128;

/// One gradient stop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    pub color: Color,
    pub position: f32,
}

impl GradientStop {
    /// A stop NumStops added that was not set yet [INFERENCE: new stops are
    /// zero].
    const ZERO: Self = Self {
        color: Color::from_argb(0),
        position: 0.0,
    };
}

/// The Fill.Gradient stop arrays. NumStops sets the count and the stops
/// below it read as zero until they are set. Only stops up to the highest
/// one set are stored, so memory follows the stop values a scene sets, not
/// the NumStops value.
#[derive(Clone, Debug, PartialEq)]
pub struct GradientStops {
    count: u32,
    stored: Vec<GradientStop>,
}

impl GradientStops {
    pub fn new() -> Self {
        Self {
            count: 0,
            stored: Vec::new(),
        }
    }

    /// Stops with NumStops equal to their number.
    pub fn from_stops(stops: Vec<GradientStop>) -> Self {
        Self {
            count: u32::try_from(stops.len()).unwrap_or(u32::MAX),
            stored: stops,
        }
    }

    /// NumStops.
    pub fn len(&self) -> usize {
        self.count as usize
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Stop `index`; `None` at or past NumStops.
    pub fn get(&self, index: usize) -> Option<GradientStop> {
        (index < self.len()).then(|| self.at(index))
    }

    fn at(&self, index: usize) -> GradientStop {
        self.stored
            .get(index)
            .copied()
            .unwrap_or(GradientStop::ZERO)
    }

    fn set_count(&mut self, count: u32) {
        self.count = count;
        self.stored.truncate(self.len());
    }

    fn get_mut(&mut self, index: usize) -> Option<&mut GradientStop> {
        if index >= self.len() {
            return None;
        }

        if index >= self.stored.len() {
            self.stored.resize(index + 1, GradientStop::ZERO);
        }

        self.stored.get_mut(index)
    }
}

impl Default for GradientStops {
    fn default() -> Self {
        Self::new()
    }
}

/// Fill.FillType (doc/xui-render.md 8). The getter returns values without a
/// fill behavior unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillType {
    None,
    Solid,
    /// Gradient; a versioned transform scales by the shorter side.
    GradientMinSide,
    /// Gradient; a versioned transform scales by the longer side.
    GradientMaxSide,
    Texture,
    Unknown(u32),
}

impl FillType {
    pub fn from_raw(raw: u32) -> Self {
        match raw {
            0 => Self::None,
            1 => Self::Solid,
            2 => Self::GradientMinSide,
            3 => Self::GradientMaxSide,
            4 => Self::Texture,
            other => Self::Unknown(other),
        }
    }

    pub fn raw(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Solid => 1,
            Self::GradientMinSide => 2,
            Self::GradientMaxSide => 3,
            Self::Texture => 4,
            Self::Unknown(raw) => raw,
        }
    }

    pub fn is_gradient(self) -> bool {
        matches!(self, Self::GradientMinSide | Self::GradientMaxSide)
    }
}

/// The Fill compound (figure +0x14).
#[derive(Clone, Debug, PartialEq)]
pub struct Fill {
    pub fill_type: FillType,
    pub color: Color,
    pub texture_path: Option<Box<[u16]>>,
    pub radial: bool,
    pub stops: GradientStops,
    pub translation: Vector3,
    pub scale: Vector3,
    /// Degrees.
    pub rotation: f32,
    /// Packed word +0x84, fields in [`PackedField`].
    pub packed: u32,
}

impl Fill {
    /// DC texture flags of the fill.
    pub(crate) fn texture_flags(&self) -> TextureFlags {
        TextureFlags(PackedField::TEXTURE_FLAGS.get(self.packed))
    }

    pub fn transform_version(&self) -> u32 {
        PackedField::TRANSFORM_VERSION.get(self.packed)
    }

    fn set_packed(&mut self, field: PackedField, value: u32) {
        let PackedField { shift, mask } = field;
        self.packed = (self.packed & !(mask << shift)) | ((value & mask) << shift);
    }
}

/// A bit field of the packed fill word: the property value masked with
/// `mask`, stored shifted left by `shift`.
#[derive(Clone, Copy, Debug)]
struct PackedField {
    shift: u32,
    mask: u32,
}

impl PackedField {
    const WRAP_X: Self = Self {
        shift: 2,
        mask: 0x3,
    };
    const WRAP_Y: Self = Self {
        shift: 4,
        mask: 0x3,
    };
    /// Property values 0x10/0x20 land in bits 6-7.
    const BRUSH_FLAGS: Self = Self {
        shift: 2,
        mask: 0x30,
    };
    const TRANSFORM_VERSION: Self = Self {
        shift: 8,
        mask: 0x3,
    };
    /// WrapX, WrapY and BrushFlags together form the DC texture flags.
    const TEXTURE_FLAGS: Self = Self {
        shift: 2,
        mask: 0x3f,
    };

    fn get(self, packed: u32) -> u32 {
        (packed >> self.shift) & self.mask
    }
}

/// Triangulated outline of a figure at one size.
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    /// The size the mesh was built for.
    pub size: Size,
    /// Flattened points including the closing point (equal to the first).
    pub points: Vec<[f32; 2]>,
    /// Flattened points before the closing segment (stroke of open paths).
    pub open_count: usize,
    pub convex: bool,
    /// Fill triangles over `points`: a fan when convex, else ear clipping.
    pub fill: Vec<[u32; 3]>,
}

#[derive(Debug)]
pub(crate) struct Figure {
    pub(crate) stroke_width: f32,
    pub(crate) stroke_color: Color,
    pub(crate) fill: Fill,
    pub(crate) closed: bool,
    pub(crate) points: Vec<FigurePoint>,
    /// Stored by the binary Points form; computed from the outline otherwise.
    pub(crate) design_size: Option<Size>,
    pub(crate) mesh: Option<Mesh>,
    pub(crate) gradient_texture: Option<TextureId>,
    pub(crate) gradient_dirty: bool,
    /// The gradient texture holds straight colors at full alpha.
    pub(crate) gradient_straight: bool,
    pub(crate) fill_texture: Option<CachedTexture>,
}

impl Figure {
    pub(crate) fn new() -> Self {
        Self {
            stroke_width: 0.0,
            stroke_color: Color::from_argb(0),
            fill: Fill {
                fill_type: FillType::None,
                color: Color::from_argb(0),
                texture_path: None,
                radial: false,
                stops: GradientStops::new(),
                translation: Vector3::new(0.0, 0.0, 0.0),
                scale: Vector3::new(0.0, 0.0, 0.0),
                rotation: 0.0,
                packed: 0,
            },
            closed: false,
            points: Vec::new(),
            design_size: None,
            mesh: None,
            gradient_texture: None,
            gradient_dirty: true,
            gradient_straight: false,
            fill_texture: None,
        }
    }

    /// XuiFigure property `prop` with the compound members below it.
    pub(crate) fn store(
        &mut self,
        prop: FigureProp,
        members: &[&'static PropDef],
        index: Option<u32>,
        def: &'static PropDef,
        value: PropValue,
    ) -> Result<(), PropertyError> {
        let mismatch = |value: &PropValue| PropertyError::TypeMismatch {
            name: def.name,
            expected: def.ty.scalar(),
            found: value.scalar_type(),
        };
        let not_on_class = PropertyError::NotOnClass { name: def.name };

        match (prop, members) {
            (FigureProp::Stroke, [member]) => {
                match (member_of(figure_prop(FigureProp::Stroke), member), value) {
                    (Some(StrokeMember::StrokeWidth), PropValue::Float(width)) => {
                        self.stroke_width = width
                    }
                    (Some(StrokeMember::StrokeColor), PropValue::Color(color)) => {
                        self.stroke_color = color
                    }
                    (Some(_), value) => return Err(mismatch(&value)),
                    (None, _) => return Err(not_on_class),
                }
            }
            (FigureProp::Fill, [member]) => {
                let Some(member) = member_of::<FillMember>(figure_prop(FigureProp::Fill), member)
                else {
                    return Err(not_on_class);
                };
                self.store_fill(member, value)
                    .map_err(|value| mismatch(&value))?;
            }
            (FigureProp::Fill, [gradient, member])
                if ptr::eq(*gradient, fill_member(FillMember::Gradient)) =>
            {
                self.store_gradient(member, index, def, value)?
            }
            (FigureProp::Closed, []) => match value {
                PropValue::Bool(closed) => self.closed = closed,
                value => return Err(mismatch(&value)),
            },
            (FigureProp::Points, []) => match value {
                PropValue::Points(points) => {
                    self.design_size = Some(Size {
                        width: points.width,
                        height: points.height,
                    });
                    // An incomplete point list is ignored; the size applies.
                    if let Some(points) = points.points {
                        self.points = points;
                    }
                    self.mesh = None;
                }
                value => return Err(mismatch(&value)),
            },
            _ => return Err(not_on_class),
        }

        Ok(())
    }

    fn store_fill(&mut self, member: FillMember, value: PropValue) -> Result<(), PropValue> {
        let fill = &mut self.fill;
        match (member, value) {
            (FillMember::FillType, PropValue::Unsigned(fill_type)) => {
                fill.fill_type = FillType::from_raw(fill_type)
            }
            (FillMember::FillColor, PropValue::Color(color)) => fill.color = color,
            (FillMember::TextureFileName, PropValue::String(path)) => {
                fill.texture_path = path;
                self.fill_texture = None;
            }
            (FillMember::Translation, PropValue::Vector(translation)) => {
                fill.translation = translation
            }
            (FillMember::Scale, PropValue::Vector(scale)) => fill.scale = scale,
            (FillMember::Rotation, PropValue::Float(rotation)) => fill.rotation = rotation,
            (FillMember::WrapX, PropValue::Unsigned(wrap)) => {
                fill.set_packed(PackedField::WRAP_X, wrap)
            }
            (FillMember::WrapY, PropValue::Unsigned(wrap)) => {
                fill.set_packed(PackedField::WRAP_Y, wrap)
            }
            (FillMember::BrushFlags, PropValue::Unsigned(flags)) => {
                fill.set_packed(PackedField::BRUSH_FLAGS, flags)
            }
            (FillMember::TransformVersion, PropValue::Unsigned(version)) => {
                fill.set_packed(PackedField::TRANSFORM_VERSION, version)
            }
            (_, value) => return Err(value),
        }

        Ok(())
    }

    fn store_gradient(
        &mut self,
        member: &PropDef,
        index: Option<u32>,
        def: &'static PropDef,
        value: PropValue,
    ) -> Result<(), PropertyError> {
        let gradient_def = fill_member(FillMember::Gradient);
        let stops = &mut self.fill.stops;
        let mismatch = |value: &PropValue| PropertyError::TypeMismatch {
            name: def.name,
            expected: def.ty.scalar(),
            found: value.scalar_type(),
        };
        let stop_index = |count: usize| -> Result<usize, PropertyError> {
            let index = index.ok_or(PropertyError::IndexRequired { name: def.name })?;
            usize::try_from(index)
                .ok()
                .filter(|&slot| slot < count)
                .ok_or(PropertyError::IndexOutOfRange {
                    name: def.name,
                    index,
                    count,
                })
        };

        match (member_of(gradient_def, member), value) {
            (Some(GradientMember::Radial), PropValue::Bool(radial)) => self.fill.radial = radial,
            (Some(GradientMember::NumStops), PropValue::Integer(count)) => {
                let count = u32::try_from(count).map_err(|_| PropertyError::IndexOutOfRange {
                    name: def.name,
                    index: count as u32,
                    count: 0,
                })?;
                stops.set_count(count);
            }
            (Some(GradientMember::StopColor), PropValue::Color(color)) => {
                let slot = stop_index(stops.len())?;
                if let Some(stop) = stops.get_mut(slot) {
                    stop.color = color;
                }
            }
            (Some(GradientMember::StopPos), PropValue::Float(position)) => {
                let slot = stop_index(stops.len())?;
                if let Some(stop) = stops.get_mut(slot) {
                    stop.position = position;
                }
            }
            (Some(_), value) => return Err(mismatch(&value)),
            (None, _) => return Err(PropertyError::NotOnClass { name: def.name }),
        }
        self.gradient_dirty = true;

        Ok(())
    }

    pub(crate) fn fetch(
        &self,
        prop: FigureProp,
        members: &[&'static PropDef],
        index: Option<u32>,
    ) -> Option<PropValue> {
        match (prop, members) {
            (FigureProp::Stroke, [member]) => {
                match member_of(figure_prop(FigureProp::Stroke), member)? {
                    StrokeMember::StrokeWidth => Some(PropValue::Float(self.stroke_width)),
                    StrokeMember::StrokeColor => Some(PropValue::Color(self.stroke_color)),
                }
            }
            (FigureProp::Fill, [member]) => {
                let fill = &self.fill;
                match member_of(figure_prop(FigureProp::Fill), member)? {
                    FillMember::FillType => Some(PropValue::Unsigned(fill.fill_type.raw())),
                    FillMember::FillColor => Some(PropValue::Color(fill.color)),
                    FillMember::Translation => Some(PropValue::Vector(fill.translation)),
                    FillMember::Scale => Some(PropValue::Vector(fill.scale)),
                    FillMember::Rotation => Some(PropValue::Float(fill.rotation)),
                    FillMember::WrapX => {
                        Some(PropValue::Unsigned(PackedField::WRAP_X.get(fill.packed)))
                    }
                    FillMember::WrapY => {
                        Some(PropValue::Unsigned(PackedField::WRAP_Y.get(fill.packed)))
                    }
                    FillMember::BrushFlags => Some(PropValue::Unsigned(
                        PackedField::BRUSH_FLAGS.get(fill.packed),
                    )),
                    FillMember::TransformVersion => {
                        Some(PropValue::Unsigned(fill.transform_version()))
                    }
                    FillMember::TextureFileName | FillMember::Gradient => None,
                }
            }
            (FigureProp::Fill, [_, member]) => {
                match member_of(fill_member(FillMember::Gradient), member)? {
                    GradientMember::Radial => Some(PropValue::Bool(self.fill.radial)),
                    GradientMember::NumStops => {
                        Some(PropValue::Integer(self.fill.stops.len() as i32))
                    }
                    GradientMember::StopColor => stops_value(&self.fill.stops, index, true),
                    GradientMember::StopPos => stops_value(&self.fill.stops, index, false),
                }
            }
            (FigureProp::Closed, []) => Some(PropValue::Bool(self.closed)),
            _ => None,
        }
    }

    /// The mesh for `size`, rebuilt when the size or the points changed
    /// (message 0x10 marks it dirty in the original).
    pub(crate) fn mesh(&mut self, size: Size) -> &Mesh {
        if self.mesh.as_ref().is_some_and(|mesh| mesh.size != size) {
            self.mesh = None;
        }
        let points = &self.points;
        let design_size = self.design_size;

        self.mesh
            .get_or_insert_with(|| build_mesh(points, design_size, size))
    }
}

/// Not 1:1: a non-convex outline of more than `MAX_EAR_CLIP_POINTS`
/// flattened points is not filled. Ear clipping is quadratic in the point
/// count and runs again on every size change, so the original's limit of
/// 65535 points (u16 indices) lets one figure stall each frame of a size
/// animation.
fn build_mesh(source: &[FigurePoint], design_size: Option<Size>, size: Size) -> Mesh {
    const MAX_EAR_CLIP_POINTS: usize = 1024;

    let design = design_size.unwrap_or_else(|| design_bounds(source));
    let scale_x = if design.width == 0.0 {
        1.0
    } else {
        size.width / design.width
    };
    let scale_y = if design.height == 0.0 {
        1.0
    } else {
        size.height / design.height
    };

    let mut points = Vec::new();
    let flattened = flatten(source, scale_x, scale_y, &mut points);
    let mut fill = Vec::new();
    if flattened.convex {
        fan(points.len(), &mut fill);
    } else if points.len() <= MAX_EAR_CLIP_POINTS {
        ear_clip(&points, &mut fill);
    }

    Mesh {
        size,
        points,
        open_count: flattened.open_count,
        convex: flattened.convex,
        fill,
    }
}

/// Result of [`flatten`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Flattened {
    pub convex: bool,
    /// Points emitted before the closing segment.
    pub open_count: usize,
}

/// `XuiBezier_Flatten` over all segments, each point and control point
/// scaled by `(scale_x, scale_y)`. Segment i runs from point i through its
/// two control points to point i + 1; the last one closes to point 0. The
/// output starts with point 0 and ends with the closing point. Convexity is
/// tracked as the point sink does.
pub fn flatten(
    points: &[FigurePoint],
    scale_x: f32,
    scale_y: f32,
    out: &mut Vec<[f32; 2]>,
) -> Flattened {
    out.clear();
    let Some(first) = points.first() else {
        return Flattened {
            convex: true,
            open_count: 0,
        };
    };
    let scaled = |point: &FigurePoint, offset: usize| {
        [
            point.coords[offset] * scale_x,
            point.coords[offset + 1] * scale_y,
        ]
    };

    let mut sink = ConvexitySink::new();
    sink.push(out, scaled(first, 0));
    let mut open_count = 1;
    for (index, point) in points.iter().enumerate() {
        if index + 1 == points.len() {
            open_count = out.len();
        }
        let next = &points[(index + 1) % points.len()];
        let curve = [
            scaled(point, 0),
            scaled(point, 2),
            scaled(point, 4),
            scaled(next, 0),
        ];
        subdivide(curve, out, &mut sink);
    }

    Flattened {
        convex: sink.convex,
        open_count,
    }
}

/// Bounding box size of the unscaled flattened outline
/// (`XuiFigure_ComputeDesignBounds`); the box origin is not subtracted.
pub fn design_bounds(points: &[FigurePoint]) -> Size {
    let mut flattened = Vec::new();
    flatten(points, 1.0, 1.0, &mut flattened);
    let Some(first) = flattened.first() else {
        return Size {
            width: 0.0,
            height: 0.0,
        };
    };

    let (mut min, mut max) = (*first, *first);
    for point in &flattened {
        min = [min[0].min(point[0]), min[1].min(point[1])];
        max = [max[0].max(point[0]), max[1].max(point[1])];
    }

    Size {
        width: max[0] - min[0],
        height: max[1] - min[1],
    }
}

struct ConvexitySink {
    previous_edge: Option<[f32; 2]>,
    sign: Option<bool>,
    convex: bool,
}

impl ConvexitySink {
    fn new() -> Self {
        Self {
            previous_edge: None,
            sign: None,
            convex: true,
        }
    }

    fn push(&mut self, out: &mut Vec<[f32; 2]>, point: [f32; 2]) {
        if let Some(last) = out.last() {
            let edge = normalize([point[0] - last[0], point[1] - last[1]]);
            if let Some(previous) = self.previous_edge {
                let cross = previous[0] * edge[1] - previous[1] * edge[0];
                if cross.abs() >= 1e-4 {
                    let positive = cross > 0.0;
                    if self.sign.is_some_and(|sign| sign != positive) {
                        self.convex = false;
                    }
                    self.sign = Some(positive);
                }
            }
            self.previous_edge = Some(edge);
        }
        out.push(point);
    }
}

/// Recursive midpoint subdivision as an explicit stack; first halves first.
fn subdivide(curve: [[f32; 2]; 4], out: &mut Vec<[f32; 2]>, sink: &mut ConvexitySink) {
    // Bounds for malformed point data (non-finite or huge coordinates are
    // never flat): the original indexes the mesh with u16, so a valid outline
    // stays below 65536 points; past that, segments are emitted unsplit.
    const MAX_DEPTH: u8 = 32;
    const MAX_POINTS: usize = u16::MAX as usize;

    let mut stack = alloc::vec![(curve, 0u8)];
    while let Some((curve, depth)) = stack.pop() {
        if depth >= MAX_DEPTH || out.len() >= MAX_POINTS || is_flat(&curve) {
            sink.push(out, curve[3]);
            continue;
        }

        let [p0, p1, p2, p3] = curve;
        let mid = |a: [f32; 2], b: [f32; 2]| [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
        let p01 = mid(p0, p1);
        let p12 = mid(p1, p2);
        let p23 = mid(p2, p3);
        let p012 = mid(p01, p12);
        let p123 = mid(p12, p23);
        let center = mid(p012, p123);
        stack.push(([center, p123, p23, p3], depth + 1));
        stack.push(([p0, p01, p012, center], depth + 1));
    }
}

/// `XuiBezier_IsFlat`: short chord, or both control points within
/// squared distance 0.5 of the chord line.
fn is_flat(curve: &[[f32; 2]; 4]) -> bool {
    let [p0, c1, c2, p3] = *curve;
    let chord = [p3[0] - p0[0], p3[1] - p0[1]];
    let length_squared = chord[0] * chord[0] + chord[1] * chord[1];
    if length_squared < 0.5 {
        return true;
    }

    let distance_squared = |point: [f32; 2]| {
        let cross = chord[0] * (point[1] - p0[1]) - chord[1] * (point[0] - p0[0]);
        cross * cross / length_squared
    };

    distance_squared(c1) < 0.5 && distance_squared(c2) < 0.5
}

/// Unit vector; a zero vector stays zero [INFERENCE].
fn normalize(vector: [f32; 2]) -> [f32; 2] {
    let length = libm::sqrtf(vector[0] * vector[0] + vector[1] * vector[1]);
    if length == 0.0 {
        return [0.0, 0.0];
    }

    [vector[0] / length, vector[1] / length]
}

/// Triangle fan over the first `count - 1` points (the closing point is
/// left out): `count - 3` triangles.
pub fn fan(count: usize, out: &mut Vec<[u32; 3]>) {
    out.clear();
    for index in 1..count.saturating_sub(2) {
        out.push([0, index as u32, index as u32 + 1]);
    }
}

/// `Orient(a, b, c)` classes with the 0.5 dead zone.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Turn {
    Left,
    Right,
    Collinear,
}

fn orient(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> Turn {
    let area = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
    if area > 0.5 {
        Turn::Left
    } else if area < -0.5 {
        Turn::Right
    } else {
        Turn::Collinear
    }
}

struct EarClipper<'a> {
    points: &'a [[f32; 2]],
    next: Vec<u32>,
    prev: Vec<u32>,
    ear: Vec<bool>,
    /// Negates Left and LeftOn for clockwise outlines.
    flip: bool,
}

impl EarClipper<'_> {
    fn point(&self, node: u32) -> [f32; 2] {
        self.points[node as usize]
    }

    fn left(&self, a: u32, b: u32, c: u32) -> bool {
        (orient(self.point(a), self.point(b), self.point(c)) == Turn::Left) != self.flip
    }

    fn left_on(&self, a: u32, b: u32, c: u32) -> bool {
        (orient(self.point(a), self.point(b), self.point(c)) != Turn::Right) != self.flip
    }

    fn collinear(&self, a: u32, b: u32, c: u32) -> bool {
        orient(self.point(a), self.point(b), self.point(c)) == Turn::Collinear
    }

    fn between(&self, a: u32, b: u32, c: u32) -> bool {
        if !self.collinear(a, b, c) {
            return false;
        }

        let (a, b, c) = (self.point(a), self.point(b), self.point(c));
        let axis = if a[0] != b[0] { 0 } else { 1 };

        (a[axis] <= c[axis] && c[axis] <= b[axis]) || (b[axis] <= c[axis] && c[axis] <= a[axis])
    }

    fn intersect_proper(&self, a: u32, b: u32, c: u32, d: u32) -> bool {
        if self.collinear(a, b, c)
            || self.collinear(a, b, d)
            || self.collinear(c, d, a)
            || self.collinear(c, d, b)
        {
            return false;
        }

        (self.left(a, b, c) != self.left(a, b, d)) && (self.left(c, d, a) != self.left(c, d, b))
    }

    fn intersect(&self, a: u32, b: u32, c: u32, d: u32) -> bool {
        self.intersect_proper(a, b, c, d)
            || self.between(a, b, c)
            || self.between(a, b, d)
            || self.between(c, d, a)
            || self.between(c, d, b)
    }

    /// No ring edge that avoids `a` and `b` crosses segment ab.
    fn diagonalie(&self, a: u32, b: u32, head: u32) -> bool {
        let mut c = head;
        loop {
            let d = self.next[c as usize];
            if c != a && d != a && c != b && d != b && self.intersect(a, b, c, d) {
                return false;
            }
            c = d;
            if c == head {
                return true;
            }
        }
    }

    fn in_cone(&self, a: u32, b: u32) -> bool {
        let a0 = self.prev[a as usize];
        let a1 = self.next[a as usize];
        if self.left_on(a, a1, a0) {
            return self.left(a, b, a0) && self.left(b, a, a1);
        }

        !(self.left_on(a, b, a1) && self.left_on(b, a, a0))
    }

    fn diagonal(&self, a: u32, b: u32, head: u32) -> bool {
        self.in_cone(a, b) && self.in_cone(b, a) && self.diagonalie(a, b, head)
    }
}

/// `XuiPolygon_EarClip` over `points` (which end with the closing point):
/// `points.len() - 3` triangles of point indices.
pub fn ear_clip(points: &[[f32; 2]], out: &mut Vec<[u32; 3]>) {
    out.clear();
    let count = points.len();
    if count < 4 {
        return;
    }

    let mut clipper = EarClipper {
        points,
        next: (0..count as u32)
            .map(|node| (node + 1) % count as u32)
            .collect(),
        prev: (0..count as u32)
            .map(|node| (node + count as u32 - 1) % count as u32)
            .collect(),
        ear: alloc::vec![false; count],
        flip: false,
    };
    let mut head = 0u32;

    // Orientation from the first node with the strictly smallest y, while
    // the closing duplicate is still in the ring.
    let mut lowest = 0u32;
    for node in 1..count as u32 {
        if clipper.point(node)[1] < clipper.point(lowest)[1] {
            lowest = node;
        }
    }
    let after = clipper.next[lowest as usize];
    let before = if lowest == head {
        clipper.prev[clipper.prev[head as usize] as usize]
    } else {
        clipper.prev[lowest as usize]
    };
    clipper.flip = orient(
        clipper.point(lowest),
        clipper.point(after),
        clipper.point(before),
    ) != Turn::Left;

    // Drop the closing duplicate.
    let mut remaining = count;
    let last = clipper.prev[head as usize];
    if clipper.point(last) == clipper.point(head) {
        let before_last = clipper.prev[last as usize];
        clipper.next[before_last as usize] = head;
        clipper.prev[head as usize] = before_last;
        remaining -= 1;
    }

    let mut node = head;
    loop {
        clipper.ear[node as usize] = clipper.diagonal(
            clipper.prev[node as usize],
            clipper.next[node as usize],
            head,
        );
        node = clipper.next[node as usize];
        if node == head {
            break;
        }
    }

    let mut force = false;
    while remaining > 3 {
        let mut node = head;
        let mut found = false;
        loop {
            if clipper.ear[node as usize] || force {
                let (prev, next) = (clipper.prev[node as usize], clipper.next[node as usize]);
                out.push([node, next, prev]);

                let prev_prev = clipper.prev[prev as usize];
                let next_next = clipper.next[next as usize];
                clipper.ear[prev as usize] = clipper.diagonal(prev_prev, next, head);
                clipper.ear[next as usize] = clipper.diagonal(prev, next_next, head);

                clipper.next[prev as usize] = next;
                clipper.prev[next as usize] = prev;
                head = next;
                remaining -= 1;
                force = false;
                found = true;
                break;
            }
            node = clipper.next[node as usize];
            if node == head {
                break;
            }
        }
        if !found {
            force = true;
        }
    }

    out.push([
        head,
        clipper.next[head as usize],
        clipper.prev[head as usize],
    ]);
}

/// One-sided stroke as a triangle strip (`XuiShape_BuildStroke`). `points`
/// ends with the closing point; an open path stops after `open_count`
/// points.
pub fn stroke_strip(
    points: &[[f32; 2]],
    open_count: usize,
    width: f32,
    closed: bool,
    out: &mut Vec<[f32; 2]>,
) {
    out.clear();
    let count = points.len();
    if count < 2 {
        return;
    }

    let unique = count - 1;
    let offset_of = |index: usize| {
        let p = points[index];
        let q = points[(index + 1) % unique];
        let direction = normalize([q[0] - p[0], q[1] - p[1]]);
        ([direction[1] * width, -direction[0] * width], p, q)
    };
    let add = |a: [f32; 2], b: [f32; 2]| [a[0] + b[0], a[1] + b[1]];

    for segment in 0..count {
        let (offset, p, q) = offset_of(segment % unique);
        let p = if segment == unique { points[unique] } else { p };
        if segment == 0 {
            out.extend([p, add(p, offset), q, add(q, offset)]);
        } else if segment + 1 < count {
            out.extend([add(p, offset), add(p, offset), p, add(q, offset), q]);
        } else {
            out.push(add(p, offset));
        }
    }

    // Open: `5 * m - 8` primitives, the strip before the closing segment.
    if !closed {
        out.truncate((5 * open_count).saturating_sub(6));
    }
}

/// Triangles of a strip of `count` vertices.
pub fn strip_triangles(count: usize, out: &mut Vec<[u32; 3]>) {
    out.clear();
    for index in 0..count.saturating_sub(2) as u32 {
        out.push([index, index + 1, index + 2]);
    }
}

/// `XuiDC_FillGradientTexels`: 128 ARGB texels.
pub fn gradient_texels(stops: &GradientStops) -> [u32; GRADIENT_TEXELS] {
    let mut texels = [0; GRADIENT_TEXELS];
    let Some(last) = stops.len().checked_sub(1) else {
        return texels;
    };

    let mut low = 0;
    let mut high = 1.min(last);
    for (index, texel) in texels.iter_mut().enumerate() {
        let t = index as f32 / GRADIENT_TEXELS as f32;
        if t > stops.at(high).position {
            low = high;
            if high < last {
                high += 1;
            }
        }

        let (from, to) = (stops.at(low), stops.at(high));
        let weight = if high == low {
            1.0
        } else {
            let span = to.position - from.position;
            if span < 1e-4 {
                1.0
            } else if t < from.position {
                0.0
            } else {
                (t - from.position) / span
            }
        };

        let (from, to) = (from.color.argb(), to.color.argb());
        let mut argb = 0;
        for shift in [24, 16, 8, 0] {
            let a = ((from >> shift) & 0xff) as f32 / 255.0;
            let b = ((to >> shift) & 0xff) as f32 / 255.0;
            argb |= u32::from(unit_to_byte(a + (b - a) * weight)) << shift;
        }
        *texel = argb;
    }

    texels
}

/// `XuiFill_ComputeTexTransform`: element-local position to texture
/// coordinates. `texture_size` is the fill texture size for type 4.
pub fn fill_transform(fill: &Fill, size: Size, texture_size: Option<Size>) -> Matrix4 {
    let Size { width, height } = size;
    let texture = texture_size.unwrap_or(Size {
        width: 1.0,
        height: 1.0,
    });
    let rotation = Matrix4::rotation_z_degrees(fill.rotation);
    let scale = Matrix4::scaling(fill.scale.x, fill.scale.y, fill.scale.z);
    let translation = fill.translation;

    if fill.transform_version() == 0 {
        let matrix = if fill.fill_type == FillType::Texture {
            Matrix4::translation(-width / 2.0, -height / 2.0, 0.0)
                .mul(&scale)
                .mul(&rotation)
                .mul(&Matrix4::translation(width / 2.0, height / 2.0, 0.0))
                .mul(&Matrix4::scaling(
                    1.0 / texture.width,
                    1.0 / texture.height,
                    1.0,
                ))
        } else {
            Matrix4::scaling(1.0 / width, 1.0 / height, 1.0)
                .mul(&Matrix4::translation(-0.5, -0.5, 0.0))
                .mul(&scale)
                .mul(&rotation)
                .mul(&Matrix4::translation(0.5, 0.5, 0.0))
        };
        return matrix.mul(&Matrix4::translation(
            translation.x,
            translation.y,
            translation.z,
        ));
    }

    let reference = match fill.fill_type {
        FillType::Texture => texture,
        FillType::GradientMinSide => Size {
            width: width.min(height),
            height: width.min(height),
        },
        FillType::GradientMaxSide => Size {
            width: width.max(height),
            height: width.max(height),
        },
        _ => size,
    };
    let mut matrix = rotation.mul(&Matrix4::scaling(
        fill.scale.x / reference.width,
        fill.scale.y / reference.height,
        0.0,
    ));
    let a = matrix.transform_coord(width, 0.0, 0.0);
    let b = matrix.transform_coord(width, height, 0.0);
    let c = matrix.transform_coord(0.0, height, 0.0);
    matrix.0[3][0] = 0.5 - 0.5 * b[0] + translation.x * a[0] + translation.y * c[0];
    matrix.0[3][1] = 0.5 - 0.5 * b[1] + translation.x * a[1] + translation.y * c[1];

    matrix
}

//! Element arena: storage, links between elements and tree operations.

use alloc::boxed::Box;
use alloc::collections::BTreeSet;
use alloc::vec::Vec;
use core::ptr;

use exui_core::schema::ClassDef;
use exui_core::schema::ClassId;
use exui_core::schema::ClassRef;
use exui_core::schema::PropDef;
use exui_core::schema::xui;
use exui_core::value::Quaternion;
use exui_core::value::Vector3;
use xbox360_draw::draw::TextureId;

use crate::math::Matrix4;
use crate::math::local_transform;
use crate::scene::PropertyError;
use crate::scene::TreeError;
use crate::scene::content::Content;
use crate::scene::content::ControlProps;
use crate::scene::effect::EffectTarget;
use crate::scene::image::CustomImage;
use crate::scene::property::Effect;
use crate::scene::property::PropValue;
use crate::scene::property::Stored;
use crate::scene::property::StoredProperty;
use crate::scene::property::fetch_string;
use crate::scene::property::fetch_value;
use crate::scene::property::store_field;

/// Handle of an element in an [`ElementTree`]. A destroyed element's id
/// never resolves again.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ElementId {
    slot: u32,
    generation: u32,
}

/// Handle of a base path registered with [`ElementTree::add_base_path`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BasePathId(u32);

/// The element flag word (element +0xf0).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ElementFlags(pub u32);

impl ElementFlags {
    pub const SHOW: u32 = 0x1;
    pub const TRANSFORM_DIRTY: u32 = 0x2;
    pub const INPUT_BLOCKED: u32 = 0x4;
    pub const TIMELINE_RECURSION_API: u32 = 0x8;
    pub const LAYOUT_IN_PROGRESS: u32 = 0x10;
    pub const KEEP_DESIGN_LAYOUT: u32 = 0x20;
    pub const DESIGN_TIME: u32 = 0x40;
    pub const DISABLE_TIMELINE_RECURSION: u32 = 0x80;
    pub const COLOR_WRITE_MASK: u32 = 0xf00;
    pub const CLIP_CHILDREN: u32 = 0x1000;
    pub const AUTOPLAY_DISABLED: u32 = 0x2000;
    pub const ENABLE_EFFECTS: u32 = 0x4000;
    pub const DISABLE_FOCUS_RECURSION: u32 = 0x10000;

    pub const fn contains(self, bits: u32) -> bool {
        self.0 & bits == bits
    }

    pub fn set(&mut self, bits: u32, on: bool) {
        if on {
            self.0 |= bits;
        } else {
            self.0 &= !bits;
        }
    }

    const COLOR_WRITE_SHIFT: u32 = 8;

    /// ColorWriteFlags: 0 inherits; bit 0 red, 1 green, 2 blue, 3 alpha.
    pub const fn color_write(self) -> u32 {
        (self.0 & Self::COLOR_WRITE_MASK) >> Self::COLOR_WRITE_SHIFT
    }

    pub fn set_color_write(&mut self, value: u32) {
        let bits = (value << Self::COLOR_WRITE_SHIFT) & Self::COLOR_WRITE_MASK;
        self.0 = (self.0 & !Self::COLOR_WRITE_MASK) | bits;
    }
}

/// Anchor bits (`XuiElement_ApplyAnchor`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Anchor(pub u32);

impl Anchor {
    pub const LEFT: u32 = 0x01;
    pub const TOP: u32 = 0x02;
    pub const RIGHT: u32 = 0x04;
    pub const BOTTOM: u32 = 0x08;
    pub const CENTER_X: u32 = 0x10;
    pub const CENTER_Y: u32 = 0x20;
    pub const SCALE_X: u32 = 0x40;
    pub const SCALE_Y: u32 = 0x80;

    pub const fn contains(self, bits: u32) -> bool {
        self.0 & bits == bits
    }

    pub fn clear(&mut self, bits: u32) {
        self.0 &= !bits;
    }
}

/// BlendMode property value (doc/xui-render.md 6). Values above 6 leave the
/// device state unchanged but are still inherited and compared.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum XuiBlendMode {
    #[default]
    Inherit,
    Normal,
    Multiply,
    Darken,
    Lighten,
    Additive,
    Subtract,
    Unknown(u32),
}

impl XuiBlendMode {
    pub fn from_raw(raw: u32) -> Self {
        match raw {
            0 => Self::Inherit,
            1 => Self::Normal,
            2 => Self::Multiply,
            3 => Self::Darken,
            4 => Self::Lighten,
            5 => Self::Additive,
            6 => Self::Subtract,
            other => Self::Unknown(other),
        }
    }

    pub fn raw(self) -> u32 {
        match self {
            Self::Inherit => 0,
            Self::Normal => 1,
            Self::Multiply => 2,
            Self::Darken => 3,
            Self::Lighten => 4,
            Self::Additive => 5,
            Self::Subtract => 6,
            Self::Unknown(raw) => raw,
        }
    }
}

/// Element size in design units.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

/// A size change of one element, in occurrence order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SizeChange {
    pub element: ElementId,
    pub old: Size,
    pub new: Size,
}

/// What the class chain of an element makes it draw and do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassTraits {
    pub content: ContentKind,
    /// Has a XuiControl level.
    pub is_control: bool,
    /// XuiGroup, XuiScene, XuiVisual or XuiList: lays out its children when
    /// its own size changes.
    pub lays_out_children: bool,
}

impl ClassTraits {
    pub fn of(class: ClassRef<'_>) -> Self {
        let mut traits = Self {
            content: ContentKind::None,
            is_control: false,
            lays_out_children: false,
        };
        for level in class.levels() {
            let def = level.def();
            let is = |candidate: &ClassDef| ptr::eq(def, candidate);
            if is(&xui::XUI_CONTROL) {
                traits.is_control = true;
            }
            if is(&xui::XUI_GROUP)
                || is(&xui::XUI_SCENE)
                || is(&xui::XUI_VISUAL)
                || is(&xui::XUI_LIST)
            {
                traits.lays_out_children = true;
            }
            let content: [(&ClassDef, ContentKind); 6] = [
                (&xui::XUI_FIGURE, ContentKind::Figure),
                (&xui::XUI_IMAGE, ContentKind::Image),
                (&xui::XUI_IMAGE_PRESENTER, ContentKind::ImagePresenter),
                (&xui::XUI_NINE_GRID, ContentKind::NineGrid),
                (&xui::XUI_TEXT, ContentKind::Text),
                (&xui::XUI_TEXT_PRESENTER, ContentKind::TextPresenter),
            ];
            if let Some((_, kind)) = content.into_iter().find(|(candidate, _)| is(candidate)) {
                traits.content = kind;
            }
        }

        traits
    }
}

/// The drawing class of an element.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContentKind {
    /// Draws only its children.
    None,
    Figure,
    Image,
    ImagePresenter,
    NineGrid,
    Text,
    TextPresenter,
}

/// Snapshot taken by `XuiElement_CaptureDesignLayout`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DesignLayout {
    pub(crate) size: Size,
    pub(crate) parent_size: Size,
    pub(crate) position: Vector3,
    pub(crate) scale_x: f32,
    pub(crate) scale_y: f32,
}

impl DesignLayout {
    const ZERO: Self = Self {
        size: Size {
            width: 0.0,
            height: 0.0,
        },
        parent_size: Size {
            width: 0.0,
            height: 0.0,
        },
        position: Vector3::new(0.0, 0.0, 0.0),
        scale_x: 0.0,
        scale_y: 0.0,
    };
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Links {
    pub(crate) parent: Option<ElementId>,
    pub(crate) first_child: Option<ElementId>,
    pub(crate) last_child: Option<ElementId>,
    pub(crate) next_sibling: Option<ElementId>,
    pub(crate) prev_sibling: Option<ElementId>,
}

/// One XUI element.
#[derive(Debug)]
pub struct Element {
    pub(crate) class: ClassId,
    pub(crate) traits: ClassTraits,
    pub(crate) id: Option<Box<[u16]>>,
    pub(crate) base_path: Option<BasePathId>,
    pub(crate) links: Links,

    pub(crate) size: Size,
    pub(crate) opacity: f32,
    pub(crate) position: Vector3,
    pub(crate) scale: Vector3,
    pub(crate) rotation: Quaternion,
    pub(crate) pivot: Vector3,
    pub(crate) anchor: Anchor,
    pub(crate) blend_mode: XuiBlendMode,
    pub(crate) flags: ElementFlags,
    pub(crate) design: DesignLayout,
    pub(crate) local: Matrix4,

    pub(crate) control: Option<Box<ControlProps>>,
    pub(crate) content: Content,
    /// Properties without a dedicated field, as last set.
    pub(crate) extra: Vec<StoredProperty>,
    pub(crate) custom_image: Option<Box<CustomImage>>,
    /// Render target of EnableEffects (element +0xf4).
    pub(crate) effect_target: Option<EffectTarget>,
}

impl Element {
    /// A new element with the defaults of its property definitions. The
    /// constructor sets flags to Show | dirty.
    fn new(class: ClassRef<'_>) -> Self {
        let traits = ClassTraits::of(class);
        let mut element = Self {
            class: class.id(),
            traits,
            id: None,
            base_path: None,
            links: Links::default(),
            size: Size::default(),
            opacity: 1.0,
            position: Vector3::new(0.0, 0.0, 0.0),
            scale: Vector3::new(1.0, 1.0, 1.0),
            rotation: Quaternion::new(0.0, 0.0, 0.0, 1.0),
            pivot: Vector3::new(0.0, 0.0, 0.0),
            anchor: Anchor(0),
            blend_mode: XuiBlendMode::Inherit,
            flags: ElementFlags(ElementFlags::SHOW | ElementFlags::TRANSFORM_DIRTY),
            design: DesignLayout::ZERO,
            local: Matrix4::IDENTITY,
            control: traits.is_control.then(|| Box::new(ControlProps::new())),
            content: Content::new(traits.content),
            extra: Vec::new(),
            custom_image: None,
            effect_target: None,
        };

        let mut path: Vec<&'static PropDef> = Vec::new();
        for level in class.levels() {
            for def in level.def().props {
                path.push(def);
                element.apply_defaults(&mut path);
                path.pop();
            }
        }

        element
    }

    /// Applies the schema default of `path` and of every compound member
    /// below it. Unhandled properties keep no entry; reads fall back to
    /// the definition default.
    fn apply_defaults(&mut self, path: &mut Vec<&'static PropDef>) {
        let def = path[path.len() - 1];
        if let Some(members) = def.ty.compound() {
            for member in members {
                path.push(member);
                self.apply_defaults(path);
                path.pop();
            }
            return;
        }

        let Some(default) = def.default else {
            return;
        };
        let value = PropValue::from_default(default);
        // Defaults bypass the setters: no layout runs during construction.
        if let Ok(Stored::Handled(effect)) = store_field(self, path, None, value) {
            match effect {
                Effect::Width(width) => self.size.width = width,
                Effect::Height(height) => self.size.height = height,
                Effect::Position(position) => self.position = position,
                Effect::Scale(scale) => self.scale = scale,
                Effect::None => {}
            }
        }
    }

    pub fn class(&self) -> ClassId {
        self.class
    }

    pub fn traits(&self) -> ClassTraits {
        self.traits
    }

    /// The Id property; `None` for the null string.
    pub fn id(&self) -> Option<&[u16]> {
        self.id.as_deref()
    }

    pub fn base_path(&self) -> Option<BasePathId> {
        self.base_path
    }

    pub fn size(&self) -> Size {
        self.size
    }

    pub fn opacity(&self) -> f32 {
        self.opacity
    }

    pub fn position(&self) -> Vector3 {
        self.position
    }

    pub fn scale(&self) -> Vector3 {
        self.scale
    }

    pub fn rotation(&self) -> Quaternion {
        self.rotation
    }

    pub fn pivot(&self) -> Vector3 {
        self.pivot
    }

    pub fn anchor(&self) -> Anchor {
        self.anchor
    }

    pub fn blend_mode(&self) -> XuiBlendMode {
        self.blend_mode
    }

    pub fn flags(&self) -> ElementFlags {
        self.flags
    }

    pub fn is_shown(&self) -> bool {
        self.flags.contains(ElementFlags::SHOW)
    }

    /// The control's Visual property (skin visual name), if it is a control.
    pub fn visual(&self) -> Option<&[u16]> {
        self.control.as_ref()?.visual.as_deref()
    }

    /// The local matrix, recomputed when the transform is dirty.
    pub fn local_transform(&mut self) -> Matrix4 {
        if self.flags.contains(ElementFlags::TRANSFORM_DIRTY) {
            self.local = local_transform(self.position, self.scale, self.rotation, self.pivot);
            self.flags.set(ElementFlags::TRANSFORM_DIRTY, false);
        }

        self.local
    }
}

#[derive(Debug)]
struct Slot {
    generation: u32,
    element: Option<Element>,
}

/// Arena of elements. Children keep document (insertion) order, which is
/// the draw order.
#[derive(Debug, Default)]
pub struct ElementTree {
    slots: Vec<Slot>,
    free: Vec<u32>,
    base_paths: Vec<Box<[u16]>>,
    pub(crate) size_changes: Vec<SizeChange>,
    pub(crate) released_textures: Vec<TextureId>,
}

impl ElementTree {
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            base_paths: Vec::new(),
            size_changes: Vec::new(),
            released_textures: Vec::new(),
        }
    }

    /// Creates a detached element of `class` with its default properties.
    pub fn create(&mut self, class: ClassRef<'_>) -> ElementId {
        let element = Element::new(class);
        if let Some(slot) = self.free.pop() {
            let entry = &mut self.slots[slot as usize];
            entry.element = Some(element);
            return ElementId {
                slot,
                generation: entry.generation,
            };
        }

        let slot = self.slots.len() as u32;
        self.slots.push(Slot {
            generation: 0,
            element: Some(element),
        });

        ElementId {
            slot,
            generation: 0,
        }
    }

    pub fn contains(&self, id: ElementId) -> bool {
        self.get(id).is_some()
    }

    pub fn get(&self, id: ElementId) -> Option<&Element> {
        let slot = self.slots.get(id.slot as usize)?;
        if slot.generation != id.generation {
            return None;
        }

        slot.element.as_ref()
    }

    pub fn get_mut(&mut self, id: ElementId) -> Option<&mut Element> {
        let slot = self.slots.get_mut(id.slot as usize)?;
        if slot.generation != id.generation {
            return None;
        }

        slot.element.as_mut()
    }

    pub(crate) fn element(&self, id: ElementId) -> Result<&Element, TreeError> {
        self.get(id)
            .ok_or(TreeError::UnknownElement { element: id })
    }

    pub(crate) fn element_mut(&mut self, id: ElementId) -> Result<&mut Element, TreeError> {
        self.get_mut(id)
            .ok_or(TreeError::UnknownElement { element: id })
    }

    pub fn class(&self, id: ElementId) -> Option<ClassId> {
        Some(self.get(id)?.class)
    }

    pub fn parent(&self, id: ElementId) -> Option<ElementId> {
        self.get(id)?.links.parent
    }

    pub fn first_child(&self, id: ElementId) -> Option<ElementId> {
        self.get(id)?.links.first_child
    }

    pub fn last_child(&self, id: ElementId) -> Option<ElementId> {
        self.get(id)?.links.last_child
    }

    pub fn next_sibling(&self, id: ElementId) -> Option<ElementId> {
        self.get(id)?.links.next_sibling
    }

    pub fn prev_sibling(&self, id: ElementId) -> Option<ElementId> {
        self.get(id)?.links.prev_sibling
    }

    /// Children in draw order.
    pub fn children(&self, id: ElementId) -> Children<'_> {
        Children {
            tree: self,
            next: self.first_child(id),
        }
    }

    /// First direct child whose Id equals `id` (`XuiElement_GetChildById`,
    /// case-sensitive).
    pub fn child_by_id(&self, parent: ElementId, id: &[u16]) -> Option<ElementId> {
        self.children(parent)
            .find(|&child| self.get(child).and_then(Element::id) == Some(id))
    }

    pub fn flags(&self, id: ElementId) -> Option<ElementFlags> {
        Some(self.get(id)?.flags)
    }

    pub fn set_flags(&mut self, id: ElementId, flags: ElementFlags) -> Result<(), TreeError> {
        self.element_mut(id)?.flags = flags;

        Ok(())
    }

    pub fn is_shown(&self, id: ElementId) -> bool {
        self.get(id).is_some_and(Element::is_shown)
    }

    pub fn set_show(&mut self, id: ElementId, show: bool) -> Result<(), TreeError> {
        self.element_mut(id)?.flags.set(ElementFlags::SHOW, show);

        Ok(())
    }

    /// Sets or removes the image `id` draws before its content.
    pub fn set_custom_image(
        &mut self,
        id: ElementId,
        image: Option<CustomImage>,
    ) -> Result<(), TreeError> {
        self.element_mut(id)?.custom_image = image.map(Box::new);

        Ok(())
    }

    pub fn add_base_path(&mut self, path: &[u16]) -> BasePathId {
        if let Some(index) = self
            .base_paths
            .iter()
            .position(|existing| **existing == *path)
        {
            return BasePathId(index as u32);
        }
        self.base_paths.push(path.into());

        BasePathId(self.base_paths.len() as u32 - 1)
    }

    pub fn base_path_units(&self, id: BasePathId) -> &[u16] {
        // Ids come from `add_base_path` of this tree, which never removes.
        &self.base_paths[id.0 as usize]
    }

    pub fn base_path(&self, id: ElementId) -> Option<BasePathId> {
        self.get(id)?.base_path
    }

    pub fn set_base_path(
        &mut self,
        id: ElementId,
        base: Option<BasePathId>,
    ) -> Result<(), TreeError> {
        self.element_mut(id)?.base_path = base;

        Ok(())
    }

    /// Size changes since the last call, in occurrence order.
    pub fn take_size_changes(&mut self) -> Vec<SizeChange> {
        core::mem::take(&mut self.size_changes)
    }

    /// Inserts the detached `child` under `parent` before `before`, or last.
    /// Attaching captures the child's design layout against the new parent.
    pub fn insert_child(
        &mut self,
        parent: ElementId,
        child: ElementId,
        before: Option<ElementId>,
    ) -> Result<(), TreeError> {
        self.element(parent)?;
        if self.element(child)?.links.parent.is_some() {
            return Err(TreeError::AlreadyAttached { child });
        }
        let mut ancestor = Some(parent);
        while let Some(current) = ancestor {
            if current == child {
                return Err(TreeError::Cycle { parent, child });
            }
            ancestor = self.parent(current);
        }
        if let Some(before) = before
            && self.parent(before) != Some(parent)
        {
            return Err(TreeError::NotAChild { parent, before });
        }

        let prev = match before {
            Some(before) => self.prev_sibling(before),
            None => self.last_child(parent),
        };
        {
            let links = &mut self.element_mut(child)?.links;
            links.parent = Some(parent);
            links.prev_sibling = prev;
            links.next_sibling = before;
        }
        match prev {
            Some(prev) => self.element_mut(prev)?.links.next_sibling = Some(child),
            None => self.element_mut(parent)?.links.first_child = Some(child),
        }
        match before {
            Some(before) => self.element_mut(before)?.links.prev_sibling = Some(child),
            None => self.element_mut(parent)?.links.last_child = Some(child),
        }

        self.element_mut(child)?
            .flags
            .set(ElementFlags::TRANSFORM_DIRTY, true);
        self.capture_design(child);

        Ok(())
    }

    /// Appends `child` as the last child of `parent`.
    pub fn append_child(&mut self, parent: ElementId, child: ElementId) -> Result<(), TreeError> {
        self.insert_child(parent, child, None)
    }

    /// Detaches `id` from its parent; it keeps its children.
    pub fn unlink(&mut self, id: ElementId) -> Result<(), TreeError> {
        let links = self.element(id)?.links;
        let Some(parent) = links.parent else {
            return Ok(());
        };

        match links.prev_sibling {
            Some(prev) => self.element_mut(prev)?.links.next_sibling = links.next_sibling,
            None => self.element_mut(parent)?.links.first_child = links.next_sibling,
        }
        match links.next_sibling {
            Some(next) => self.element_mut(next)?.links.prev_sibling = links.prev_sibling,
            None => self.element_mut(parent)?.links.last_child = links.prev_sibling,
        }
        let links = &mut self.element_mut(id)?.links;
        links.parent = None;
        links.prev_sibling = None;
        links.next_sibling = None;

        Ok(())
    }

    /// Unlinks `id` and frees it with its whole subtree. Textures the
    /// elements own are destroyed on the next draw.
    pub fn destroy(&mut self, id: ElementId) -> Result<(), TreeError> {
        self.unlink(id)?;

        let mut pending = alloc::vec![id];
        while let Some(current) = pending.pop() {
            pending.extend(self.children(current));

            let slot = &mut self.slots[current.slot as usize];
            if let Some(mut element) = slot.element.take() {
                element
                    .content
                    .release_textures(&mut self.released_textures);
                if let Some(target) = element.effect_target.take() {
                    self.released_textures.push(target.texture);
                }
            }
            // A slot whose generation is used up is retired.
            if let Some(generation) = slot.generation.checked_add(1) {
                slot.generation = generation;
                self.free.push(current.slot);
            }
        }

        Ok(())
    }

    /// Moves every texture the elements own to `out`, those of destroyed
    /// elements included.
    pub(crate) fn release_textures(&mut self, out: &mut Vec<TextureId>) {
        out.append(&mut self.released_textures);

        for slot in &mut self.slots {
            if let Some(element) = &mut slot.element {
                element.content.release_textures(out);
                if let Some(target) = element.effect_target.take() {
                    out.push(target.texture);
                }
            }
        }
    }

    /// Ids of the resource textures the elements draw.
    pub(crate) fn cached_textures(&self) -> BTreeSet<TextureId> {
        self.slots
            .iter()
            .filter_map(|slot| slot.element.as_ref())
            .filter_map(|element| element.content.cached_texture())
            .map(|texture| texture.id)
            .collect()
    }

    /// World matrix: the local matrices of `id` and its ancestors.
    pub fn world_transform(&mut self, id: ElementId) -> Option<Matrix4> {
        let mut world = self.get_mut(id)?.local_transform();
        let mut ancestor = self.parent(id);
        while let Some(current) = ancestor {
            world = world.mul(&self.get_mut(current)?.local_transform());
            ancestor = self.parent(current);
        }

        Some(world)
    }

    /// Sets a property like the original setter for `path` (one definition,
    /// or a compound followed by its members). `index` selects an element
    /// of an indexed property.
    pub fn set_property(
        &mut self,
        id: ElementId,
        path: &[&'static PropDef],
        index: Option<u32>,
        value: PropValue,
    ) -> Result<(), PropertyError> {
        let element = self
            .get_mut(id)
            .ok_or(PropertyError::UnknownElement { element: id })?;
        let Some(&def) = path.last() else {
            return Err(PropertyError::EmptyPath);
        };
        if def.ty.scalar() != Some(value.scalar_type()) {
            return Err(PropertyError::TypeMismatch {
                name: def.name,
                expected: def.ty.scalar(),
                found: value.scalar_type(),
            });
        }
        match (def.flags.is_indexed(), index) {
            (true, None) => return Err(PropertyError::IndexRequired { name: def.name }),
            (false, Some(_)) => return Err(PropertyError::NotIndexed { name: def.name }),
            _ => {}
        }

        let size = element.size;
        let effect = match store_field(element, path, index, value)? {
            Stored::Handled(effect) => effect,
            Stored::Unhandled(value) => {
                element.store_extra(path, index, value);
                Effect::None
            }
        };
        match effect {
            Effect::None => {}
            Effect::Width(width) => {
                self.set_bounds(id, width, size.height);
                self.capture_design(id);
            }
            Effect::Height(height) => {
                self.set_bounds(id, size.width, height);
                self.capture_design(id);
            }
            Effect::Position(position) => self.set_position_from_setter(id, position),
            Effect::Scale(scale) => self.set_scale_anchored(id, scale),
        }

        Ok(())
    }

    /// Current value of a property: its field, the last value set, or the
    /// definition default. `None` for a missing element, an unset property
    /// without default, or an index past the list.
    pub fn property(
        &self,
        id: ElementId,
        path: &[&'static PropDef],
        index: Option<u32>,
    ) -> Option<PropValue> {
        let element = self.get(id)?;
        if let Some(text) = fetch_string(element, path) {
            return Some(PropValue::String(text.map(Into::into)));
        }
        if let Some(value) = fetch_value(element, path, index) {
            return Some(value);
        }
        if let Some(value) = element.extra(path, index) {
            return Some(value.clone());
        }

        path.last()?.default.map(PropValue::from_default)
    }

    /// A string property without copying. `None` also for the null string.
    pub fn string(&self, id: ElementId, path: &[&'static PropDef]) -> Option<&[u16]> {
        let element = self.get(id)?;
        if let Some(text) = fetch_string(element, path) {
            return text;
        }
        match element.extra(path, None) {
            Some(PropValue::String(text)) => text.as_deref(),
            _ => None,
        }
    }
}

/// Children of an element in draw order.
#[derive(Clone)]
pub struct Children<'t> {
    tree: &'t ElementTree,
    next: Option<ElementId>,
}

impl Iterator for Children<'_> {
    type Item = ElementId;

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next?;
        self.next = self.tree.next_sibling(current);

        Some(current)
    }
}

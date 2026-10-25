//! Scene text localization: the per-scene `.xus` table applied when a file
//! loads with a locale set, and the re-localization of loaded scenes
//! after a locale change (`0x9214A458`, doc/xui-controls.md "Scene text
//! localization").

mod fixed;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::vec::Vec;

use exui_core::Document;
use exui_core::schema::PropDef;
use exui_core::schema::ScalarType;

use crate::control::Builtin;
use crate::message::Body;
use crate::message::Context;
use crate::resource::EntryKeys;
use crate::resource::StringTable;
use crate::scene;
use crate::scene::BasePathId;
use crate::scene::ElementId;
use crate::scene::PropValue;

/// Property flag 0x4: the value is localized text.
const LOCALIZED: u32 = 0x4;

/// Text to apply to a scene tree.
enum TextTable {
    /// Values of the file's own tree by key path (`Id.Id.Property`).
    Loaded(BTreeMap<Vec<u16>, Vec<u16>>),
    /// The scene's `.xus` table of the current locale.
    Localized(StringTable),
}

impl TextTable {
    /// Whether lookups use the key path.
    fn keyed_by_name(&self) -> bool {
        match self {
            Self::Loaded(_) => true,
            Self::Localized(table) => table.keys() == EntryKeys::Name,
        }
    }

    /// `0x92171020`: the value for the property with key path `key` and
    /// key `prop_key` of the element with load index `element`.
    fn lookup(&self, key: &[u16], prop_key: u16, element: Option<u32>) -> Option<&[u16]> {
        match self {
            Self::Loaded(values) => values.get(key).map(Vec::as_slice),
            Self::Localized(table) => match table.keys() {
                // An element without a load index is never looked up.
                EntryKeys::Id => {
                    let element = element?;
                    table.by_id((element & 0xFFFF) | (u32::from(prop_key) << 16))
                }
                EntryKeys::Name => table.by_name(key),
                // Index tables carry no keys to match.
                EntryKeys::Index => None,
            },
        }
    }
}

/// One element of a walk and the key path of its parent.
struct Walk {
    element: ElementId,
    prefix: Vec<u16>,
}

impl Context<'_, '_> {
    /// `XuiLoadFromBinary` 0x9214A9F8 after instantiation: `root` stores
    /// `path` when it is a canvas or scene (0x92145158), and with a locale
    /// set the file's `.xus` table is applied to it (`0x9214A458(root, 0,
    /// 0)`).
    pub(crate) fn localize_loaded(&mut self, root: ElementId, path: BasePathId) {
        if !self.stores_load_path(root) {
            return;
        }

        let _ = self.tree.set_load_path(root, Some(path));
        if self.resources.locale().is_some() {
            self.apply_localized_table(root, path);
        }
    }

    /// `0x9214A9F0`: restores the text of `element`'s tree from the file it
    /// came from, then applies the file's `.xus` table when a locale is
    /// set. The file is the load path of `element` or of its nearest
    /// ancestor scene or canvas that has one.
    pub fn relocalize(&mut self, element: ElementId) {
        let Some(path) = self.bubble_load_path(element) else {
            return;
        };

        if let Some(values) = self.loaded_text(path) {
            self.apply_text(element, &TextTable::Loaded(values));
        }
        if self.resources.locale().is_some() {
            self.apply_localized_table(element, path);
        }
    }

    /// `0x9215B560`, the 0x1f handler of XuiScene and XuiCanvas: only an
    /// element with its own load path is re-localized.
    pub(crate) fn relocalize_own(&mut self, element: ElementId) {
        if self.own_load_path(element).is_some() {
            self.relocalize(element);
        }
    }

    /// `0x921505C8`, from `XuiSceneNavigateBack`: when `current` received
    /// 0x1f, 0x1f is broadcast from the first of `target` and its ancestors
    /// that has a load path (the topmost when none has), and the flag of
    /// `current` is cleared.
    pub(crate) fn relocalize_back_target(&mut self, current: ElementId, target: ElementId) {
        if !self
            .scene_state(current)
            .is_some_and(|state| state.relocalized)
        {
            return;
        }

        let mut holder = target;
        while self.own_load_path(holder).is_none() {
            let Some(parent) = self.tree.parent(holder) else {
                break;
            };
            holder = parent;
        }
        self.broadcast(holder, &Body::Relocalize);

        if let Some(state) = self.scene_state(current) {
            state.relocalized = false;
        }
    }

    /// Whether the element keeps a load path (message 0x19 is handled by
    /// XuiScene and XuiCanvas only).
    fn stores_load_path(&self, element: ElementId) -> bool {
        self.is_builtin_a(element, Builtin::Scene) || self.is_builtin_a(element, Builtin::Canvas)
    }

    /// Message 0x18 sent to the element itself.
    fn own_load_path(&self, element: ElementId) -> Option<BasePathId> {
        if !self.stores_load_path(element) {
            return None;
        }

        self.tree.load_path(element)
    }

    /// Message 0x18 bubbled from `element` (`0x92145100`). A scene without
    /// a path passes it on; a canvas answers even without one.
    fn bubble_load_path(&self, element: ElementId) -> Option<BasePathId> {
        let mut current = Some(element);
        while let Some(node) = current {
            if self.is_builtin_a(node, Builtin::Canvas) {
                return self.tree.load_path(node);
            }
            if self.is_builtin_a(node, Builtin::Scene)
                && let Some(path) = self.tree.load_path(node)
            {
                return Some(path);
            }
            current = self.tree.parent(node);
        }

        None
    }

    /// The localized text of the file at `path` as loaded: the file is
    /// instantiated again, its localized property values are read and the
    /// copy is destroyed (`0x92171918`).
    fn loaded_text(&mut self, path: BasePathId) -> Option<BTreeMap<Vec<u16>, Vec<u16>>> {
        let units: Vec<u16> = self.tree.base_path_units(path).into();
        let bytes = self.resources.read_path(&units, None).ok()?;
        let document = Document::parse(&bytes, self.registry).ok()?;
        let copy = scene::instantiate(
            self.tree,
            self.registry,
            &document,
            document.root(),
            None,
            &mut |_, _, _| {},
        )
        .ok()?;

        let mut values = BTreeMap::new();
        let mut pending = Vec::new();
        pending.push(Walk {
            element: copy,
            prefix: Vec::new(),
        });
        while let Some(walk) = pending.pop() {
            let Some(prefix) = self.element_prefix(&walk) else {
                self.push_children(&mut pending, walk.element, walk.prefix);
                continue;
            };

            for (def, _) in self.property_keys(walk.element) {
                if def.flags.bits() & LOCALIZED == 0 {
                    continue;
                }
                let Some(text) = self
                    .tree
                    .property(walk.element, &[def], None)
                    .and_then(|value| value_text(&value))
                else {
                    continue;
                };
                let mut key = prefix.clone();
                key.extend(def.name.encode_utf16());
                // Genuine scenes have no two elements with one key path;
                // the original's choice among equal keys follows its sort.
                values.entry(key).or_insert(text);
            }
            self.push_children(&mut pending, walk.element, prefix);
        }

        self.destroy(copy);

        Some(values)
    }

    /// Applies the `.xus` table next to `path` (`XuiPathReplaceExtWithXus`
    /// 0x92140B60, then `XuiStringTableLoad`).
    fn apply_localized_table(&mut self, element: ElementId, path: BasePathId) {
        let mut units: Vec<u16> = self.tree.base_path_units(path).into();
        let Some(start) = units.len().checked_sub(3).filter(|&start| start > 0) else {
            return;
        };
        units.truncate(start);
        units.extend("xus".encode_utf16());

        let Ok(locator) = self.resources.resolve(&units, None) else {
            return;
        };
        let Ok(table) = self.resources.string_table(&locator) else {
            return;
        };

        self.apply_text(element, &TextTable::Localized(table));
    }

    /// `0x92171908`: walks the tree of `element` and sets every property
    /// the table has a value for.
    fn apply_text(&mut self, element: ElementId, table: &TextTable) {
        let by_name = table.keyed_by_name();
        let mut key = Vec::new();
        let mut pending = Vec::new();
        pending.push(Walk {
            element,
            prefix: Vec::new(),
        });
        while let Some(walk) = pending.pop() {
            let Some(prefix) = self.element_prefix(&walk) else {
                self.push_children(&mut pending, walk.element, walk.prefix);
                continue;
            };

            let index = self
                .tree
                .load_index(walk.element)
                .map(|index| index.0.get());
            for (def, prop_key) in self.property_keys(walk.element) {
                key.clear();
                if by_name {
                    key.extend_from_slice(&prefix);
                    key.extend(def.name.encode_utf16());
                }
                let Some(value) = table
                    .lookup(&key, prop_key, index)
                    .and_then(|text| text_value(text, def))
                else {
                    continue;
                };
                // `XuiObjectSetPropertyById`; its result is ignored.
                let _ = self.set_property(walk.element, &[def], None, value);
            }
            self.push_children(&mut pending, walk.element, prefix);
        }
    }

    /// `0x921716E8`: the key path of the element (`Id.` or, without an
    /// Id, `ClassName.` after its parent's), or `None` for an element
    /// whose class is XuiCanvas, whose own properties are skipped.
    fn element_prefix(&self, walk: &Walk) -> Option<Vec<u16>> {
        let class = self.registry.class(self.tree.class(walk.element)?)?;
        if class.name() == "XuiCanvas" {
            return None;
        }

        let mut prefix = walk.prefix.clone();
        match self.tree.get(walk.element).and_then(|element| element.id()) {
            Some(id) => prefix.extend_from_slice(id),
            None => prefix.extend(class.name().encode_utf16()),
        }
        prefix.push(u16::from(b'.'));

        Some(prefix)
    }

    /// Queues the children the walk enters, first child first: not those
    /// with their own load path and not those of class XuiVisual.
    fn push_children(&self, pending: &mut Vec<Walk>, element: ElementId, prefix: Vec<u16>) {
        let children: Vec<ElementId> = self
            .tree
            .children(element)
            .filter(|&child| self.own_load_path(child).is_none())
            .filter(|&child| {
                self.tree
                    .class(child)
                    .and_then(|class| self.registry.class(class))
                    .is_none_or(|class| class.name() != "XuiVisual")
            })
            .collect();
        for child in children.into_iter().rev() {
            pending.push(Walk {
                element: child,
                prefix: prefix.clone(),
            });
        }
    }

    /// `0x921715C0`: the scalar, non-indexed property definitions of every
    /// class level, root level first, each with its key `level << 8 |
    /// position` (level 0 is the root class).
    fn property_keys(&self, element: ElementId) -> Vec<(&'static PropDef, u16)> {
        let Some(class) = self
            .tree
            .class(element)
            .and_then(|class| self.registry.class(class))
        else {
            return Vec::new();
        };

        let mut keys = Vec::new();
        for (level, class) in class.levels().enumerate() {
            for (position, def) in class.def().props.iter().enumerate() {
                // No localized property is compound or indexed.
                if def.ty.scalar().is_none() || def.flags.is_indexed() {
                    continue;
                }
                let key = ((level as u16) << 8).wrapping_add(position as u16);
                keys.push((def, key));
            }
        }

        keys
    }
}

/// `0x921568F0`: a value as text. The null string reads as empty text.
/// Localized properties are strings, floats and integers only.
fn value_text(value: &PropValue) -> Option<Vec<u16>> {
    let text = match value {
        PropValue::String(text) => return Some(text.as_deref().unwrap_or_default().to_vec()),
        // `_snwprintf` `%f` of the float widened to a double.
        PropValue::Float(value) => {
            let text = fixed::format_fixed(f64::from(*value), 6);
            return Some(text.into_iter().map(u16::from).collect());
        }
        PropValue::Integer(value) => format!("{value}"),
        PropValue::Unsigned(value) => format!("{value}"),
        _ => return None,
    };

    Some(text.encode_utf16().collect())
}

/// `0x92140EE8`: text as a value of the property's type. Empty text is the
/// null string; numbers are read like `swscanf` `%f`, `%d` and `%u`.
fn text_value(text: &[u16], def: &PropDef) -> Option<PropValue> {
    let value = match def.ty.scalar()? {
        ScalarType::String => {
            PropValue::String((!text.is_empty()).then(|| text.to_vec().into_boxed_slice()))
        }
        ScalarType::Float => PropValue::Float(scan_number(text, true)?.parse::<f64>().ok()? as f32),
        ScalarType::Integer => {
            PropValue::Integer(scan_number(text, false)?.parse::<i64>().ok()? as i32)
        }
        ScalarType::Unsigned => {
            PropValue::Unsigned(scan_number(text, false)?.parse::<i64>().ok()? as u32)
        }
        _ => return None,
    };

    Some(value)
}

/// The leading number of `text` after white space: a sign and digits, and
/// for a float a fraction and an exponent. `None` without a digit.
fn scan_number(text: &[u16], float: bool) -> Option<alloc::string::String> {
    let mut chars = text
        .iter()
        .map(|&unit| char::from_u32(u32::from(unit)).unwrap_or('\u{0}'))
        .skip_while(|c| c.is_ascii_whitespace())
        .peekable();
    let mut number = alloc::string::String::new();

    if let Some(&sign) = chars.peek()
        && (sign == '+' || sign == '-')
    {
        number.push(sign);
        chars.next();
    }
    let mut digits = 0;
    while let Some(&digit) = chars.peek().filter(|c| c.is_ascii_digit()) {
        number.push(digit);
        digits += 1;
        chars.next();
    }
    if float && chars.peek() == Some(&'.') {
        number.push('.');
        chars.next();
        while let Some(&digit) = chars.peek().filter(|c| c.is_ascii_digit()) {
            number.push(digit);
            digits += 1;
            chars.next();
        }
    }
    if digits == 0 {
        return None;
    }
    if float && matches!(chars.peek(), Some('e' | 'E')) {
        let mut exponent = alloc::string::String::from("e");
        chars.next();
        if let Some(&sign) = chars.peek()
            && (sign == '+' || sign == '-')
        {
            exponent.push(sign);
            chars.next();
        }
        let mut exponent_digits = 0;
        while let Some(&digit) = chars.peek().filter(|c| c.is_ascii_digit()) {
            exponent.push(digit);
            exponent_digits += 1;
            chars.next();
        }
        if exponent_digits > 0 {
            number.push_str(&exponent);
        }
    }

    Some(number)
}

#[cfg(test)]
mod tests;

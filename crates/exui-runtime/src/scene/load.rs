//! Element creation from a parsed XUR document.

use alloc::vec::Vec;
use core::num::NonZeroU32;

use exui_core::Document;
use exui_core::Registry;
use exui_core::custom::FigurePoints;
use exui_core::schema::PropDef;
use exui_core::schema::xui;
use exui_core::xur::ObjectRef;
use exui_core::xur::Property;
use exui_core::xur::Value;

use crate::scene::BasePathId;
use crate::scene::ElementId;
use crate::scene::ElementTree;
use crate::scene::LoadError;
use crate::scene::LoadIndex;
use crate::scene::PropValue;
use crate::scene::PropertyError;

/// Creates a detached element tree for `object` and its descendants. Each
/// element is created with its class defaults, receives its loaded
/// properties in file order and is then attached to its parent (which
/// captures its design layout), parents before children [INFERENCE: the
/// instantiation pass was not traced; this order keeps every capture
/// against the parent's loaded size]. Every element gets its
/// [`LoadIndex`]. `visit` sees every element after its whole subtree
/// exists, parents first. Every element gets `base`.
pub fn instantiate<'d, 'a>(
    tree: &mut ElementTree,
    registry: &Registry,
    document: &'d Document<'a>,
    object: ObjectRef<'d, 'a>,
    base: Option<BasePathId>,
    visit: &mut dyn FnMut(&mut ElementTree, ElementId, ObjectRef<'d, 'a>),
) -> Result<ElementId, LoadError> {
    let root = create(tree, registry, document, object, base)?;
    let mut created = alloc::vec![(root, object)];
    if let Err(error) = create_descendants(tree, registry, document, base, &mut created) {
        // Partial trees are not kept.
        let _ = tree.destroy(root);
        return Err(error);
    }

    number_elements(tree, root);

    for (element, object) in created {
        visit(tree, element, object);
    }

    Ok(root)
}

/// Creates and attaches the descendants of `created[0]` depth first in file
/// order, recording each in `created`.
fn create_descendants<'d, 'a>(
    tree: &mut ElementTree,
    registry: &Registry,
    document: &'d Document<'a>,
    base: Option<BasePathId>,
    created: &mut Vec<(ElementId, ObjectRef<'d, 'a>)>,
) -> Result<(), LoadError> {
    let (root, object) = created[0];
    let mut pending = Vec::new();
    push_children(&mut pending, object, root);

    while let Some((current, parent)) = pending.pop() {
        let element = create(tree, registry, document, current, base)?;
        if let Err(error) = tree.append_child(parent, element) {
            let _ = tree.destroy(element);
            return Err(error.into());
        }
        created.push((element, current));
        push_children(&mut pending, current, element);
    }

    Ok(())
}

/// Numbers the subtree of `root` like `XuiInstantiateObjectTree`
/// 0x921523F8: each child after its own subtree, from 1, and the root last
/// (`XuiCreateFromLoadedObject` 0x92152B90).
fn number_elements(tree: &mut ElementTree, root: ElementId) {
    let mut next = NonZeroU32::MIN;
    // Each entry is an element and whether its children are numbered.
    let mut pending = alloc::vec![(root, false)];
    while let Some((element, children_done)) = pending.pop() {
        if !children_done {
            pending.push((element, true));
            let start = pending.len();
            pending.extend(tree.children(element).map(|child| (child, false)));
            pending[start..].reverse();
            continue;
        }

        tree.set_load_index(element, Some(LoadIndex(next)));
        next = next.saturating_add(1);
    }
}

/// Queues the children of `object` so that the first pops first.
fn push_children<'d, 'a>(
    pending: &mut Vec<(ObjectRef<'d, 'a>, ElementId)>,
    object: ObjectRef<'d, 'a>,
    element: ElementId,
) {
    let start = pending.len();
    pending.extend(object.children().map(|child| (child, element)));
    pending[start..].reverse();
}

/// One element with its properties, before it is attached.
fn create(
    tree: &mut ElementTree,
    registry: &Registry,
    document: &Document<'_>,
    object: ObjectRef<'_, '_>,
    base: Option<BasePathId>,
) -> Result<ElementId, LoadError> {
    let Some(authored) = registry.class(object.class()) else {
        return Err(LoadError::UnknownClass {
            class: object.class(),
        });
    };

    // ClassOverride: an unknown name is ignored; a class not derived from
    // the authored one fails the load.
    let class_override = &xui::XUI_CONTROL.props[0];
    let class = match object
        .property(class_override)
        .and_then(|property| document.value(property))
        .and_then(Value::as_string)
        .flatten()
    {
        Some(name) => match document
            .string(name)
            .and_then(|name| registry.find_wide(name))
            .and_then(|id| registry.class(id))
        {
            Some(candidate) if candidate.is_a(authored.id()) => candidate,
            Some(candidate) => {
                return Err(LoadError::ClassOverrideNotDerived {
                    authored: authored.name(),
                    override_name: candidate.name(),
                });
            }
            None => authored,
        },
        None => authored,
    };

    let element = tree.create(class);
    tree.set_base_path(element, base)?;

    let mut path = Vec::new();
    for property in object.properties() {
        path.clear();
        path.push(property.def());
        if let Err(error) = apply(tree, element, document, property, &mut path) {
            let _ = tree.destroy(element);
            return Err(error);
        }
    }

    Ok(element)
}

fn apply(
    tree: &mut ElementTree,
    element: ElementId,
    document: &Document<'_>,
    property: &Property,
    path: &mut Vec<&'static PropDef>,
) -> Result<(), LoadError> {
    let def = property.def();
    let indexed = def.flags.is_indexed();

    for (index, value) in document.values(property).iter().enumerate() {
        if let Value::Compound(compound) = value {
            for member in document.compound(*compound) {
                path.push(member.def());
                apply(tree, element, document, member, path)?;
                path.pop();
            }
            continue;
        }

        let Some(value) = convert(document, def, value)? else {
            continue;
        };
        let index = indexed.then_some(index as u32);
        tree.set_property(element, path, index, value)
            .map_err(|source| LoadError::Property {
                name: def.name,
                source,
            })?;
    }

    Ok(())
}

/// Owned value of a loaded entry. `None` when the original applies nothing
/// (a Points blob shorter than its size header).
fn convert(
    document: &Document<'_>,
    def: &'static PropDef,
    value: &Value,
) -> Result<Option<PropValue>, LoadError> {
    let missing = LoadError::MissingTableEntry { name: def.name };
    let converted = match *value {
        Value::Bool(value) => PropValue::Bool(value),
        Value::Integer(value) => PropValue::Integer(value),
        Value::Unsigned(value) => PropValue::Unsigned(value),
        Value::Float(value) => PropValue::Float(value),
        // An index past the string table reads as the null string.
        Value::String(id) => PropValue::String(
            id.and_then(|id| document.string(id))
                .map(|text| text.units().collect()),
        ),
        Value::Color(value) => PropValue::Color(value),
        Value::Vector(index) => PropValue::Vector(document.vector(index).ok_or(missing)?),
        Value::Quaternion(index) => {
            PropValue::Quaternion(document.quaternion(index).ok_or(missing)?)
        }
        Value::Custom(offset) => match document.custom_data(offset).and_then(FigurePoints::parse) {
            Some(points) => PropValue::Points(points),
            None => return Ok(None),
        },
        Value::Compound(_) => {
            return Err(LoadError::Property {
                name: def.name,
                source: PropertyError::Compound { name: def.name },
            });
        }
    };

    Ok(Some(converted))
}

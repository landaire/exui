//! Element creation from a parsed XUR document.

use alloc::vec::Vec;
use core::num::NonZeroU32;

use exui_core::Document;
use exui_core::Registry;
use exui_core::custom::FigurePoints;
use exui_core::schema::ClassId;
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

/// A skin string-table replacement for `ClassOverride`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LocalizedClass {
    /// No replacement exists; use the authored property.
    Authored,
    /// The replacement names no registered class; use the authored class.
    Unknown,
    /// The replacement names this registered class.
    Override(ClassId),
}
/// A skin string-table replacement for one scalar property.
#[derive(Clone, Debug)]
pub(crate) struct LocalizedProperty {
    pub(crate) def: &'static PropDef,
    pub(crate) value: PropValue,
}

/// Creates a detached element tree for `object` and its descendants. Each
/// element is created with its class defaults, receives its loaded
/// properties in file order and is then attached to its parent before its
/// children are created. Every element gets its [`LoadIndex`]. `visit` sees
/// every element after the whole tree exists, parents first. Every element
/// gets `base`.
pub fn instantiate<'d, 'a>(
    tree: &mut ElementTree,
    registry: &Registry,
    document: &'d Document<'a>,
    object: ObjectRef<'d, 'a>,
    base: Option<BasePathId>,
    visit: &mut dyn FnMut(&mut ElementTree, ElementId, ObjectRef<'d, 'a>),
) -> Result<ElementId, LoadError> {
    instantiate_with_properties(
        tree,
        registry,
        document,
        object,
        base,
        &mut |_| LocalizedClass::Authored,
        &mut |_| Vec::new(),
        visit,
    )
}

/// `localized_properties` replaces authored scalar properties in place and
/// supplies localized properties that were not authored.
pub(crate) fn instantiate_with_properties<'d, 'a>(
    tree: &mut ElementTree,
    registry: &Registry,
    document: &'d Document<'a>,
    object: ObjectRef<'d, 'a>,
    base: Option<BasePathId>,
    localized_class: &mut dyn FnMut(ObjectRef<'d, 'a>) -> LocalizedClass,
    localized_properties: &mut dyn FnMut(ObjectRef<'d, 'a>) -> Vec<LocalizedProperty>,
    visit: &mut dyn FnMut(&mut ElementTree, ElementId, ObjectRef<'d, 'a>),
) -> Result<ElementId, LoadError> {
    let root = create(
        tree,
        registry,
        document,
        object,
        base,
        localized_class,
        localized_properties,
    )?;
    let mut created = alloc::vec![(root, object)];
    if let Err(error) = create_descendants(
        tree,
        registry,
        document,
        base,
        localized_class,
        localized_properties,
        &mut created,
    ) {
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
    localized_class: &mut dyn FnMut(ObjectRef<'d, 'a>) -> LocalizedClass,
    localized_properties: &mut dyn FnMut(ObjectRef<'d, 'a>) -> Vec<LocalizedProperty>,
    created: &mut Vec<(ElementId, ObjectRef<'d, 'a>)>,
) -> Result<(), LoadError> {
    let (root, object) = created[0];
    let mut pending = Vec::new();
    push_children(&mut pending, object, root);

    while let Some((current, parent)) = pending.pop() {
        let element = create(
            tree,
            registry,
            document,
            current,
            base,
            localized_class,
            localized_properties,
        )?;
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
fn create<'d, 'a>(
    tree: &mut ElementTree,
    registry: &Registry,
    document: &'d Document<'a>,
    object: ObjectRef<'d, 'a>,
    base: Option<BasePathId>,
    localized_class: &mut dyn FnMut(ObjectRef<'d, 'a>) -> LocalizedClass,
    localized_properties: &mut dyn FnMut(ObjectRef<'d, 'a>) -> Vec<LocalizedProperty>,
) -> Result<ElementId, LoadError> {
    let Some(authored) = registry.class(object.class()) else {
        return Err(LoadError::UnknownClass {
            class: object.class(),
        });
    };

    let authored_override = || {
        object
            .property(&xui::XUI_CONTROL.props[0])
            .and_then(|property| document.value(property))
            .and_then(Value::as_string)
            .flatten()
            .and_then(|name| document.string(name))
            .and_then(|name| registry.find_wide(name))
    };
    let override_class = match localized_class(object) {
        LocalizedClass::Authored => authored_override(),
        LocalizedClass::Unknown => None,
        LocalizedClass::Override(class) => Some(class),
    };
    // An unknown name is ignored; a class not derived from the authored one
    // fails the load.
    let class = match override_class.and_then(|class| registry.class(class)) {
        Some(candidate) if candidate.is_a(authored.id()) => candidate,
        Some(candidate) => {
            return Err(LoadError::ClassOverrideNotDerived {
                authored: authored.name(),
                override_name: candidate.name(),
            });
        }
        None => authored,
    };

    let element = tree.create(class);
    tree.set_base_path(element, base)?;

    let mut localized = localized_properties(object);
    let mut path = Vec::new();
    for property in object.properties() {
        path.clear();
        path.push(property.def());
        let result = localized
            .iter()
            .position(|replacement| core::ptr::eq(replacement.def, property.def()))
            .map(|index| {
                let replacement = localized.remove(index);
                tree.set_property(element, &[replacement.def], None, replacement.value)
                    .map_err(|source| LoadError::Property {
                        name: replacement.def.name,
                        source,
                    })
            })
            .unwrap_or_else(|| apply(tree, element, document, property, &mut path));
        if let Err(error) = result {
            let _ = tree.destroy(element);
            return Err(error);
        }
    }
    for replacement in localized {
        if let Err(source) = tree.set_property(element, &[replacement.def], None, replacement.value)
        {
            let _ = tree.destroy(element);
            return Err(LoadError::Property {
                name: replacement.def.name,
                source,
            });
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

//! XUI class registry: classes, base classes and property definitions.
//!
//! Tables come from the `XuiRegisterClass` call sites of dash 6717; see
//! doc/xui-classes.md.

pub mod dash;
pub mod xui;

use alloc::vec::Vec;

use crate::error::RegisterError;
use crate::value::Color;
use crate::value::Quaternion;
use crate::value::Vector3;
use crate::wide::WideStr;

/// Property value type.
#[derive(Clone, Copy, Debug)]
pub enum PropType {
    Bool,
    Integer,
    Unsigned,
    Float,
    String,
    Color,
    Vector,
    Quaternion,
    /// A nested property block over these sub-definitions.
    Compound(&'static [PropDef]),
    Custom,
}

impl PropType {
    /// Type code of the original property definition record.
    pub const fn code(self) -> u8 {
        match self {
            Self::Bool => 1,
            Self::Integer => 2,
            Self::Unsigned => 3,
            Self::Float => 4,
            Self::String => 5,
            Self::Color => 6,
            Self::Vector => 7,
            Self::Quaternion => 8,
            Self::Compound(_) => 9,
            Self::Custom => 10,
        }
    }

    /// The type of a single stored value; `None` for compounds.
    pub const fn scalar(self) -> Option<ScalarType> {
        match self.kind() {
            PropKind::Scalar(scalar) => Some(scalar),
            PropKind::Compound(_) => None,
        }
    }

    /// Splits the type into scalars and compounds for exhaustive matching.
    pub const fn kind(self) -> PropKind {
        let scalar = match self {
            Self::Bool => ScalarType::Bool,
            Self::Integer => ScalarType::Integer,
            Self::Unsigned => ScalarType::Unsigned,
            Self::Float => ScalarType::Float,
            Self::String => ScalarType::String,
            Self::Color => ScalarType::Color,
            Self::Vector => ScalarType::Vector,
            Self::Quaternion => ScalarType::Quaternion,
            Self::Custom => ScalarType::Custom,
            Self::Compound(defs) => return PropKind::Compound(defs),
        };

        PropKind::Scalar(scalar)
    }

    /// Sub-definitions of a compound type.
    pub const fn compound(self) -> Option<&'static [PropDef]> {
        match self {
            Self::Compound(defs) => Some(defs),
            _ => None,
        }
    }
}

/// A [`PropType`] as either a single value or a nested block.
#[derive(Clone, Copy, Debug)]
pub enum PropKind {
    Scalar(ScalarType),
    Compound(&'static [PropDef]),
}

/// Every [`PropType`] except compounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScalarType {
    Bool,
    Integer,
    Unsigned,
    Float,
    String,
    Color,
    Vector,
    Quaternion,
    Custom,
}

/// Property definition flag word.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PropFlags(u32);

impl PropFlags {
    /// The property holds a counted list of values. The loader reads a count
    /// prefix before the values. The other bits do not affect loading.
    pub const INDEXED: Self = Self(0x1);

    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn is_indexed(self) -> bool {
        self.contains(Self::INDEXED)
    }
}

/// Default value attached to a property definition. The variant follows the
/// type of the stored default, which in one case differs from the property
/// type (`ScriptData.Selected`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DefaultValue {
    Bool(bool),
    Integer(i32),
    Unsigned(u32),
    Float(f32),
    /// `None` is the null string.
    String(Option<&'static str>),
    Color(Color),
    Vector(Vector3),
    Quaternion(Quaternion),
}

/// A property definition. Definitions are identified by address
/// (`core::ptr::eq`): distinct classes may declare equal-looking records,
/// such as `XuiElement.Id` and `XuiEffect.Id`.
#[derive(Debug)]
pub struct PropDef {
    pub name: &'static str,
    pub ty: PropType,
    pub flags: PropFlags,
    pub default: Option<DefaultValue>,
}

impl PropDef {
    pub const fn new(
        name: &'static str,
        ty: PropType,
        flags: PropFlags,
        default: Option<DefaultValue>,
    ) -> Self {
        Self {
            name,
            ty,
            flags,
            default,
        }
    }
}

#[derive(Debug)]
pub struct ClassDef {
    pub name: &'static str,
    pub base: Option<&'static str>,
    /// Properties declared by this class level only, in bitmask order.
    pub props: &'static [PropDef],
}

impl ClassDef {
    pub const fn new(
        name: &'static str,
        base: Option<&'static str>,
        props: &'static [PropDef],
    ) -> Self {
        Self { name, base, props }
    }
}

/// Index of a class in the [`Registry`] that issued it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClassId(u16);

impl ClassId {
    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug)]
struct RegisteredClass {
    def: &'static ClassDef,
    base: Option<ClassId>,
    /// Number of base classes above this one.
    depth: u16,
}

/// Registered classes. Names compare case sensitively, as in the original.
#[derive(Debug)]
pub struct Registry {
    classes: Vec<RegisteredClass>,
}

impl Registry {
    /// Registry holding the classes `XuiInit` registers, in its order.
    pub fn new() -> Self {
        let mut registry = Self {
            classes: Vec::new(),
        };

        for def in xui::BUILTIN {
            // The built-in table is base-first and free of duplicates.
            registry
                .register(def)
                .expect("built-in XUI class table is consistent");
        }

        registry
    }

    /// Adds a class. The base class must already be registered and the name
    /// must be new, as `XuiRegisterClass` requires.
    pub fn register(&mut self, def: &'static ClassDef) -> Result<ClassId, RegisterError> {
        if self.find(def.name).is_some() {
            return Err(RegisterError::AlreadyRegistered { name: def.name });
        }

        let (base, depth) = match def.base {
            Some(base_name) => {
                let Some(base) = self.find(base_name) else {
                    return Err(RegisterError::BaseNotRegistered {
                        name: def.name,
                        base: base_name,
                    });
                };
                (Some(base), self.classes[base.index()].depth + 1)
            }
            None => (None, 0),
        };

        let Ok(index) = u16::try_from(self.classes.len()) else {
            return Err(RegisterError::RegistryFull { name: def.name });
        };

        self.classes.push(RegisteredClass { def, base, depth });

        Ok(ClassId(index))
    }

    /// Registers each class in order, stopping at the first failure.
    pub fn register_all(&mut self, defs: &[&'static ClassDef]) -> Result<(), RegisterError> {
        for def in defs {
            self.register(def)?;
        }

        Ok(())
    }

    pub fn find(&self, name: &str) -> Option<ClassId> {
        self.position(|def| def.name == name)
    }

    pub fn find_wide(&self, name: WideStr<'_>) -> Option<ClassId> {
        self.position(|def| name.eq_str(def.name))
    }

    pub fn find_utf16(&self, name: &[u16]) -> Option<ClassId> {
        self.position(|def| def.name.encode_utf16().eq(name.iter().copied()))
    }

    /// `None` when `id` was issued by another registry.
    pub fn class(&self, id: ClassId) -> Option<ClassRef<'_>> {
        self.classes.get(id.index())?;

        Some(ClassRef { registry: self, id })
    }

    /// Registered classes in registration order.
    pub fn classes(&self) -> impl Iterator<Item = ClassRef<'_>> {
        (0..self.classes.len()).map(|index| ClassRef {
            registry: self,
            id: ClassId(index as u16),
        })
    }

    pub fn len(&self) -> usize {
        self.classes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.classes.is_empty()
    }

    fn position(&self, matches: impl Fn(&ClassDef) -> bool) -> Option<ClassId> {
        let index = self.classes.iter().position(|class| matches(class.def))?;

        u16::try_from(index).ok().map(ClassId)
    }

    fn entry(&self, id: ClassId) -> &RegisteredClass {
        // ClassRef is only built for ids of this registry.
        &self.classes[id.index()]
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

/// A class of a [`Registry`].
#[derive(Clone, Copy, Debug)]
pub struct ClassRef<'r> {
    registry: &'r Registry,
    id: ClassId,
}

impl<'r> ClassRef<'r> {
    pub fn id(self) -> ClassId {
        self.id
    }

    pub fn def(self) -> &'static ClassDef {
        self.registry.entry(self.id).def
    }

    pub fn name(self) -> &'static str {
        self.def().name
    }

    pub fn base(self) -> Option<ClassRef<'r>> {
        let base = self.registry.entry(self.id).base?;

        Some(ClassRef {
            registry: self.registry,
            id: base,
        })
    }

    /// Class `levels_up` base steps above this one.
    pub fn ancestor(self, levels_up: u8) -> Option<ClassRef<'r>> {
        let mut class = self;
        for _ in 0..levels_up {
            class = class.base()?;
        }

        Some(class)
    }

    /// Class levels from the root base class down to this class, the order
    /// in which the loader reads property blocks.
    pub fn levels(self) -> Levels<'r> {
        let depth = self.registry.entry(self.id).depth;

        Levels {
            leaf: self,
            remaining: depth + 1,
        }
    }

    /// Whether this class is `ancestor` or derives from it.
    pub fn is_a(self, ancestor: ClassId) -> bool {
        let mut class = Some(self);
        while let Some(current) = class {
            if current.id == ancestor {
                return true;
            }
            class = current.base();
        }

        false
    }
}

/// Root-first walk over a class and its bases.
#[derive(Clone, Debug)]
pub struct Levels<'r> {
    leaf: ClassRef<'r>,
    remaining: u16,
}

impl<'r> Iterator for Levels<'r> {
    type Item = ClassRef<'r>;

    fn next(&mut self) -> Option<Self::Item> {
        self.remaining = self.remaining.checked_sub(1)?;

        // Depth is small (at most a handful of levels), so walking up from
        // the leaf each step costs less than storing the chain.
        let mut class = self.leaf;
        for _ in 0..self.remaining {
            class = class.base()?;
        }

        Some(class)
    }
}

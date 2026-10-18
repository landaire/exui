use std::vec::Vec;

use crate::error::RegisterError;
use crate::schema::ClassDef;
use crate::schema::PropType;
use crate::schema::Registry;
use crate::schema::dash;
use crate::schema::xui;

static ORPHAN: ClassDef = ClassDef::new("Orphan", Some("NoSuchBase"), &[]);
static DUPLICATE: ClassDef = ClassDef::new("XuiScene", Some("XuiControl"), &[]);

fn full_registry() -> Registry {
    let mut registry = Registry::new();
    xui::register_sound_xaudio(&mut registry).expect("XAudio group registers");
    xui::register_html(&mut registry).expect("HTML group registers");
    dash::register(&mut registry).expect("dash classes register");

    registry
}

/// Every class's level chain starts at `XuiElement`, ends at the class and
/// only visits classes registered before it.
#[test]
fn level_chains_are_rooted_and_ordered() {
    let registry = full_registry();

    for class in registry.classes() {
        let levels: Vec<_> = class.levels().map(|level| level.id()).collect();
        let root = levels.first().and_then(|id| registry.class(*id));

        assert_eq!(
            root.map(|root| root.name()),
            Some("XuiElement"),
            "{}",
            class.name()
        );
        assert_eq!(levels.last(), Some(&class.id()), "{}", class.name());
        assert!(
            levels.windows(2).all(|pair| pair[0] < pair[1]),
            "{}",
            class.name()
        );
    }
}

#[test]
fn registration_rules() {
    let mut registry = Registry::new();

    assert_eq!(
        [registry.register(&ORPHAN), registry.register(&DUPLICATE)],
        [
            Err(RegisterError::BaseNotRegistered {
                name: "Orphan",
                base: "NoSuchBase",
            }),
            Err(RegisterError::AlreadyRegistered { name: "XuiScene" }),
        ]
    );
}

#[test]
fn lookup_is_case_sensitive_and_ancestors_resolve() {
    let registry = Registry::new();
    let tab_scene = registry
        .find("XuiTabScene")
        .and_then(|id| registry.class(id))
        .expect("built-in");
    let control = registry.find("XuiControl").expect("built-in");

    assert_eq!(registry.find("xuitabscene"), None);
    assert_eq!(tab_scene.ancestor(2).map(|class| class.id()), Some(control));
    assert!(tab_scene.ancestor(4).is_none());
    assert!(tab_scene.is_a(control));
}

/// Distinct classes declare look-alike definitions; identity is by address.
#[test]
fn definitions_compare_by_address() {
    let element_id = &xui::XUI_ELEMENT.props[0];
    let effect_id = &xui::XUI_EFFECT.props[0];

    assert_eq!((element_id.name, effect_id.name), ("Id", "Id"));
    assert!(matches!(element_id.ty, PropType::String));
    assert!(!core::ptr::eq(element_id, effect_id));
}

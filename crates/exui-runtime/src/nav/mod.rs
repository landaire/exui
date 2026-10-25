//! Scene creation, transitions and scene navigation
//! (doc/xui-timelines.md section 7).

mod localize;

use alloc::format;
use alloc::rc::Rc;
use alloc::string::String;
use alloc::vec::Vec;

use exui_core::Document;
use exui_core::schema::PropDef;
use exui_core::value::Vector3;
use exui_core::xur::Value;

use crate::control::Builtin;
use crate::control::ClassState;
use crate::control::SavedProperty;
use crate::control::props;
use crate::error::NavError;
use crate::error::TransitionError;
use crate::message::Body;
use crate::message::Context;
use crate::message::FocusUser;
use crate::message::Message;
use crate::message::ResourceMask;
use crate::message::TransitionAction;
use crate::message::TransitionKind;
use crate::resource;
use crate::scene;
use crate::scene::BasePathId;
use crate::scene::ElementId;
use crate::scene::LoadError;
use crate::scene::PropValue;
use crate::skin::Created;
use crate::timeline;
use crate::timeline::Frame;
use crate::timeline::PlayEnd;
use crate::timeline::PlayRange;
use crate::timeline::build_timeline_data;

/// User argument of the navigate functions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavUser {
    Focus(FocusUser),
    /// 0xfe: do not set focus.
    NoFocus,
    /// 0xfd: the user of the focus owner of the source scene, else 0xff.
    FromSource,
}

/// Whether the navigation happened (S_OK) or was cancelled by the forward
/// query (S_FALSE).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NavOutcome {
    Navigated,
    Cancelled,
}

/// Pivot and size of a transition visual record, read when the record has
/// properties.
#[derive(Clone, Copy, Debug)]
struct TransitionVisual {
    pivot: Option<Vector3>,
    width: f32,
    height: f32,
}

fn transition_names(kind: TransitionKind) -> &'static str {
    match kind {
        TransitionKind::To => "TransTo",
        TransitionKind::From => "TransFrom",
        TransitionKind::BackTo => "TransBackTo",
        TransitionKind::BackFrom => "TransBackFrom",
    }
}

fn transition_property(kind: TransitionKind) -> &'static PropDef {
    match kind {
        TransitionKind::To => props::trans_to(),
        TransitionKind::From => props::trans_from(),
        TransitionKind::BackTo => props::trans_back_to(),
        TransitionKind::BackFrom => props::trans_back_from(),
    }
}

impl Context<'_, '_> {
    /// `XuiSceneCreate` 0x92153048: loads a scene file relative to `base`
    /// with timelines autoplaying, takes the first child of the file's root
    /// that has a XuiScene level, discards the root and runs the init pass
    /// on the detached scene.
    pub fn create_scene(
        &mut self,
        base: Option<BasePathId>,
        path: &[u16],
    ) -> Result<ElementId, NavError> {
        self.create_scene_with(base, path, |_, _| {})
    }

    /// [`Self::create_scene`] that runs `prepare` on the detached scene
    /// before the init pass, where `XuiSceneCreate` gives the scene its
    /// init data.
    pub fn create_scene_with(
        &mut self,
        base: Option<BasePathId>,
        path: &[u16],
        prepare: impl FnOnce(&mut Self, ElementId),
    ) -> Result<ElementId, NavError> {
        let base_units: Option<Vec<u16>> = base.map(|base| self.tree.base_path_units(base).into());
        let full = resource::join(path, base_units.as_deref())?;
        let bytes = self.resources.read_path(&full, None)?;
        let document = Document::parse(&bytes, self.registry)?;

        let root = self.instantiate_document(&document, base)?;
        let load_path = self.tree.add_base_path(&full);
        self.localize_loaded(root, load_path);
        let children: Vec<ElementId> = self.tree.children(root).collect();
        let Some(scene) = children
            .into_iter()
            .find(|child| self.is_builtin_a(*child, Builtin::Scene))
        else {
            self.destroy(root);
            return Err(NavError::NoScene);
        };

        // Raised load depth: the root sees no child-removed message.
        self.runtime.load_depth += 1;
        self.unlink(scene);
        self.destroy(root);
        self.runtime.load_depth -= 1;

        // `XuiElementSetBasePath` 0x9214F350.
        let _ = self.tree.set_load_path(scene, Some(load_path));

        prepare(self, scene);
        self.init_element_tree(scene);

        Ok(scene)
    }

    /// `XuiLoadFromBinary` with init (0x9214ab00): loads a file relative to
    /// `base` and returns its root element, detached, with timelines
    /// autoplaying and the init pass run.
    pub fn load_document(
        &mut self,
        base: Option<BasePathId>,
        path: &[u16],
    ) -> Result<ElementId, LoadError> {
        let base_units: Option<Vec<u16>> = base.map(|base| self.tree.base_path_units(base).into());
        let full = resource::join(path, base_units.as_deref())?;
        let bytes = self.resources.read_path(&full, None)?;
        let document = Document::parse(&bytes, self.registry)?;

        let root = self.instantiate_document(&document, base)?;
        let load_path = self.tree.add_base_path(&full);
        self.localize_loaded(root, load_path);
        self.init_element_tree(root);

        Ok(root)
    }

    /// Instantiates a whole document detached, building timelines and
    /// autoplaying them, without the init pass.
    pub fn instantiate_document(
        &mut self,
        document: &Document<'_>,
        base: Option<BasePathId>,
    ) -> Result<ElementId, LoadError> {
        let mut created: Vec<Created> = Vec::new();
        self.runtime.load_depth += 1;
        let result = scene::instantiate(
            self.tree,
            self.registry,
            document,
            document.root(),
            base,
            &mut |_, element, object| {
                created.push(Created {
                    element,
                    timeline: build_timeline_data(document, object).map(Rc::new),
                });
            },
        );
        self.runtime.load_depth -= 1;
        let root = result?;

        self.finish_instantiation(created, true);

        Ok(root)
    }

    fn resolve_nav_user(&self, source: ElementId, user: NavUser) -> Option<FocusUser> {
        match user {
            NavUser::Focus(user) => Some(user),
            NavUser::NoFocus => None,
            NavUser::FromSource => Some(
                self.focus_owner(source)
                    .and_then(|owner| self.focus_user(owner))
                    .unwrap_or(FocusUser::Shared),
            ),
        }
    }

    fn scene_in_transition(&mut self, scene: ElementId) -> bool {
        self.is_builtin_a(scene, Builtin::Scene)
            && self
                .scene_state(scene)
                .is_some_and(|state| state.in_transition())
    }

    /// `XuiScene_InterruptTransitions` 0x92150668.
    pub fn interrupt_transitions(&mut self, scene: ElementId) {
        let partner = self.scene_state(scene).and_then(|state| state.partner);
        let other = match partner {
            Some(partner) => Some(partner),
            None => self.back_scene(scene),
        };
        if let Some(other) = other
            && self.scene_in_transition(other)
        {
            self.send_body(other, Body::InterruptTransition);
        }

        if self.scene_in_transition(scene) {
            self.send_body(scene, Body::InterruptTransition);
        }
    }

    /// Scene 0x2a, `XuiScene_OnInterruptTransition` 0x9215b618.
    pub(crate) fn on_interrupt_transition(&mut self, scene: ElementId) {
        let Some(state) = self.scene_state(scene) else {
            return;
        };
        let (own, wrapper) = (state.own_transition, state.wrapper_transition);

        if own {
            timeline::finish_now(self, scene);
        }

        if wrapper
            && let Some(parent) = self.tree.parent(scene)
            && self.is_builtin_a(parent, Builtin::Transition)
        {
            timeline::finish_now(self, parent);
            self.stop_timeline(scene, true);
        }
    }

    /// `XuiSceneNavigateForward` 0x92153590.
    pub fn navigate_forward(
        &mut self,
        current: ElementId,
        stay_visible: bool,
        next: ElementId,
        user: NavUser,
    ) -> Result<NavOutcome, NavError> {
        let user = self.resolve_nav_user(current, user);

        let query = self.send_body(current, Body::NavigateForwardQuery { next: Some(next) });
        let Body::NavigateForwardQuery { next: Some(next) } = query.body else {
            return Ok(NavOutcome::Cancelled);
        };

        // The link is overwritten even when this fails.
        if self.set_back_scene(next, Some(current)).is_some() {
            return Err(NavError::HasBackScene);
        }

        self.interrupt_transitions(current);

        if self.tree.parent(next).is_none() {
            let parent = self.tree.parent(current).ok_or(NavError::NoParent)?;
            let before = self.tree.next_sibling(current);
            self.insert_child(parent, next, before)?;
        }

        self.broadcast(next, &Body::CreateResources);
        if let Some(user) = user {
            let _ = self.init_focus(next, user, true);
        }

        self.set_input_enabled(current, false);
        self.set_input_enabled(next, false);
        if let Some(state) = self.scene_state(current) {
            state.partner = Some(next);
        }

        let action = if stay_visible {
            TransitionAction::Keep
        } else {
            TransitionAction::Remove
        };
        let _ = self.start_transition(current, TransitionKind::From, action);
        let _ = self.start_transition(next, TransitionKind::To, TransitionAction::Keep);

        Ok(NavOutcome::Navigated)
    }

    /// `XuiSceneNavigateBack` 0x92153780.
    pub fn navigate_back(
        &mut self,
        current: ElementId,
        target: ElementId,
        user: NavUser,
    ) -> Result<(), NavError> {
        let user = self.resolve_nav_user(current, user);

        let mut back = self.back_scene(current);
        loop {
            match back {
                Some(scene) if scene == target => break,
                Some(scene) => back = self.back_scene(scene),
                None => return Err(NavError::NotOnBackChain),
            }
        }

        self.interrupt_transitions(current);

        if self.back_scene(current) != Some(target) {
            let mut skipped = self.set_back_scene(current, Some(target));
            while let Some(scene) = skipped {
                if scene == target {
                    break;
                }
                let next = self.set_back_scene(scene, None);
                self.destroy(scene);
                skipped = next;
            }
        }

        if self.tree.parent(target).is_none() {
            let parent = self.tree.parent(current).ok_or(NavError::NoParent)?;
            self.insert_child(parent, target, Some(current))?;
            self.broadcast(target, &Body::CreateResources);
        }

        self.relocalize_back_target(current, target);

        self.send_body(target, Body::NavigatedBack { from: current });
        if let Some(user) = user {
            let _ = self.init_focus(target, user, true);
        }

        self.set_input_enabled(current, false);
        self.set_input_enabled(target, false);
        if let Some(state) = self.scene_state(target) {
            state.partner = Some(current);
        }

        let _ = self.start_transition(current, TransitionKind::BackFrom, TransitionAction::Destroy);
        let _ = self.start_transition(target, TransitionKind::BackTo, TransitionAction::Keep);

        Ok(())
    }

    /// `XuiSceneNavigateFirst` 0x92153450.
    pub fn navigate_first(
        &mut self,
        parent: ElementId,
        scene: ElementId,
        user: NavUser,
    ) -> Result<(), NavError> {
        let user = self.resolve_nav_user(parent, user);

        if self.set_back_scene(scene, None).is_some() {
            return Err(NavError::HasBackScene);
        }

        if self.tree.parent(scene).is_none() {
            self.insert_child(parent, scene, None)?;
        }

        self.broadcast(scene, &Body::CreateResources);
        if let Some(user) = user {
            let _ = self.init_focus(scene, user, true);
        }
        self.set_input_enabled(scene, false);
        let _ = self.start_transition(scene, TransitionKind::To, TransitionAction::Keep);

        Ok(())
    }

    /// `XuiScene_StartTransTo`: plays the TransTo transition of `scene`,
    /// which keeps it afterwards. Returns whether it ended at once.
    pub fn start_trans_to(&mut self, scene: ElementId) -> bool {
        self.start_transition(scene, TransitionKind::To, TransitionAction::Keep)
            .unwrap_or(false)
    }

    /// `XuiScene_StartTransBackTo` 0x921531A0: plays the TransBackTo
    /// transition of `scene`, which keeps it afterwards. Returns whether it
    /// ended at once.
    pub fn start_trans_back_to(&mut self, scene: ElementId) -> bool {
        self.start_transition(scene, TransitionKind::BackTo, TransitionAction::Keep)
            .unwrap_or(false)
    }

    /// `XuiScene_StartTransition` 0x92152730. Returns whether the
    /// transition ended at once.
    fn start_transition(
        &mut self,
        scene: ElementId,
        kind: TransitionKind,
        action: TransitionAction,
    ) -> Result<bool, TransitionError> {
        let Some(parent) = self.tree.parent(scene) else {
            return Err(TransitionError::NoParent);
        };
        if self.is_builtin_a(parent, Builtin::Transition) {
            return Err(TransitionError::InTransition);
        }

        let message = self.send_body(
            scene,
            Body::StartTransition {
                kind,
                deferred: false,
                action,
            },
        );
        let (deferred, action) = match message.body {
            Body::StartTransition {
                deferred, action, ..
            } => (message.handled && deferred, action),
            _ => (false, action),
        };
        if deferred && let Some(state) = self.scene_state(scene) {
            state.own_transition = true;
            state.pending = Some((kind, action));
        }

        // [INFERENCE] The transition visual is looked up with the scene's
        // base path in the prefix argument, so in practice by plain name.
        let name: Option<Vec<u16>> = self
            .tree
            .string(scene, &[transition_property(kind)])
            .filter(|name| !name.is_empty())
            .map(Into::into);
        let base: Option<Vec<u16>> = self
            .tree
            .base_path(scene)
            .map(|base| self.tree.base_path_units(base).into());
        let object = name.and_then(|name| self.runtime.skins.lookup(&name, base.as_deref()));
        let object = object.filter(|object| {
            self.runtime
                .skins
                .object_ref(*object)
                .is_some_and(|object| !object.timelines().is_empty())
        });

        let Some(object) = object else {
            let own = self
                .scene_state(scene)
                .is_some_and(|state| state.own_transition);
            if !own {
                self.on_transition_end(scene, kind, action);
                return Ok(true);
            }
            return Ok(false);
        };

        self.start_wrapper_transition(scene, parent, object, kind, action);
        Ok(false)
    }

    /// The XuiTransition part of `XuiScene_StartTransition`.
    fn start_wrapper_transition(
        &mut self,
        scene: ElementId,
        parent: ElementId,
        object: crate::skin::SkinObject,
        kind: TransitionKind,
        action: TransitionAction,
    ) {
        // Defaults of the transition record reader (doc/xui-timelines.md 7).
        const DEFAULT_TRANSITION_WIDTH: f32 = 60.0;
        const DEFAULT_TRANSITION_HEIGHT: f32 = 40.0;

        let Some(class) = self
            .registry
            .find("XuiTransition")
            .and_then(|class| self.registry.class(class))
        else {
            return;
        };

        let (data, paths, visual) = {
            let Some(document) = self.runtime.skins.document(object.skin) else {
                return;
            };
            let Some(record) = document.object(object.object) else {
                return;
            };
            let Some(first) = record.timelines().first() else {
                return;
            };
            let data = timeline::build_single_track(document, first);
            let paths: Vec<(Vec<&'static PropDef>, Option<u32>)> = data.tracks[0]
                .paths
                .iter()
                .map(|path| (path.defs.to_vec(), path.index))
                .collect();

            let read = |name: &str| {
                record
                    .properties()
                    .iter()
                    .find(|property| property.def().name == name)
                    .and_then(|property| document.value(property).copied())
            };
            let has_properties = !record.properties().is_empty();
            let pivot = match read("Pivot") {
                Some(Value::Vector(index)) => document.vector(index),
                _ => None,
            };
            let width = match read("Width") {
                Some(Value::Float(width)) => width,
                _ => DEFAULT_TRANSITION_WIDTH,
            };
            let height = match read("Height") {
                Some(Value::Float(height)) => height,
                _ => DEFAULT_TRANSITION_HEIGHT,
            };
            (
                data,
                paths,
                has_properties.then_some(TransitionVisual {
                    pivot,
                    width,
                    height,
                }),
            )
        };

        let transition = self.tree.create(class);
        if let Some(state) = self.transition_state(transition) {
            state.kind = kind;
            state.action = action;
        }

        if action != TransitionAction::Destroy {
            self.save_scene_props(transition, scene, &paths);
        }

        if let Some(TransitionVisual {
            pivot,
            width: visual_w,
            height: visual_h,
        }) = visual
        {
            let mut pivot = pivot.unwrap_or(Vector3::new(0.0, 0.0, 0.0));
            let size = self
                .tree
                .get(scene)
                .map(|scene| scene.size())
                .unwrap_or_default();
            if visual_w > 0.01 {
                pivot.x = pivot.x / visual_w * size.width;
            }
            if visual_h > 0.01 {
                pivot.y = pivot.y / visual_h * size.height;
            }
            let _ =
                self.tree
                    .set_property(scene, &[props::pivot()], None, PropValue::Vector(pivot));
        }

        let before = Some(scene);
        let _ = self.insert_child(parent, transition, before);
        self.unlink(scene);
        let _ = self.insert_child(transition, scene, None);

        let data = Rc::new(data);
        let length = {
            let timeline = self
                .runtime
                .timelines
                .create(transition, data, |_| Some(scene));
            timeline.length()
        };
        self.play_timeline(
            transition,
            PlayRange {
                start: Frame(0),
                initial: Frame(0),
                end: PlayEnd::from_frame(Frame(length as i32)),
                looping: true,
            },
            false,
        );
        timeline::apply_at_frame(self, transition, Frame(0));

        if let Some(state) = self.scene_state(scene) {
            state.wrapper_transition = true;
        }
    }

    /// `XuiTransition_SaveSceneProps`: pivot and every animated property.
    fn save_scene_props(
        &mut self,
        transition: ElementId,
        scene: ElementId,
        paths: &[(Vec<&'static PropDef>, Option<u32>)],
    ) {
        let mut saved = Vec::new();
        let pivot = [props::pivot()];
        for (path, index) in core::iter::once((pivot.to_vec(), None)).chain(paths.iter().cloned()) {
            if let Some(value) = self.tree.property(scene, &path, index) {
                saved.push(SavedProperty { path, index, value });
            }
        }

        if let Some(state) = self.transition_state(transition) {
            state.saved = saved;
        }
    }

    /// `XuiTransition_RestoreSceneProps`.
    fn restore_scene_props(&mut self, transition: ElementId, scene: ElementId) {
        let saved = self
            .transition_state(transition)
            .map(|state| state.saved.clone())
            .unwrap_or_default();
        for SavedProperty { path, index, value } in saved {
            let _ = self.set_property(scene, &path, index, value);
        }
    }

    /// XuiTransition on its own 0xf.
    pub(crate) fn on_transition_wrapper_end(&mut self, transition: ElementId) {
        let Some(parent) = self.tree.parent(transition) else {
            return;
        };
        let Some(scene) = self.tree.first_child(transition) else {
            return;
        };

        let own = self
            .scene_state(scene)
            .is_some_and(|state| state.own_transition);
        let (kind, action) = self
            .transition_state(transition)
            .map_or((TransitionKind::To, TransitionAction::Keep), |state| {
                (state.kind, state.action)
            });

        if own || action == TransitionAction::Keep {
            self.runtime.load_depth += 1;
            self.unlink(scene);
            let before = self.tree.next_sibling(transition);
            let _ = self.insert_child(parent, scene, before);
            self.runtime.load_depth -= 1;
            self.restore_scene_props(transition, scene);
        }

        if let Some(state) = self.scene_state(scene) {
            state.wrapper_transition = false;
        }

        // Restoring twice for a kept scene matches the original.
        if !own {
            self.restore_scene_props(transition, scene);
            self.on_transition_end(scene, kind, action);
        }

        self.destroy(transition);
    }

    /// Scene 0x1d, `XuiScene_OnStartTransition` 0x921692e8: plays the
    /// scene's own named-frame transition.
    pub(crate) fn on_start_transition(
        &mut self,
        scene: ElementId,
        kind: TransitionKind,
        message: &mut Message,
    ) {
        let base = transition_names(kind);
        let index = self.scene_state(scene).map_or(0, |state| match kind {
            TransitionKind::To | TransitionKind::BackFrom => state.dest_index,
            TransitionKind::From | TransitionKind::BackTo => state.src_index,
        });

        let find = |cx: &Self, suffix: &str| -> Option<(u32, u32)> {
            let start: String = format!("{base}{suffix}");
            let end: String = format!("End{base}{suffix}");
            Some((
                cx.find_named_frame(scene, &start)?,
                cx.find_named_frame(scene, &end)?,
            ))
        };
        let frames = if index != 0 {
            find(self, &format!("{index}")).or_else(|| find(self, ""))
        } else {
            find(self, "")
        };

        match frames {
            Some((start, end)) => {
                message.handled = true;
                let start = Frame(start as i32);
                self.play_timeline(
                    scene,
                    PlayRange {
                        start,
                        initial: start,
                        end: PlayEnd::from_frame(Frame(end as i32)),
                        looping: true,
                    },
                    false,
                );
                if let Body::StartTransition { deferred, .. } = &mut message.body {
                    *deferred = true;
                }
            }
            None => {
                // Scenes without transition frames play their whole
                // timeline from 0 with no end.
                self.play_timeline(
                    scene,
                    PlayRange {
                        start: Frame(0),
                        initial: Frame(0),
                        end: PlayEnd::Length,
                        looping: true,
                    },
                    false,
                );
            }
        }
    }

    /// `XuiScene_OnTransitionEnd` 0x92151438.
    pub(crate) fn on_transition_end(
        &mut self,
        scene: ElementId,
        kind: TransitionKind,
        action: TransitionAction,
    ) {
        if matches!(action, TransitionAction::Remove | TransitionAction::Destroy) {
            let message = self.send_body(
                scene,
                Body::BeforeUnlink {
                    flags: ResourceMask::ALL,
                },
            );
            if let Body::BeforeUnlink { flags } = message.body
                && flags != ResourceMask::NONE
            {
                self.free_resources(Some(scene), flags);
            }
            self.unlink(scene);
        }

        let partner = self.scene_state(scene).and_then(|state| state.partner);
        let back = self.back_scene(scene);
        let other = partner.or(back);
        let other_busy = other.is_some_and(|other| self.scene_in_transition(other));
        if !other_busy {
            self.set_input_enabled(scene, true);
            if let Some(partner) = partner {
                self.set_input_enabled(partner, true);
                if let Some(state) = self.scene_state(scene) {
                    state.partner = None;
                }
            } else if let Some(back) = back {
                self.set_input_enabled(back, true);
                if let Some(state) = self.scene_state(back) {
                    state.partner = None;
                }
            }
        }

        let message = self.send_body(scene, Body::TransitionEnded { kind, action });
        if kind == TransitionKind::BackFrom {
            self.set_back_scene(scene, None);
        }
        if let Body::TransitionEnded {
            action: TransitionAction::Destroy,
            ..
        } = message.body
        {
            self.destroy(scene);
        }
    }

    /// Which transition, if any, is pending on a scene.
    pub fn scene_transition(
        &mut self,
        scene: ElementId,
    ) -> Option<(TransitionKind, TransitionAction)> {
        match self.class_state(scene) {
            Some(ClassState::Transition(state)) => Some((state.kind, state.action)),
            _ => self.scene_state(scene).and_then(|state| state.pending),
        }
    }
}

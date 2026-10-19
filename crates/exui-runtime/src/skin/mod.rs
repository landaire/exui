//! Skin registry, visual lookup and control visual creation
//! (xui-controls.md "Visual and skin resolution").

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::rc::Rc;
use alloc::vec::Vec;

use exui_core::Document;
use exui_core::Registry;
use exui_core::schema::ClassRef;
use exui_core::schema::ScalarType;
use exui_core::xur::ObjectId;
use exui_core::xur::ObjectRef;
use exui_core::xur::Value;

use crate::control::Builtin;
use crate::error::SkinError;
use crate::error::VisualError;
use crate::message::Body;
use crate::message::Context;
use crate::scene;
use crate::scene::BasePathId;
use crate::scene::ElementFlags;
use crate::scene::ElementId;
use crate::timeline::TimelineData;
use crate::timeline::build_timeline_data;

/// Index of a loaded skin file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SkinIndex(u32);

/// A loaded object record of a skin: a registered visual or one of its
/// descendants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SkinObject {
    pub skin: SkinIndex,
    pub object: ObjectId,
}

struct Skin<'p> {
    document: Document<'p>,
    /// Skin file path; visuals resolve relative paths against it.
    base: BasePathId,
}

/// The visual map and the skin source records.
#[derive(Default)]
pub struct SkinRegistry<'p> {
    skins: Vec<Skin<'p>>,
    /// Key (`prefix || Id`) to visual, in registration order. First
    /// registration wins.
    visuals: Vec<(Box<[u16]>, SkinObject)>,
    /// Skin file of the unprefixed skin.
    base_source: Option<BasePathId>,
    /// Skin file per prefix.
    sources: Vec<(Box<[u16]>, BasePathId)>,
    /// Instantiation guard (loaded object flag bit 0).
    in_progress: Vec<SkinObject>,
    /// Timeline data shared by every instance of a skin object.
    timelines: BTreeMap<SkinObject, Option<Rc<TimelineData>>>,
}

impl core::fmt::Debug for SkinRegistry<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SkinRegistry")
            .field("skins", &self.skins.len())
            .field("visuals", &self.visuals.len())
            .finish()
    }
}

/// `Id` of a loaded object: the first entry named Id of type String.
fn object_id(document: &Document<'_>, object: ObjectRef<'_, '_>) -> Option<Box<[u16]>> {
    let property = object.properties().iter().find(|property| {
        property.def().name == "Id" && property.def().ty.scalar() == Some(ScalarType::String)
    })?;
    let Some(Value::String(Some(id))) = document.value(property) else {
        return None;
    };

    Some(document.string(*id)?.units().collect())
}

impl<'p> SkinRegistry<'p> {
    pub fn new() -> Self {
        Self::default()
    }

    /// `XuiLoadVisualFromBinary` 0x92154050: registers every top-level
    /// object with an Id under `prefix || Id`.
    pub fn load(
        &mut self,
        registry: &Registry,
        bytes: &'p [u8],
        base: BasePathId,
        prefix: Option<&[u16]>,
    ) -> Result<SkinIndex, SkinError> {
        let document = Document::parse(bytes, registry)?;
        let index = SkinIndex(self.skins.len() as u32);

        match prefix {
            None => self.base_source = Some(base),
            Some(prefix) => match self.sources.iter_mut().find(|(key, _)| **key == *prefix) {
                Some((_, source)) => *source = base,
                None => self.sources.push((prefix.into(), base)),
            },
        }

        let mut keys = Vec::new();
        for child in document.root().children() {
            let Some(id) = object_id(&document, child) else {
                continue;
            };
            let mut key: Vec<u16> = prefix.map(|prefix| prefix.to_vec()).unwrap_or_default();
            key.extend_from_slice(&id);
            keys.push((key.into_boxed_slice(), child.id()));
        }

        self.skins.push(Skin { document, base });

        for (key, object) in keys {
            if self.visuals.iter().any(|(existing, _)| *existing == key) {
                return Err(SkinError::Duplicate { key });
            }
            self.visuals.push((
                key,
                SkinObject {
                    skin: index,
                    object,
                },
            ));
        }

        Ok(index)
    }

    /// `XuiFreeVisuals`: all visuals (sources kept), or every key starting
    /// with `prefix` and that prefix's source.
    pub fn free(&mut self, prefix: Option<&[u16]>) {
        match prefix {
            None => self.visuals.clear(),
            Some(prefix) => {
                self.visuals
                    .retain(|(key, _)| key.is_empty() || !key.starts_with(prefix));
                self.sources.retain(|(key, _)| **key != *prefix);
            }
        }
    }

    /// `XuiLookupLoadedObject` 0x92150f70: `prefix || name`, then `name`.
    /// Returns the object and whether the prefixed key matched.
    pub fn lookup_scoped(
        &self,
        name: &[u16],
        prefix: Option<&[u16]>,
    ) -> Option<(SkinObject, bool)> {
        if let Some(prefix) = prefix {
            let found = self.visuals.iter().find(|(key, _)| {
                key.len() == prefix.len() + name.len()
                    && key.starts_with(prefix)
                    && key.ends_with(name)
            });
            if let Some((_, object)) = found {
                return Some((*object, true));
            }
        }

        self.visuals
            .iter()
            .find(|(key, _)| **key == *name)
            .map(|(_, object)| (*object, false))
    }

    /// `XuiSkin_LookupVisual`.
    pub fn lookup(&self, name: &[u16], prefix: Option<&[u16]>) -> Option<SkinObject> {
        self.lookup_scoped(name, prefix).map(|(object, _)| object)
    }

    /// `XuiSkin_GetSourcePath`.
    pub fn source(&self, prefix: Option<&[u16]>) -> Option<BasePathId> {
        match prefix {
            None => self.base_source,
            Some(prefix) => self
                .sources
                .iter()
                .find(|(key, _)| **key == *prefix)
                .map(|(_, base)| *base),
        }
    }

    pub fn document(&self, skin: SkinIndex) -> Option<&Document<'p>> {
        self.skins.get(skin.0 as usize).map(|skin| &skin.document)
    }

    pub fn object_ref(&self, object: SkinObject) -> Option<ObjectRef<'_, 'p>> {
        self.document(object.skin)?.object(object.object)
    }

    /// First direct child record whose class satisfies `accept`.
    pub fn find_child_record(
        &self,
        registry: &Registry,
        parent: SkinObject,
        accept: impl Fn(ClassRef<'_>) -> bool,
    ) -> Option<SkinObject> {
        let object = self.object_ref(parent)?;
        let child = object
            .children()
            .find(|child| registry.class(child.class()).is_some_and(&accept))?;

        Some(SkinObject {
            skin: parent.skin,
            object: child.id(),
        })
    }
}

/// Elements created by an instantiation, with their objects' timeline
/// data, for the work done after the tree exists.
pub(crate) struct Created {
    pub element: ElementId,
    pub timeline: Option<Rc<TimelineData>>,
}

impl Context<'_, '_> {
    /// Timelines, autoplay and property side effects for freshly
    /// instantiated elements (the end of `XuiInstantiateObjectTree`).
    pub(crate) fn finish_instantiation(&mut self, created: Vec<Created>, autoplay: bool) {
        // Attaching each loaded child sends 0x4 to its parent (0x921643a8),
        // which assigns scene child indexes and registers unfocused-input
        // controls. Handlers see the child as the last one at that point,
        // so the attaches of each fresh child list are replayed in order.
        for Created {
            element: parent, ..
        } in &created
        {
            let children: Vec<ElementId> = self.tree.children(*parent).collect();
            for child in &children {
                let _ = self.tree.unlink(*child);
            }
            for child in children {
                if self.tree.insert_child(*parent, child, None).is_ok() {
                    self.send_body(*parent, Body::ChildAdded { child });
                }
            }
        }

        for Created { element, timeline } in created {
            if !autoplay && let Some(mut flags) = self.tree.flags(element) {
                flags.set(ElementFlags::AUTOPLAY_DISABLED, true);
                let _ = self.tree.set_flags(element, flags);
            }

            if let Some(data) = timeline {
                let tree = &*self.tree;
                let timeline = self.runtime.timelines.create(element, data, |id| {
                    id.and_then(|id| tree.child_by_id(element, id))
                });
                if autoplay {
                    timeline.play(
                        crate::timeline::PlayRange {
                            start: crate::timeline::Frame(0),
                            initial: crate::timeline::Frame(0),
                            end: crate::timeline::PlayEnd::Length,
                            looping: true,
                        },
                        true,
                    );
                }
            }

            // Setters that the loader bypasses but whose effects the
            // original runs while applying loaded properties.
            if self.is_builtin_a(element, Builtin::SoundXAudio)
                && autoplay
                && self
                    .tree
                    .string(element, &[crate::control::props::sound_file()])
                    .is_some()
            {
                self.schedule_sound_apply(element);
            }
            if self.is_builtin_a(element, Builtin::CommonList) {
                let has_items = [
                    crate::control::props::items_text(),
                    crate::control::props::items_image(),
                    crate::control::props::items_nav_path(),
                ]
                .into_iter()
                .any(|def| self.tree.string(element, &[def]).is_some());
                if has_items {
                    self.common_list_apply_item_props(element);
                }
            }
        }
    }

    /// Instantiates a skin object (`XuiCreateFromLoadedObject` without
    /// init). Timeline data is shared between instances.
    pub fn instantiate_skin_object(
        &mut self,
        object: SkinObject,
        autoplay: bool,
    ) -> Result<ElementId, VisualError> {
        self.runtime.load_depth += 1;
        let mut visited: Vec<(ElementId, ObjectId)> = Vec::new();
        let result = {
            let skins = &self.runtime.skins;
            match skins.skins.get(object.skin.0 as usize) {
                Some(skin) => match skin.document.object(object.object) {
                    Some(root) => scene::instantiate(
                        self.tree,
                        self.registry,
                        &skin.document,
                        root,
                        Some(skin.base),
                        &mut |_, element, object| visited.push((element, object.id())),
                    )
                    .map_err(VisualError::from),
                    None => Err(VisualError::NotFound),
                },
                None => Err(VisualError::NotFound),
            }
        };
        self.runtime.load_depth -= 1;
        let element = result?;

        let created = visited
            .into_iter()
            .map(|(element, id)| {
                let key = SkinObject {
                    skin: object.skin,
                    object: id,
                };
                let timeline = self.skin_timeline(key);
                Created { element, timeline }
            })
            .collect();
        self.finish_instantiation(created, autoplay);

        Ok(element)
    }

    fn skin_timeline(&mut self, key: SkinObject) -> Option<Rc<TimelineData>> {
        if let Some(cached) = self.runtime.skins.timelines.get(&key) {
            return cached.clone();
        }

        let skins = &self.runtime.skins;
        let data = skins.skins.get(key.skin.0 as usize).and_then(|skin| {
            let object = skin.document.object(key.object)?;
            build_timeline_data(&skin.document, object).map(Rc::new)
        });
        self.runtime.skins.timelines.insert(key, data.clone());

        data
    }

    /// `XuiSkin_CreateVisual` 0x92153058.
    pub fn create_skin_visual(
        &mut self,
        name: &[u16],
        prefix: Option<&[u16]>,
        no_autoplay: bool,
    ) -> Result<ElementId, VisualError> {
        let Some((object, used_prefix)) = self.runtime.skins.lookup_scoped(name, prefix) else {
            return Err(VisualError::NotFound);
        };
        if self.runtime.skins.in_progress.contains(&object) {
            return Err(VisualError::Recursive);
        }

        self.runtime.skins.in_progress.push(object);
        let result = self.instantiate_skin_object(object, !no_autoplay);
        self.runtime
            .skins
            .in_progress
            .retain(|other| *other != object);
        let visual = result?;

        // A visual found without the prefix keeps a null prefix, which hides
        // the scene prefix from the controls inside it.
        if used_prefix {
            self.send_body(
                visual,
                Body::SetSkinPrefix {
                    prefix: prefix.map(Rc::from),
                },
            );
        }

        Ok(visual)
    }

    /// `XuiControl_CreateVisual` 0x9215b2b0: the Visual property, else the
    /// class names of the control's chain, most derived first.
    pub fn create_control_visual(&mut self, control: ElementId) -> Result<ElementId, VisualError> {
        self.destroy_control_visual(control);

        let prefix = self.skin_prefix(control);
        let no_autoplay = self
            .tree
            .flags(control)
            .is_some_and(|flags| flags.contains(ElementFlags::AUTOPLAY_DISABLED));

        let mut visual = None;
        let name: Option<Vec<u16>> = self
            .tree
            .string(control, &[crate::control::props::visual()])
            .filter(|name| !name.is_empty())
            .map(Into::into);
        if let Some(name) = name {
            match self.create_skin_visual(&name, prefix.as_deref(), no_autoplay) {
                Ok(created) => visual = Some(created),
                Err(VisualError::Recursive) => return Err(VisualError::Recursive),
                Err(_) => {}
            }
        }

        if visual.is_none() {
            let mut level = self
                .tree
                .class(control)
                .and_then(|class| self.registry.class(class));
            while let Some(class) = level {
                let name: Vec<u16> = class.name().encode_utf16().collect();
                match self.create_skin_visual(&name, prefix.as_deref(), no_autoplay) {
                    Ok(created) => {
                        visual = Some(created);
                        break;
                    }
                    Err(VisualError::Recursive) => return Err(VisualError::Recursive),
                    Err(_) => {}
                }
                level = class.base();
            }
        }

        let Some(visual) = visual else {
            return Err(VisualError::NotFound);
        };

        // A failed insert leaks the visual, as the original does.
        let first = self.tree.first_child(control);
        self.runtime.load_depth += 1;
        let inserted = self.insert_child(control, visual, first);
        self.runtime.load_depth -= 1;
        inserted?;

        if let Some(state) = self.control_state(control) {
            state.visual_created = true;
        }
        self.init_element_tree(visual);

        Ok(visual)
    }

    /// `XuiControl_DestroyVisual` 0x92157958.
    pub fn destroy_control_visual(&mut self, control: ElementId) {
        if let Some(visual) = self.control_visual(control) {
            self.destroy(visual);
        }
        if let Some(state) = self.control_state(control) {
            state.visual_created = false;
        }
    }

    /// Loads a skin from a mapped resource and registers its visuals.
    pub fn load_visuals(
        &mut self,
        path: &[u16],
        prefix: Option<&[u16]>,
    ) -> Result<SkinIndex, SkinError> {
        let bytes = match self.resources.read_path(path, None)? {
            alloc::borrow::Cow::Borrowed(bytes) => bytes,
            alloc::borrow::Cow::Owned(_) => return Err(SkinError::NotMapped),
        };

        let base = self.tree.add_base_path(path);
        self.runtime.skins.load(self.registry, bytes, base, prefix)
    }

    /// `XuiElementSetSkinPrefix` and `XuiBroadcastRecreateVisuals`: sets the
    /// prefix on `root` and sends 0x9 to the whole tree.
    pub fn apply_skin_prefix(&mut self, root: ElementId, prefix: Option<&[u16]>) {
        self.send_body(
            root,
            Body::SetSkinPrefix {
                prefix: prefix.map(Rc::from),
            },
        );
        self.broadcast(root, &Body::CreateVisual);
    }

    /// Sends 0x0 to every shown control without a visual and every shown
    /// XuiImage with scene content to load, so that both exist before
    /// drawing, as the render message does.
    pub fn prepare_visuals(&mut self, root: ElementId) {
        let mut stack = Vec::new();
        stack.push(root);
        while let Some(element) = stack.pop() {
            if !self.is_shown(element) {
                continue;
            }

            if self.is_builtin_a(element, Builtin::Control)
                && !self
                    .control_state(element)
                    .is_some_and(|state| state.visual_created)
            {
                self.send_body(element, Body::Render);
            }
            if self.is_builtin_a(element, Builtin::Image)
                && self
                    .tree
                    .image_content(element)
                    .is_some_and(|image| image.needs_scene_load())
            {
                self.send_body(element, Body::Render);
            }

            let mut children: Vec<ElementId> = self.tree.children(element).collect();
            children.reverse();
            stack.extend(children);
        }
    }
}

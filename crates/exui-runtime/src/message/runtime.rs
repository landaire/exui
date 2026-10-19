//! Behavior state of the element tree: class behaviors, per-element class
//! instances, timelines, focus, timers and skins.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::collections::BTreeSet;
use alloc::rc::Rc;
use alloc::vec::Vec;
use core::any::Any;

use exui_core::Registry;
use exui_core::schema::ClassId;

use crate::control::Builtin;
use crate::control::ElementState;
use crate::error::RuntimeError;
use crate::focus::FocusSlots;
use crate::message::ClassBehavior;
use crate::message::MessageHook;
use crate::message::Timers;
use crate::resource::TextureObject;
use crate::resource::TextureReleases;
use crate::scene::ElementId;
use crate::scene::ElementTree;
use crate::skin::SkinRegistry;
use crate::text::FontLibrary;
use crate::text::TextRenderer;
use crate::timeline::Frame;
use crate::timeline::PlayEnd;
use crate::timeline::Timelines;

/// Sound requests of XuiSoundXAudio for the platform audio layer.
#[derive(Clone, Debug, PartialEq)]
pub enum SoundEvent {
    /// Start a voice for the element, replacing any voice it has.
    Start {
        element: ElementId,
        /// Path as authored; resolve with `base`.
        file: Rc<[u16]>,
        base: Option<crate::scene::BasePathId>,
        /// Linear amplitude, 0.0 when muted.
        amplitude: f32,
    },
    /// Stop the element's voice.
    Stop { element: ElementId },
    /// Change the element's voice volume.
    Volume { element: ElementId, amplitude: f32 },
}

/// Platform audio hook. No decoding happens in the runtime.
pub trait SoundSink {
    fn event(&mut self, event: SoundEvent);

    /// Whether the element's voice has finished or does not exist.
    fn is_done(&self, element: ElementId) -> bool;
}

/// Events reported to an optional observer, for logging.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceEvent {
    /// A timeline was played with named frames resolved.
    Play {
        element: ElementId,
        start: Frame,
        end: PlayEnd,
    },
}

pub struct Runtime<'p> {
    builtins: Vec<Option<Builtin>>,
    behaviors: BTreeMap<ClassId, Rc<dyn ClassBehavior>>,
    pub(crate) states: BTreeMap<ElementId, ElementState>,
    instances: BTreeMap<(ElementId, ClassId), Box<dyn Any>>,

    pub(crate) timelines: Timelines,
    pub(crate) focus: FocusSlots,
    pub(crate) timers: Timers,
    pub(crate) skins: SkinRegistry<'p>,
    pub(crate) sound: Option<Box<dyn SoundSink + 'p>>,
    pub(crate) trace: Option<Box<dyn FnMut(TraceEvent) + 'p>>,
    pub(crate) hook: Option<Rc<dyn MessageHook>>,

    /// Fonts and the text renderer, here so handlers can measure text.
    pub fonts: FontLibrary,
    pub(crate) text: TextRenderer,

    /// Global 0x92809cc8 bit 0: keyboard press and navigation keys off.
    pub keyboard_keys_disabled: bool,
    /// `XuiSoundSetGlobalMute`.
    pub global_mute: bool,
    pub(crate) init_focus_depth: u32,
    pub(crate) load_depth: u32,
    pub(crate) dispatching: Vec<ElementId>,
    /// Elements sent 0xa by a destroy whose free has not completed.
    pub(crate) destroyed: BTreeSet<ElementId>,
    /// Roots of destroyed subtrees freed once no element of them is in
    /// dispatch.
    pub(crate) pending_free: Vec<ElementId>,
    pub(crate) texture_releases: TextureReleases,
}

impl<'p> Runtime<'p> {
    /// Built-in behaviors attach to the XUI class names present in
    /// `registry`.
    pub fn new(registry: &Registry) -> Self {
        let mut builtins = alloc::vec![None; registry.len()];
        for class in registry.classes() {
            builtins[class.id().index()] = Builtin::from_name(class.name());
        }

        Self {
            builtins,
            behaviors: BTreeMap::new(),
            states: BTreeMap::new(),
            instances: BTreeMap::new(),
            timelines: Timelines::new(),
            focus: FocusSlots::new(),
            timers: Timers::new(),
            skins: SkinRegistry::new(),
            sound: None,
            trace: None,
            hook: None,
            fonts: FontLibrary::new(),
            text: TextRenderer::new(),
            keyboard_keys_disabled: false,
            global_mute: false,
            init_focus_depth: 0,
            load_depth: 0,
            dispatching: Vec::new(),
            destroyed: BTreeSet::new(),
            pending_free: Vec::new(),
            texture_releases: TextureReleases::new(),
        }
    }

    /// A texture object of `image` for image presenters and custom images.
    /// The draw destroys its texture after the last reference drops.
    pub fn create_texture_object(&self, image: xbox360_draw::image::Image) -> TextureObject {
        self.texture_releases.create(image)
    }

    /// `XuiSetMessageHook` 0x9213d730: `None` removes the hook.
    pub fn set_message_hook(&mut self, hook: Option<Rc<dyn MessageHook>>) {
        self.hook = hook;
    }

    pub(crate) fn message_hook(&self) -> Option<Rc<dyn MessageHook>> {
        self.hook.clone()
    }

    /// The factory [`Runtime::create_texture_object`] uses, for objects
    /// that create texture objects later without a runtime at hand.
    pub fn texture_factory(&self) -> TextureReleases {
        self.texture_releases.clone()
    }

    /// Registers the message handler of an application class. Classes
    /// without a behavior pass every message to their base class.
    pub fn register_behavior(
        &mut self,
        registry: &Registry,
        class_name: &'static str,
        behavior: Rc<dyn ClassBehavior>,
    ) -> Result<(), RuntimeError> {
        let Some(class) = registry.find(class_name) else {
            return Err(RuntimeError::UnknownClass { name: class_name });
        };

        if self.behaviors.contains_key(&class) {
            return Err(RuntimeError::DuplicateBehavior { name: class_name });
        }

        self.behaviors.insert(class, behavior);
        Ok(())
    }

    pub fn set_sound_sink(&mut self, sink: Option<Box<dyn SoundSink + 'p>>) {
        self.sound = sink;
    }

    pub fn set_trace(&mut self, trace: Option<Box<dyn FnMut(TraceEvent) + 'p>>) {
        self.trace = trace;
    }

    pub fn timelines(&self) -> &Timelines {
        &self.timelines
    }

    pub fn focus(&self) -> &FocusSlots {
        &self.focus
    }

    /// `XuiSetTimer` and `XuiKillTimer` for application classes.
    pub fn timers_mut(&mut self) -> &mut Timers {
        &mut self.timers
    }

    pub fn skins(&self) -> &SkinRegistry<'p> {
        &self.skins
    }

    pub fn skins_mut(&mut self) -> &mut SkinRegistry<'p> {
        &mut self.skins
    }

    pub(crate) fn behavior(&self, class: ClassId) -> Option<Rc<dyn ClassBehavior>> {
        self.behaviors.get(&class).map(Rc::clone)
    }

    pub(crate) fn builtin(&self, class: ClassId) -> Option<Builtin> {
        self.builtins.get(class.index()).copied().flatten()
    }

    /// Instance data of a class level, created on first use. A freed
    /// element gets none.
    pub(crate) fn instance_mut<T: Any>(
        &mut self,
        tree: &ElementTree,
        element: ElementId,
        class: ClassId,
    ) -> Option<&mut T> {
        if !self.instances.contains_key(&(element, class)) {
            if !tree.contains(element) {
                return None;
            }
            let behavior = self.behaviors.get(&class)?;
            let instance = behavior.create_instance()?;
            self.instances.insert((element, class), instance);
        }

        self.instances.get_mut(&(element, class))?.downcast_mut()
    }

    pub(crate) fn trace(&mut self, event: TraceEvent) {
        if let Some(trace) = &mut self.trace {
            trace(event);
        }
    }

    /// Drops every piece of state of a freed element.
    pub(crate) fn forget(&mut self, element: ElementId) {
        self.states.remove(&element);
        self.instances.retain(|(owner, _), _| *owner != element);
        self.timelines.remove(element);
        self.timers.remove_element(element);
        self.focus.forget(element);
    }
}

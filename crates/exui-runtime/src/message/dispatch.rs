//! `XuiSendMessage`, `XuiBubbleMessage` and subtree broadcast over class
//! levels.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::any::Any;

use exui_core::Registry;
use exui_core::schema::ClassId;
use exui_core::schema::PropDef;

use crate::control;
use crate::message::Body;
use crate::message::Message;
use crate::message::ResourceMask;
use crate::message::Runtime;
use crate::resource::Resources;
use crate::scene::ElementId;
use crate::scene::ElementTree;
use crate::scene::PropValue;

/// Message handler of one class level. Built-in classes are handled by the
/// runtime; the application registers behaviors for its own classes by
/// class name with [`Runtime::register_behavior`].
pub trait ClassBehavior {
    /// Instance data of this class level for a new element.
    fn create_instance(&self) -> Option<Box<dyn Any>> {
        None
    }

    /// Handles a message sent to `element` at this class level. Set
    /// `message.handled` to stop the walk to the base classes.
    fn on_message(&self, cx: &mut Context<'_, '_>, element: ElementId, message: &mut Message);

    /// Setter of a property `def` that this class level declares, run by
    /// [`Context::set_property`]. Returns true when the setter took the
    /// value; false stores it unchanged.
    fn set_property(
        &self,
        _cx: &mut Context<'_, '_>,
        _element: ElementId,
        _def: &'static PropDef,
        _value: &PropValue,
    ) -> bool {
        false
    }
}

/// Global message hook (`XuiSetMessageHook`). `XuiSendMessage` calls it at
/// every class level, before that level's procedure, so one message can
/// reach it several times.
pub trait MessageHook {
    fn on_message(&self, cx: &mut Context<'_, '_>, element: ElementId, message: &mut Message);
}

/// Handle of one class level of an element. Each level has its own handle;
/// a message sent to it starts at that level, so derived levels never see
/// it (timers of a base class, for example).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LevelHandle {
    pub element: ElementId,
    /// `None`: the most-derived level (the canonical element handle).
    pub level: Option<ClassId>,
}

impl LevelHandle {
    pub fn element(element: ElementId) -> Self {
        Self {
            element,
            level: None,
        }
    }
}

/// Everything a handler can reach.
pub struct Context<'a, 'p> {
    pub tree: &'a mut ElementTree,
    pub registry: &'a Registry,
    pub resources: &'a Resources<'p>,
    pub runtime: &'a mut Runtime<'p>,
}

impl<'a, 'p> Context<'a, 'p> {
    pub fn new(
        tree: &'a mut ElementTree,
        registry: &'a Registry,
        resources: &'a Resources<'p>,
        runtime: &'a mut Runtime<'p>,
    ) -> Self {
        Self {
            tree,
            registry,
            resources,
            runtime,
        }
    }

    /// `XuiSendMessage`: offers the message to each class level, most
    /// derived first, until one sets `handled`. Destroy reaches every level.
    pub fn send(&mut self, element: ElementId, message: &mut Message) {
        let Some(class) = self.tree.class(element) else {
            return;
        };

        message.handled = false;
        self.walk_levels(element, Some(class), message);
    }

    /// Sends starting at the base class of `level`, as a class forwarding a
    /// message to its base level does.
    pub fn send_to_base(&mut self, element: ElementId, level: ClassId, message: &mut Message) {
        let base = self
            .registry
            .class(level)
            .and_then(|class| class.base())
            .map(|class| class.id());

        self.walk_levels(element, base, message);
    }

    /// `XuiSendMessage` to the handle of one class level: the walk starts
    /// at that level.
    pub fn send_to_level(&mut self, handle: LevelHandle, message: &mut Message) {
        let Some(level) = handle.level else {
            self.send(handle.element, message);
            return;
        };

        message.handled = false;
        self.walk_levels(handle.element, Some(level), message);
    }

    /// The handle of class level `level` of `element`. The most-derived
    /// level is the canonical element handle.
    pub fn level_handle(&self, element: ElementId, level: ClassId) -> LevelHandle {
        let most_derived = self.tree.class(element) == Some(level);

        LevelHandle {
            element,
            level: (!most_derived).then_some(level),
        }
    }

    fn walk_levels(&mut self, element: ElementId, start: Option<ClassId>, message: &mut Message) {
        let destroy = matches!(message.body, Body::Destroy);
        self.runtime.dispatching.push(element);

        let mut level = start;
        while let Some(class) = level {
            if !self.tree.contains(element) {
                break;
            }

            if let Some(hook) = self.runtime.message_hook() {
                hook.on_message(self, element, message);
            }

            if let Some(behavior) = self.runtime.behavior(class) {
                behavior.on_message(self, element, message);
            } else if let Some(builtin) = self.runtime.builtin(class) {
                control::handle(self, element, builtin, class, message);
            }

            if message.handled && !destroy {
                break;
            }

            level = self
                .registry
                .class(class)
                .and_then(|class| class.base())
                .map(|class| class.id());
        }

        self.runtime.dispatching.pop();
        self.finish_deferred_destroys();
    }

    /// `XuiBubbleMessage`: sends to the element, then to each parent while
    /// the message stays unhandled.
    pub fn bubble(&mut self, element: ElementId, message: &mut Message) {
        let mut current = Some(element);
        while let Some(target) = current {
            // The parent is read after the handler, which may move the
            // element.
            self.send(target, message);
            if message.handled {
                return;
            }

            current = self.tree.parent(target);
        }
    }

    /// Sends a new message and returns it.
    pub fn send_body(&mut self, element: ElementId, body: Body) -> Message {
        let mut message = Message::new(body);
        self.send(element, &mut message);
        message
    }

    /// Bubbles a new message and returns it.
    pub fn bubble_body(&mut self, element: ElementId, body: Body) -> Message {
        let mut message = Message::new(body);
        self.bubble(element, &mut message);
        message
    }

    /// `XuiBroadcastMessage` 0x92147d40: the element first, then its children
    /// in order, pre-order. The first child is read after the parent's
    /// handler ran and the next sibling before recursing.
    pub fn broadcast(&mut self, element: ElementId, body: &Body) {
        let mut stack: Vec<ElementId> = Vec::new();
        stack.push(element);

        while let Some(current) = stack.pop() {
            let mut message = Message::new(body.clone());
            self.send(current, &mut message);

            let mut children: Vec<ElementId> = self.tree.children(current).collect();
            children.reverse();
            stack.extend(children);
        }
    }

    /// `XuiFreeResources` 0x921508B8: broadcasts 0x28 with `mask` from
    /// `root` when given, then flushes the texture cache on bit 0x1 and the
    /// font cache on bit 0x4.
    pub fn free_resources(&mut self, root: Option<ElementId>, mask: ResourceMask) {
        if let Some(root) = root {
            self.broadcast(root, &Body::ReleaseResources { mask });
        }

        if mask.contains(ResourceMask::TEXTURES) {
            self.resources.flush_textures(&self.tree.cached_textures());
        }

        if mask.contains(ResourceMask::FONTS) {
            self.runtime.text.release_fonts();
        }
    }

    /// Instance data of an application class level.
    pub fn instance_mut<T: Any>(&mut self, element: ElementId, class: ClassId) -> Option<&mut T> {
        self.runtime.instance_mut(self.tree, element, class)
    }

    /// Class id by name.
    pub fn class_id(&self, name: &str) -> Option<ClassId> {
        self.registry.find(name)
    }

    /// Whether the element's class is `class` or derives from it.
    pub fn is_a(&self, element: ElementId, class: ClassId) -> bool {
        self.tree
            .class(element)
            .and_then(|own| self.registry.class(own))
            .is_some_and(|own| own.is_a(class))
    }
}

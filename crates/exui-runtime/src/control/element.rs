//! XuiElement level (0x92168ea0), element lifecycle, property setters with
//! side effects and the timeline element API.

use alloc::rc::Rc;
use alloc::vec::Vec;
use core::ptr;

use exui_core::schema::PropDef;

use crate::control::Builtin;
use crate::control::ClassState;
use crate::control::props;
use crate::error::SetPropertyError;
use crate::message::Body;
use crate::message::ClassBehavior;
use crate::message::Context;
use crate::message::Message;
use crate::message::NotifyKind;
use crate::message::TabStopKind;
use crate::message::TraceEvent;
use crate::scene::ElementFlags;
use crate::scene::ElementId;
use crate::scene::PropValue;
use crate::scene::PropertyError;
use crate::scene::image::CustomImage;
use crate::timeline;
use crate::timeline::NotifyRoute;
use crate::timeline::PlayRange;
use crate::timeline::PropertyPath;
use crate::timeline::TimelineData;
use crate::timeline::TimelineHost;
use crate::timeline::TimelineNotify;
use crate::timeline::TimelineNotifyKind;
use crate::timeline::Timelines;

pub(super) fn on_message(cx: &mut Context<'_, '_>, element: ElementId, message: &mut Message) {
    match &mut message.body {
        Body::SetShow { show } => {
            let show = *show;
            let _ = cx.tree.set_show(element, show);
            message.handled = true;
        }
        Body::Destroy => {
            cx.runtime.timelines.remove(element);
        }
        Body::ReleaseResources { mask } => {
            let mask = *mask;
            if let Some(content) = cx.tree.get_mut(element).map(|element| &mut element.content) {
                content.release_resources(mask);
            }
        }
        Body::FindTabStop(query) => {
            if let Some(target) = cx.find_tab_stop(element, query.direction) {
                query.target = Some(target);
                query.target_is_source = target == query.source;
                message.handled = true;
            }
        }
        Body::GetFocusDisabled { disabled } => {
            *disabled = cx
                .tree
                .flags(element)
                .is_some_and(|flags| flags.contains(ElementFlags::DISABLE_FOCUS_RECURSION));
            message.handled = true;
        }
        Body::SetFocusDisabled { disabled } => {
            let disabled = *disabled;
            message.handled = true;
            set_focus_disabled(cx, element, disabled);
        }
        _ => {}
    }
}

/// XuiElement on 0x807: when set, focus is pushed out of the element.
fn set_focus_disabled(cx: &mut Context<'_, '_>, element: ElementId, disabled: bool) {
    if let Some(mut flags) = cx.tree.flags(element) {
        flags.set(ElementFlags::DISABLE_FOCUS_RECURSION, disabled);
        let _ = cx.tree.set_flags(element, flags);
    }

    if !disabled {
        return;
    }

    while let Some(owner) = cx.focus_owner(element) {
        let Some(user) = cx.focus_user(owner) else {
            break;
        };

        let message = cx.send_body(
            element,
            Body::FindTabStop(crate::message::NavQuery {
                direction: TabStopKind::Next,
                source: element,
                target: None,
                target_is_source: false,
            }),
        );
        let target = match message.body {
            Body::FindTabStop(query) if message.handled && !query.target_is_source => query.target,
            _ => None,
        };

        let before = cx.runtime.focus.slot(user);
        let _ = cx.set_focus(target, user);
        if cx.runtime.focus.slot(user) == before {
            // The slot did not change; stop instead of looping forever.
            break;
        }
    }
}

impl Context<'_, '_> {
    /// `XuiElementInsertChild` 0x92164530: links the child, sends 0x4 to the
    /// parent and, outside loading, 0x5 over the new subtree.
    pub fn insert_child(
        &mut self,
        parent: ElementId,
        child: ElementId,
        before: Option<ElementId>,
    ) -> Result<(), crate::scene::TreeError> {
        self.tree.insert_child(parent, child, before)?;
        self.send_body(parent, Body::ChildAdded { child });

        if self.runtime.load_depth == 0 {
            self.send_init_preorder(child, Some(parent));
        }

        Ok(())
    }

    /// `0x9214d478`: unlinks and, outside loading, sends 0x20 to the old
    /// parent.
    pub fn unlink(&mut self, element: ElementId) {
        let parent = self.tree.parent(element);
        let _ = self.tree.unlink(element);

        if let Some(parent) = parent
            && self.runtime.load_depth == 0
        {
            self.send_body(parent, Body::ChildRemoved { child: element });
        }
    }

    fn send_init_preorder(&mut self, root: ElementId, parent: Option<ElementId>) {
        let mut stack = Vec::new();
        stack.push((root, parent));
        while let Some((element, parent)) = stack.pop() {
            self.send_body(element, Body::Init { parent });

            let mut children: Vec<ElementId> = self.tree.children(element).collect();
            children.reverse();
            stack.extend(children.into_iter().map(|child| (child, Some(element))));
        }
    }

    /// `XuiInitElementTree` 0x92150c98: 0x5 pre-order, then 0x13 after
    /// each element's children.
    pub fn init_element_tree(&mut self, root: ElementId) {
        enum Step {
            Enter(ElementId, Option<ElementId>),
            Complete(ElementId),
        }

        let parent = self.tree.parent(root);
        let mut stack = Vec::new();
        stack.push(Step::Enter(root, parent));
        while let Some(step) = stack.pop() {
            match step {
                Step::Enter(element, parent) => {
                    self.send_body(element, Body::Init { parent });
                    stack.push(Step::Complete(element));

                    let mut children: Vec<ElementId> = self.tree.children(element).collect();
                    children.reverse();
                    stack.extend(
                        children
                            .into_iter()
                            .map(|child| Step::Enter(child, Some(element))),
                    );
                }
                Step::Complete(element) => {
                    self.send_body(element, Body::InitComplete);
                }
            }
        }
    }

    /// `XuiDestroyObject` 0x92147c50: sends 0xa to every element of the
    /// subtree, unlinks the subtree and frees it. Freeing waits until no
    /// element of the subtree is handling a message, in place of the
    /// per-element reference count the original holds during a dispatch
    /// (`0x921440e8`). Not 1:1: when the original frees each element of a
    /// subtree with one element still in dispatch is not reverse
    /// engineered.
    pub fn destroy(&mut self, element: ElementId) {
        // A destroy already in progress or awaiting its free covers it.
        if !self.tree.contains(element) || self.runtime.destroyed.contains(&element) {
            return;
        }

        // Destroy handlers may create children; those are destroyed too.
        loop {
            let fresh: Vec<ElementId> = self
                .subtree(element)
                .into_iter()
                .filter(|node| !self.runtime.destroyed.contains(node))
                .collect();
            if fresh.is_empty() {
                break;
            }

            for node in fresh {
                // An earlier handler may have destroyed it already.
                if !self.tree.contains(node) || !self.runtime.destroyed.insert(node) {
                    continue;
                }
                self.send_body(node, Body::Destroy);
            }
        }

        self.unlink(element);

        if self.subtree_dispatching(element) {
            self.runtime.pending_free.push(element);
            return;
        }
        self.free(element);
    }

    /// Frees the destroyed subtrees whose elements have all left dispatch.
    pub(crate) fn finish_deferred_destroys(&mut self) {
        let mut index = 0;
        while let Some(&element) = self.runtime.pending_free.get(index) {
            if self.subtree_dispatching(element) {
                index += 1;
                continue;
            }

            self.runtime.pending_free.remove(index);
            self.free(element);
        }
    }

    fn free(&mut self, element: ElementId) {
        let subtree = self.subtree(element);
        let _ = self.tree.destroy(element);
        for node in subtree {
            self.runtime.destroyed.remove(&node);
            self.runtime.forget(node);
        }
    }

    fn subtree_dispatching(&self, root: ElementId) -> bool {
        self.runtime
            .dispatching
            .iter()
            .any(|node| self.is_descendant(*node, root))
    }

    fn subtree(&self, root: ElementId) -> Vec<ElementId> {
        let mut subtree = Vec::new();
        let mut stack = Vec::new();
        stack.push(root);
        while let Some(node) = stack.pop() {
            subtree.push(node);
            stack.extend(self.tree.children(node));
        }

        subtree
    }

    /// `XuiElementSetShow`: message 0x12.
    pub fn set_show(&mut self, element: ElementId, show: bool) {
        self.send_body(element, Body::SetShow { show });
        self.flush_size_changes();
    }

    /// Sets or removes the image `element` draws before its content, the
    /// drawing a control's render message handler does with the DC.
    pub fn set_custom_image(&mut self, element: ElementId, image: Option<CustomImage>) {
        // A destroyed element has nothing to draw.
        let _ = self.tree.set_custom_image(element, image);
    }

    /// Sets a property through its class setter. Properties whose setter
    /// has side effects run them; others are stored directly.
    pub fn set_property(
        &mut self,
        element: ElementId,
        path: &[&'static PropDef],
        index: Option<u32>,
        value: PropValue,
    ) -> Result<(), SetPropertyError> {
        let Some(&first) = path.first() else {
            return Err(SetPropertyError::Store {
                source: PropertyError::EmptyPath,
            });
        };
        if !self.owns_property(element, first) {
            return Err(SetPropertyError::NotOwned { name: first.name });
        }

        if path.len() == 1 && self.run_setter(element, first, &value)? {
            self.flush_size_changes();
            return Ok(());
        }

        self.tree.set_property(element, path, index, value)?;
        self.flush_size_changes();

        Ok(())
    }

    fn owns_property(&self, element: ElementId, def: &'static PropDef) -> bool {
        let Some(class) = self.tree.class(element) else {
            return false;
        };

        let mut level = self.registry.class(class);
        while let Some(current) = level {
            if current
                .def()
                .props
                .iter()
                .any(|candidate| ptr::eq(candidate, def))
            {
                return true;
            }
            level = current.base();
        }

        false
    }

    /// Behavior of the class level that declares `def`.
    fn declaring_behavior(
        &self,
        element: ElementId,
        def: &'static PropDef,
    ) -> Option<Rc<dyn ClassBehavior>> {
        let mut level = self.registry.class(self.tree.class(element)?);
        while let Some(current) = level {
            if current
                .def()
                .props
                .iter()
                .any(|candidate| ptr::eq(candidate, def))
            {
                return self.runtime.behavior(current.id());
            }
            level = current.base();
        }

        None
    }

    /// Setters that send messages. Returns true when the setter stored the
    /// value itself.
    fn run_setter(
        &mut self,
        element: ElementId,
        def: &'static PropDef,
        value: &PropValue,
    ) -> Result<bool, SetPropertyError> {
        let is = |candidate: &'static PropDef| ptr::eq(def, candidate);

        // The Show setter 0x92145698 checks the type and calls
        // XuiElementSetShow, which sends 0x12.
        if is(props::show()) {
            if let PropValue::Bool(show) = value {
                self.send_body(element, Body::SetShow { show: *show });
                return Ok(true);
            }
        } else if is(props::enabled()) {
            if let PropValue::Bool(enabled) = value {
                self.send_body(element, Body::SetEnabled { enabled: *enabled });
                return Ok(true);
            }
        } else if is(props::text()) {
            if let PropValue::String(text) = value {
                self.send_body(
                    element,
                    Body::SetSourceText {
                        item: 0,
                        association: 0,
                        item_data: false,
                        text: text.as_deref().map(Rc::from),
                    },
                );
                return Ok(true);
            }
        } else if is(props::visual()) {
            // `XuiControl_Visual_Set` 0x9215b458: store, then recreate.
            self.tree
                .set_property(element, &[def], None, value.clone())?;
            self.send_body(element, Body::CreateVisual);
            return Ok(true);
        } else if is(props::disable_focus_recursion()) {
            if let PropValue::Bool(disabled) = value {
                self.send_body(
                    element,
                    Body::SetFocusDisabled {
                        disabled: *disabled,
                    },
                );
                self.tree
                    .set_property(element, &[def], None, value.clone())?;
                return Ok(true);
            }
        } else if is(props::sound_state()) {
            let changed = self.tree.property(element, &[def], None).as_ref() != Some(value);
            self.tree
                .set_property(element, &[def], None, value.clone())?;
            if changed {
                self.schedule_sound_apply(element);
            }
            return Ok(true);
        } else if is(props::sound_volume()) {
            if let PropValue::Float(db) = value {
                self.send_body(element, Body::SoundSetVolume { db: *db });
                return Ok(true);
            }
        } else if is(props::sound_file()) {
            let current = self.tree.string(element, &[def]);
            let new = match value {
                PropValue::String(text) => text.as_deref(),
                _ => None,
            };
            if current == new {
                return Ok(true);
            }
            self.tree
                .set_property(element, &[def], None, value.clone())?;

            let autoplay = self
                .tree
                .flags(element)
                .is_some_and(|flags| !flags.contains(ElementFlags::AUTOPLAY_DISABLED));
            if autoplay && new.is_some() {
                self.schedule_sound_apply(element);
            }
            return Ok(true);
        } else if is(props::slider_value()) {
            if let PropValue::Integer(new) = value {
                self.slider_set_value(element, *new);
                return Ok(true);
            }
        } else if is(props::progress_value()) {
            if let PropValue::Integer(new) = value {
                self.progress_set_value(element, *new);
                return Ok(true);
            }
        } else if is(props::items_text()) || is(props::items_image()) || is(props::items_nav_path())
        {
            self.tree
                .set_property(element, &[def], None, value.clone())?;
            self.common_list_apply_item_props(element);
            return Ok(true);
        } else if is(props::xui_image_path()) {
            // `XuiImage_SetPath` 0x9213f638: the store detaches scene content.
            self.tree
                .set_property(element, &[def], None, value.clone())?;
            self.destroy_released_image_scene(element);
            return Ok(true);
        } else if is(props::edit_text_limit()) {
            self.tree
                .set_property(element, &[def], None, value.clone())?;
            self.edit_apply_text_limit(element);
            return Ok(true);
        } else if is(props::edit_allowed_chars()) {
            if let PropValue::String(text) = value {
                self.edit_set_allowed_chars(element, text.as_deref());
                return Ok(true);
            }
        } else if is(props::edit_password_char()) {
            if let PropValue::String(text) = value {
                self.edit_set_password_char(element, text.as_deref());
                return Ok(true);
            }
        } else if let Some(behavior) = self.declaring_behavior(element, def)
            && behavior.set_property(self, element, def, value)
        {
            return Ok(true);
        }

        Ok(false)
    }

    /// Delivers the size changes recorded by the tree: 0x10 to the element,
    /// 0x8 to each child with the new size, then 0x80d.
    pub fn flush_size_changes(&mut self) {
        loop {
            let changes = self.tree.take_size_changes();
            if changes.is_empty() {
                return;
            }

            for change in changes {
                let element = change.element;
                if !self.tree.contains(element) {
                    continue;
                }

                self.send_body(
                    element,
                    Body::SizeChanging {
                        old: (change.old.width, change.old.height),
                        new: (change.new.width, change.new.height),
                    },
                );

                let children: Vec<ElementId> = self.tree.children(element).collect();
                for child in children {
                    self.send_body(
                        child,
                        Body::ParentSizeChanged {
                            width: change.new.width,
                            height: change.new.height,
                        },
                    );
                }

                self.send_body(element, Body::SizeChanged);
            }
        }
    }

    /// `XuiElement_PlayTimelineImpl` 0x9215a050.
    pub fn play_timeline(&mut self, element: ElementId, range: PlayRange, recurse: bool) {
        if self.runtime.timelines.get(element).is_some() {
            self.runtime.trace(TraceEvent::Play {
                element,
                start: range.start,
                end: range.end,
            });
        }

        self.walk_timelines(element, recurse, |timelines, node, flags| {
            if !flags.contains(ElementFlags::AUTOPLAY_DISABLED)
                && let Some(timeline) = timelines.get_mut(node)
            {
                timeline.play(range, true);
            }
        });
    }

    /// `XuiElement_StopTimelineImpl` 0x92159f98. Stop ignores the autoplay
    /// flag.
    pub fn stop_timeline(&mut self, element: ElementId, recurse: bool) {
        self.walk_timelines(element, recurse, |timelines, node, _| {
            if let Some(timeline) = timelines.get_mut(node) {
                timeline.stop();
            }
        });
    }

    /// `XuiElement_SetTimelineImpl`: gives a live `element` a stopped
    /// timeline of `data`, replacing any previous one. Tracks animate the
    /// child with their target id, as when a visual or scene loads.
    pub fn set_timeline(&mut self, element: ElementId, data: Rc<TimelineData>) {
        if !self.tree.contains(element) {
            return;
        }

        let tree = &*self.tree;
        self.runtime.timelines.create(element, data, |id| {
            id.and_then(|id| tree.child_by_id(element, id))
        });
    }

    /// Recursion shared by play and stop: children that are not visuals,
    /// unless the element disables timeline recursion.
    fn walk_timelines(
        &mut self,
        element: ElementId,
        recurse: bool,
        mut visit: impl FnMut(&mut Timelines, ElementId, ElementFlags),
    ) {
        let mut stack = Vec::new();
        stack.push(element);
        while let Some(node) = stack.pop() {
            let Some(flags) = self.tree.flags(node) else {
                continue;
            };
            visit(&mut self.runtime.timelines, node, flags);

            // Children are always played with recursion on.
            let blocked = flags.contains(ElementFlags::TIMELINE_RECURSION_API)
                || flags.contains(ElementFlags::DISABLE_TIMELINE_RECURSION);
            if !recurse || blocked {
                continue;
            }

            let children: Vec<ElementId> = self.tree.children(node).collect();
            for child in children.into_iter().rev() {
                if !self.is_builtin_a(child, Builtin::Visual) {
                    stack.push(child);
                }
            }
        }
    }

    /// Advances every timeline by the frame time (`XuiTimelinesTick`).
    pub fn tick_timelines(&mut self, dt_ms: f32) {
        timeline::tick(self, dt_ms);
    }

    /// Advances one timeline (`XuiTimeline_Advance`).
    pub fn advance_timeline(&mut self, owner: ElementId, dt_ms: f32) {
        timeline::advance(self, owner, dt_ms);
    }

    /// Runs the timers once (`XuiProcessTimers`).
    pub fn process_timers(&mut self) {
        let due = self.runtime.timers.begin_pass(self.tree);
        for (handle, id) in due {
            if self.runtime.timers.fire_if_due(handle, id) {
                let mut message = Message::new(Body::Timer { id });
                self.send_to_level(handle, &mut message);
            }
        }
    }
}

impl TimelineHost for Context<'_, '_> {
    fn timelines(&mut self) -> &mut Timelines {
        &mut self.runtime.timelines
    }

    fn apply_value(&mut self, target: ElementId, path: &PropertyPath, value: &PropValue) {
        let Some(&leaf) = path.defs.last() else {
            return;
        };

        let index = if leaf.flags.is_indexed() {
            Some(path.index.unwrap_or(0))
        } else {
            None
        };

        // `XuiObjectSetPropertyById` failures are ignored by the timeline.
        let _ = self.set_property(target, &path.defs, index, value.clone());
    }

    fn notify(&mut self, notify: TimelineNotify) {
        let body = match notify.kind {
            TimelineNotifyKind::Started => Body::TimelineStart {
                owner: notify.source,
            },
            TimelineNotifyKind::Ended => Body::TimelineEnd {
                owner: notify.source,
            },
        };

        match notify.route {
            NotifyRoute::Bubble(element) => {
                self.bubble_body(element, body);
            }
            NotifyRoute::Send(element) => {
                self.send_body(element, body);
            }
        }
    }
}

impl Context<'_, '_> {
    /// Bubbles a notify from `from` with `source` as the source element.
    pub fn bubble_notify(
        &mut self,
        from: Option<ElementId>,
        source: ElementId,
        kind: NotifyKind,
    ) -> Message {
        let mut message = Message::new(Body::Notify(crate::message::Notify { kind, source }));
        if let Some(from) = from {
            self.bubble(from, &mut message);
        }
        message
    }

    /// Bubbles a notify starting at the source's parent, as most notify
    /// helpers do.
    pub fn notify_parent(&mut self, source: ElementId, kind: NotifyKind) -> Message {
        let parent = self.tree.parent(source);
        self.bubble_notify(parent, source, kind)
    }

    /// `XuiElementGetSkinPrefix`: bubbled 0x22.
    pub fn skin_prefix(&mut self, element: ElementId) -> Option<Rc<[u16]>> {
        let message = self.bubble_body(element, Body::GetSkinPrefix { prefix: None });
        match message.body {
            Body::GetSkinPrefix { prefix } => prefix,
            _ => None,
        }
    }

    /// Skin prefix stored by a visual or canvas.
    pub(crate) fn stored_prefix(&mut self, element: ElementId) -> Option<&mut Option<Rc<[u16]>>> {
        match self.class_state(element)? {
            ClassState::Prefix(prefix) => Some(prefix),
            _ => None,
        }
    }
}

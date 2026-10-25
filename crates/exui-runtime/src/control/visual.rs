//! XuiVisual (0x9215bf68), XuiCanvas (0x9215ce58) and XuiTransition
//! (0x92165438) levels.

use alloc::vec::Vec;

use exui_core::schema::PropDef;

use crate::control::ClassState;
use crate::control::props;
use crate::message::Body;
use crate::message::Context;
use crate::message::Message;
use crate::message::TransitionAction;
use crate::message::TransitionKind;
use crate::scene::ElementId;
use crate::scene::PropValue;

/// XuiTransition instance: kind, action and the scene property values
/// saved before the transition animates them.
#[derive(Clone, Debug)]
pub struct TransitionState {
    pub kind: TransitionKind,
    pub action: TransitionAction,
    pub saved: Vec<SavedProperty>,
}

/// A scene property value saved by `XuiTransition_SaveSceneProps`.
#[derive(Clone, Debug)]
pub struct SavedProperty {
    pub path: Vec<&'static PropDef>,
    pub index: Option<u32>,
    pub value: PropValue,
}

impl TransitionState {
    pub fn new() -> Self {
        Self {
            kind: TransitionKind::To,
            action: TransitionAction::Keep,
            saved: Vec::new(),
        }
    }
}

impl Default for TransitionState {
    fn default() -> Self {
        Self::new()
    }
}

pub(super) fn on_visual(cx: &mut Context<'_, '_>, element: ElementId, message: &mut Message) {
    match &mut message.body {
        Body::Init { parent } => {
            let parent = *parent;
            message.handled = true;
            if let Some(parent) = parent
                && cx.is_builtin_a(parent, crate::control::Builtin::Control)
                && let Some(size) = cx.tree.get(parent).map(|parent| parent.size())
            {
                fit_to_size(cx, element, size.width, size.height);
            }
        }
        Body::ParentSizeChanged { width, height } => {
            let (width, height) = (*width, *height);
            message.handled = true;
            fit_to_size(cx, element, width, height);
        }
        Body::GetSkinPrefix { .. } => {
            let prefix = cx.stored_prefix(element).and_then(|prefix| prefix.clone());
            message.body = Body::GetSkinPrefix { prefix };
            message.handled = true;
        }
        Body::SetSkinPrefix { prefix } => {
            let prefix = prefix.clone().filter(|prefix| !prefix.is_empty());
            if let Some(stored) = cx.stored_prefix(element) {
                *stored = prefix;
            }
            message.handled = true;
        }
        // Tab order never enters a visual.
        Body::FindTabStop(query) => {
            query.target_is_source = true;
            message.handled = true;
        }
        _ => {}
    }
}

/// `XuiVisual_FitToSize` 0x9215bea0: the visual always takes the control's
/// size; the tree re-anchors its children.
fn fit_to_size(cx: &mut Context<'_, '_>, element: ElementId, width: f32, height: f32) {
    let Some(size) = cx.tree.get(element).map(|visual| visual.size()) else {
        return;
    };

    if size.width != width {
        let _ = cx
            .tree
            .set_property(element, &[props::width()], None, PropValue::Float(width));
    }
    if size.height != height {
        let _ = cx
            .tree
            .set_property(element, &[props::height()], None, PropValue::Float(height));
    }

    cx.flush_size_changes();
}

pub(super) fn on_canvas(cx: &mut Context<'_, '_>, element: ElementId, message: &mut Message) {
    match &mut message.body {
        Body::GetSkinPrefix { .. } => {
            let prefix = cx.stored_prefix(element).and_then(|prefix| prefix.clone());
            message.body = Body::GetSkinPrefix { prefix };
            message.handled = true;
        }
        Body::SetSkinPrefix { prefix } => {
            let prefix = prefix.clone().filter(|prefix| !prefix.is_empty());
            if let Some(stored) = cx.stored_prefix(element) {
                *stored = prefix;
            }
            message.handled = true;
        }
        // `XuiCanvas_OnMessage` 0x9215CE58, through 0x9215B560.
        Body::Relocalize => {
            message.handled = true;
            cx.relocalize_own(element);
        }
        _ => {}
    }
}

pub(super) fn on_transition(cx: &mut Context<'_, '_>, element: ElementId, message: &mut Message) {
    if let Body::TimelineEnd { owner } = message.body
        && owner == element
    {
        message.handled = true;
        cx.on_transition_wrapper_end(element);
    }
}

impl Context<'_, '_> {
    pub(crate) fn transition_state(&mut self, element: ElementId) -> Option<&mut TransitionState> {
        match self.class_state(element)? {
            ClassState::Transition(state) => Some(state),
            _ => None,
        }
    }
}

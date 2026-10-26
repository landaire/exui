//! Built-in XUI classes: per-element instance state and message handlers.

mod base;
mod button;
mod edit;
mod element;
mod image;
mod list;
pub mod props;
mod range;
mod scene;
mod sound;
mod state_frames;
mod tab_scene;
mod text;
mod visual;

#[cfg(test)]
pub(crate) mod tests;

use alloc::boxed::Box;

use exui_core::schema::ClassId;

use crate::message::Context;
use crate::message::Message;
use crate::scene::ElementId;

pub use base::ControlState;
pub(crate) use edit::loaded_setters;
pub use list::CommonListState;
pub use list::ItemFlags;
pub use list::ListLayout;
pub use list::ListState;
pub use range::ProgressState;
pub use range::SliderState;
pub use scene::SceneState;
pub use scene::TransIndexSide;
pub use sound::SoundState;
pub use state_frames::StateFrames;
pub use tab_scene::TabSceneState;
pub use tab_scene::TabStep;
pub use text::MessageBoxState;
pub use text::RuntimePresenterSource;
pub use visual::SavedProperty;
pub use visual::TransitionState;

/// A class level implemented by the runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Builtin {
    Element,
    Canvas,
    Control,
    Scene,
    Button,
    BackButton,
    NavButton,
    Checkbox,
    RadioButton,
    RadioGroup,
    TextPresenter,
    HtmlPresenter,
    ImagePresenter,
    Image,
    Visual,
    Group,
    Sound,
    SoundXAudio,
    Transition,
    ListItem,
    ScrollEnd,
    ScrollBar,
    List,
    CommonList,
    ProgressBar,
    Slider,
    Caret,
    Edit,
    MessageBox,
    Label,
    TabScene,
}

impl Builtin {
    pub fn from_name(name: &str) -> Option<Self> {
        let builtin = match name {
            "XuiElement" => Self::Element,
            "XuiCanvas" => Self::Canvas,
            "XuiControl" => Self::Control,
            "XuiScene" => Self::Scene,
            "XuiButton" => Self::Button,
            "XuiBackButton" => Self::BackButton,
            "XuiNavButton" => Self::NavButton,
            "XuiCheckbox" => Self::Checkbox,
            "XuiRadioButton" => Self::RadioButton,
            "XuiRadioGroup" => Self::RadioGroup,
            "XuiTextPresenter" => Self::TextPresenter,
            "XuiHtmlPresenter" => Self::HtmlPresenter,
            "XuiImagePresenter" => Self::ImagePresenter,
            "XuiImage" => Self::Image,
            "XuiVisual" => Self::Visual,
            "XuiGroup" => Self::Group,
            "XuiSound" => Self::Sound,
            "XuiSoundXAudio" => Self::SoundXAudio,
            "XuiTransition" => Self::Transition,
            "XuiListItem" => Self::ListItem,
            "XuiScrollEnd" => Self::ScrollEnd,
            "XuiScrollBar" => Self::ScrollBar,
            "XuiList" => Self::List,
            "XuiCommonList" => Self::CommonList,
            "XuiProgressBar" => Self::ProgressBar,
            "XuiSlider" => Self::Slider,
            "XuiCaret" => Self::Caret,
            "XuiEdit" => Self::Edit,
            "XuiMessageBox" => Self::MessageBox,
            "XuiLabel" => Self::Label,
            "XuiTabScene" => Self::TabScene,
            _ => return None,
        };

        Some(builtin)
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Element => "XuiElement",
            Self::Canvas => "XuiCanvas",
            Self::Control => "XuiControl",
            Self::Scene => "XuiScene",
            Self::Button => "XuiButton",
            Self::BackButton => "XuiBackButton",
            Self::NavButton => "XuiNavButton",
            Self::Checkbox => "XuiCheckbox",
            Self::RadioButton => "XuiRadioButton",
            Self::RadioGroup => "XuiRadioGroup",
            Self::TextPresenter => "XuiTextPresenter",
            Self::HtmlPresenter => "XuiHtmlPresenter",
            Self::ImagePresenter => "XuiImagePresenter",
            Self::Image => "XuiImage",
            Self::Visual => "XuiVisual",
            Self::Group => "XuiGroup",
            Self::Sound => "XuiSound",
            Self::SoundXAudio => "XuiSoundXAudio",
            Self::Transition => "XuiTransition",
            Self::ListItem => "XuiListItem",
            Self::ScrollEnd => "XuiScrollEnd",
            Self::ScrollBar => "XuiScrollBar",
            Self::List => "XuiList",
            Self::CommonList => "XuiCommonList",
            Self::ProgressBar => "XuiProgressBar",
            Self::Slider => "XuiSlider",
            Self::Caret => "XuiCaret",
            Self::Edit => "XuiEdit",
            Self::MessageBox => "XuiMessageBox",
            Self::Label => "XuiLabel",
            Self::TabScene => "XuiTabScene",
        }
    }
}

/// Instance data of the most derived built-in class level that has any.
#[derive(Debug, Default)]
pub enum ClassState {
    #[default]
    None,
    Checkbox {
        checked: bool,
    },
    RadioGroup {
        selected: Option<ElementId>,
        next_index: u32,
    },
    TabScene(TabSceneState),
    List {
        list: Box<ListState>,
        /// XuiCommonList item store; `None` below XuiCommonList.
        common: Option<Box<CommonListState>>,
    },
    ScrollEnd {
        state: crate::message::ScrollEndState,
    },
    ScrollBar(list::ScrollBarState),
    Slider(Box<SliderState>),
    Progress(Box<ProgressState>),
    Sound(SoundState),
    /// XuiVisual and XuiCanvas skin prefix.
    Prefix(Option<alloc::rc::Rc<[u16]>>),
    TextPresenter {
        owner: Option<ElementId>,
        html_version: Option<u32>,
        html_association: Option<u32>,
        html_text: Option<alloc::rc::Rc<[u16]>>,
    },
    MessageBox(MessageBoxState),
    Transition(Box<TransitionState>),
    Edit(Box<edit::EditState>),
}

/// Per-element runtime state of the built-in class levels.
#[derive(Debug, Default)]
pub struct ElementState {
    pub control: Option<ControlState>,
    pub scene: Option<Box<SceneState>>,
    pub class: ClassState,
}

impl Context<'_, '_> {
    /// Whether any class level of the element is the built-in `builtin`.
    pub fn is_builtin_a(&self, element: ElementId, builtin: Builtin) -> bool {
        let Some(class) = self.tree.class(element) else {
            return false;
        };

        let mut level = self.registry.class(class);
        while let Some(current) = level {
            if self.runtime.builtin(current.id()) == Some(builtin) {
                return true;
            }
            level = current.base();
        }

        false
    }

    /// Runtime state of an element, created from its class chain on first
    /// use.
    pub fn state_mut(&mut self, element: ElementId) -> Option<&mut ElementState> {
        if !self.runtime.states.contains_key(&element) {
            if !self.tree.contains(element) {
                return None;
            }
            let state = self.new_state(element);
            self.runtime.states.insert(element, state);
        }

        self.runtime.states.get_mut(&element)
    }

    fn new_state(&self, element: ElementId) -> ElementState {
        let mut state = ElementState::default();
        let Some(class) = self.tree.class(element) else {
            return state;
        };

        // Most derived first: the first specific level wins.
        let mut level = self.registry.class(class);
        while let Some(current) = level {
            let builtin = self.runtime.builtin(current.id());
            match builtin {
                Some(Builtin::Control) => state.control = Some(ControlState::new()),
                Some(Builtin::Scene) => state.scene = Some(Box::new(SceneState::new())),
                _ => {}
            }

            if matches!(state.class, ClassState::None)
                && let Some(builtin) = builtin
            {
                state.class = match builtin {
                    Builtin::Checkbox | Builtin::RadioButton => {
                        ClassState::Checkbox { checked: false }
                    }
                    Builtin::RadioGroup => ClassState::RadioGroup {
                        selected: None,
                        next_index: 0,
                    },
                    Builtin::TabScene => ClassState::TabScene(TabSceneState::new()),
                    Builtin::List => ClassState::List {
                        list: Box::new(ListState::new()),
                        common: None,
                    },
                    Builtin::CommonList => ClassState::List {
                        list: Box::new(ListState::new()),
                        common: Some(Box::default()),
                    },
                    Builtin::ScrollEnd => ClassState::ScrollEnd {
                        state: crate::message::ScrollEndState::Normal,
                    },
                    Builtin::ScrollBar => ClassState::ScrollBar(list::ScrollBarState::new()),
                    Builtin::Slider => ClassState::Slider(Box::new(SliderState::new())),
                    Builtin::ProgressBar => ClassState::Progress(Box::new(ProgressState::new())),
                    Builtin::Sound | Builtin::SoundXAudio => ClassState::Sound(SoundState::new()),
                    Builtin::Visual | Builtin::Canvas => ClassState::Prefix(None),
                    Builtin::TextPresenter | Builtin::HtmlPresenter => ClassState::TextPresenter {
                        owner: None,
                        html_version: None,
                        html_association: None,
                        html_text: None,
                    },
                    Builtin::MessageBox => ClassState::MessageBox(MessageBoxState::new()),
                    Builtin::Transition => ClassState::Transition(Box::default()),
                    Builtin::Edit => ClassState::Edit(Box::new(edit::EditState::new())),
                    _ => ClassState::None,
                };
            }

            level = current.base();
        }

        state
    }

    pub fn control_state(&mut self, element: ElementId) -> Option<&mut ControlState> {
        self.state_mut(element)?.control.as_mut()
    }

    pub fn scene_state(&mut self, element: ElementId) -> Option<&mut SceneState> {
        self.state_mut(element)?.scene.as_deref_mut()
    }

    pub fn class_state(&mut self, element: ElementId) -> Option<&mut ClassState> {
        Some(&mut self.state_mut(element)?.class)
    }
}

/// Routes a message to the handler of one built-in class level.
pub(crate) fn handle(
    cx: &mut Context<'_, '_>,
    element: ElementId,
    builtin: Builtin,
    class: ClassId,
    message: &mut Message,
) {
    match builtin {
        Builtin::Element => element::on_message(cx, element, message),
        Builtin::Control => base::on_message(cx, element, class, message),
        Builtin::Button => button::on_button(cx, element, message),
        // XuiBackButton's procedure handles nothing.
        Builtin::BackButton => {}
        Builtin::NavButton => button::on_nav_button(cx, element, message),
        Builtin::Checkbox | Builtin::RadioButton => button::on_checkbox(cx, element, message),
        Builtin::RadioGroup => button::on_radio_group(cx, element, message),
        Builtin::Label | Builtin::Caret => button::on_unfocusable(message),
        Builtin::Group => button::on_group(cx, element, message),
        Builtin::Scene => scene::on_message(cx, element, message),
        Builtin::TabScene => tab_scene::on_message(cx, element, message),
        Builtin::Visual => visual::on_visual(cx, element, message),
        Builtin::Canvas => visual::on_canvas(cx, element, message),
        Builtin::Transition => visual::on_transition(cx, element, message),
        Builtin::TextPresenter | Builtin::HtmlPresenter => {
            text::on_text_presenter(cx, element, message)
        }
        Builtin::ImagePresenter => {}
        Builtin::Image => image::on_image(cx, element, message),
        Builtin::MessageBox => text::on_message_box(cx, element, message),
        Builtin::List => list::on_list(cx, element, class, message),
        Builtin::CommonList => list::on_common_list(cx, element, message),
        Builtin::ListItem => list::on_list_item(cx, element, class, message),
        Builtin::ScrollEnd => list::on_scroll_end(cx, element, message),
        Builtin::ScrollBar => list::on_scroll_bar(cx, element, class, message),
        Builtin::Slider => range::on_slider(cx, element, class, message),
        Builtin::ProgressBar => range::on_progress(cx, element, class, message),
        Builtin::Sound => sound::on_sound(cx, element, message),
        Builtin::SoundXAudio => sound::on_xaudio(cx, element, message),
        Builtin::Edit => edit::on_edit(cx, element, class, message),
    }
}

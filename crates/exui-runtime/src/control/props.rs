//! Property definitions read and written by the built-in behaviors.
//! Positions follow the class registration tables in doc/xui-classes.md.

use exui_core::schema::ClassDef;
use exui_core::schema::PropDef;
use exui_core::schema::xui;

use crate::message::NavDirection;

fn def(class: &'static ClassDef, position: usize) -> &'static PropDef {
    &class.props[position]
}

pub fn width() -> &'static PropDef {
    def(&xui::XUI_ELEMENT, 1)
}

pub fn height() -> &'static PropDef {
    def(&xui::XUI_ELEMENT, 2)
}

pub fn position() -> &'static PropDef {
    def(&xui::XUI_ELEMENT, 3)
}

pub fn scale() -> &'static PropDef {
    def(&xui::XUI_ELEMENT, 4)
}

pub fn opacity() -> &'static PropDef {
    def(&xui::XUI_ELEMENT, 6)
}

pub fn anchor() -> &'static PropDef {
    def(&xui::XUI_ELEMENT, 7)
}

pub fn pivot() -> &'static PropDef {
    def(&xui::XUI_ELEMENT, 8)
}

pub fn show() -> &'static PropDef {
    def(&xui::XUI_ELEMENT, 9)
}

pub fn disable_timeline_recursion() -> &'static PropDef {
    def(&xui::XUI_ELEMENT, 11)
}

pub fn disable_focus_recursion() -> &'static PropDef {
    def(&xui::XUI_ELEMENT, 16)
}

pub fn visual() -> &'static PropDef {
    def(&xui::XUI_CONTROL, 1)
}

pub fn enabled() -> &'static PropDef {
    def(&xui::XUI_CONTROL, 2)
}

pub fn unfocused_input() -> &'static PropDef {
    def(&xui::XUI_CONTROL, 3)
}

pub fn nav(direction: NavDirection) -> &'static PropDef {
    let position = match direction {
        NavDirection::Left => 4,
        NavDirection::Right => 5,
        NavDirection::Up => 6,
        NavDirection::Down => 7,
        NavDirection::TabForward => 8,
        NavDirection::TabBackward => 9,
    };

    def(&xui::XUI_CONTROL, position)
}

pub fn text() -> &'static PropDef {
    def(&xui::XUI_CONTROL, 10)
}

pub fn point_size() -> &'static PropDef {
    def(&xui::XUI_CONTROL, 11)
}

pub fn image_path() -> &'static PropDef {
    def(&xui::XUI_CONTROL, 12)
}

/// `XuiImage.ImagePath`.
pub fn xui_image_path() -> &'static PropDef {
    def(&xui::XUI_IMAGE, 1)
}

/// `XuiFigure.Fill`.
pub fn figure_fill() -> &'static PropDef {
    def(&xui::XUI_FIGURE, 1)
}

/// `XuiFigure.Fill.TextureFileName`.
pub fn figure_fill_texture_file_name() -> &'static PropDef {
    &figure_fill()
        .ty
        .compound()
        .expect("XuiFigure.Fill is a compound")[2]
}

pub fn default_focus() -> &'static PropDef {
    def(&xui::XUI_SCENE, 0)
}

pub fn trans_from() -> &'static PropDef {
    def(&xui::XUI_SCENE, 1)
}

pub fn trans_to() -> &'static PropDef {
    def(&xui::XUI_SCENE, 2)
}

pub fn trans_back_from() -> &'static PropDef {
    def(&xui::XUI_SCENE, 3)
}

pub fn trans_back_to() -> &'static PropDef {
    def(&xui::XUI_SCENE, 4)
}

pub fn ignore_presses() -> &'static PropDef {
    def(&xui::XUI_SCENE, 6)
}

pub fn button_press_key() -> &'static PropDef {
    def(&xui::XUI_BUTTON, 0)
}

pub fn checkbox_press_key() -> &'static PropDef {
    def(&xui::XUI_CHECKBOX, 0)
}

pub fn radio_press_key() -> &'static PropDef {
    def(&xui::XUI_RADIO_BUTTON, 0)
}

pub fn press_path() -> &'static PropDef {
    def(&xui::XUI_NAV_BUTTON, 0)
}

pub fn stay_visible() -> &'static PropDef {
    def(&xui::XUI_NAV_BUTTON, 1)
}

pub fn src_trans_index() -> &'static PropDef {
    def(&xui::XUI_NAV_BUTTON, 2)
}

pub fn dest_trans_index() -> &'static PropDef {
    def(&xui::XUI_NAV_BUTTON, 3)
}

pub fn tab_count() -> &'static PropDef {
    def(&xui::XUI_TAB_SCENE, 0)
}

pub fn tab_wrap() -> &'static PropDef {
    def(&xui::XUI_TAB_SCENE, 1)
}

pub fn user_interrupt() -> &'static PropDef {
    def(&xui::XUI_TAB_SCENE, 2)
}

pub fn vertical_tabs() -> &'static PropDef {
    def(&xui::XUI_TAB_SCENE, 3)
}

pub fn no_auto_hide() -> &'static PropDef {
    def(&xui::XUI_TAB_SCENE, 4)
}

pub fn default_tab() -> &'static PropDef {
    def(&xui::XUI_TAB_SCENE, 5)
}

pub fn text_presenter_data_association() -> &'static PropDef {
    def(&xui::XUI_TEXT_PRESENTER, 6)
}

pub fn text_presenter_style() -> &'static PropDef {
    def(&xui::XUI_TEXT_PRESENTER, 4)
}

pub fn image_presenter_data_association() -> &'static PropDef {
    def(&xui::XUI_IMAGE_PRESENTER, 1)
}

pub fn sound_state() -> &'static PropDef {
    def(&xui::XUI_SOUND, 0)
}

pub fn sound_loop() -> &'static PropDef {
    def(&xui::XUI_SOUND, 1)
}

pub fn sound_volume() -> &'static PropDef {
    def(&xui::XUI_SOUND, 3)
}

pub fn sound_file() -> &'static PropDef {
    def(&xui::XUI_SOUND_X_AUDIO, 0)
}

pub fn list_item_layout() -> &'static PropDef {
    def(&xui::XUI_LIST_ITEM, 0)
}

pub fn list_item_checkable() -> &'static PropDef {
    def(&xui::XUI_LIST_ITEM, 1)
}

pub fn list_item_selected_size() -> &'static PropDef {
    def(&xui::XUI_LIST_ITEM, 2)
}

pub fn list_item_keep_size_unfocused() -> &'static PropDef {
    def(&xui::XUI_LIST_ITEM, 3)
}

pub fn list_item_spacing() -> &'static PropDef {
    def(&xui::XUI_LIST_ITEM, 4)
}

pub fn scroll_end_direction() -> &'static PropDef {
    def(&xui::XUI_SCROLL_END, 0)
}

pub fn scroll_bar_direction() -> &'static PropDef {
    def(&xui::XUI_SCROLL_BAR, 0)
}

pub fn scroll_bar_min_thumb() -> &'static PropDef {
    def(&xui::XUI_SCROLL_BAR, 1)
}

pub fn list_wrap() -> &'static PropDef {
    def(&xui::XUI_LIST, 0)
}

pub fn items_text() -> &'static PropDef {
    def(&xui::XUI_COMMON_LIST, 0)
}

pub fn items_image() -> &'static PropDef {
    def(&xui::XUI_COMMON_LIST, 1)
}

pub fn items_nav_path() -> &'static PropDef {
    def(&xui::XUI_COMMON_LIST, 2)
}

pub fn progress_min() -> &'static PropDef {
    def(&xui::XUI_PROGRESS_BAR, 0)
}

pub fn progress_max() -> &'static PropDef {
    def(&xui::XUI_PROGRESS_BAR, 1)
}

pub fn progress_value() -> &'static PropDef {
    def(&xui::XUI_PROGRESS_BAR, 2)
}

pub fn slider_min() -> &'static PropDef {
    def(&xui::XUI_SLIDER, 0)
}

pub fn slider_max() -> &'static PropDef {
    def(&xui::XUI_SLIDER, 1)
}

pub fn slider_value() -> &'static PropDef {
    def(&xui::XUI_SLIDER, 2)
}

pub fn slider_step() -> &'static PropDef {
    def(&xui::XUI_SLIDER, 3)
}

pub fn slider_vertical() -> &'static PropDef {
    def(&xui::XUI_SLIDER, 4)
}

pub fn slider_accel_inc() -> &'static PropDef {
    def(&xui::XUI_SLIDER, 5)
}

pub fn slider_accel_time() -> &'static PropDef {
    def(&xui::XUI_SLIDER, 6)
}

pub fn edit_text_limit() -> &'static PropDef {
    def(&xui::XUI_EDIT, 0)
}

pub fn edit_allowed_chars() -> &'static PropDef {
    def(&xui::XUI_EDIT, 1)
}

pub fn edit_read_only() -> &'static PropDef {
    def(&xui::XUI_EDIT, 3)
}

pub fn edit_multiline() -> &'static PropDef {
    def(&xui::XUI_EDIT, 4)
}

pub fn edit_smooth_scroll() -> &'static PropDef {
    def(&xui::XUI_EDIT, 5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_name_the_expected_definitions() {
        let checks: [(&PropDef, &str); 21] = [
            (width(), "Width"),
            (show(), "Show"),
            (disable_focus_recursion(), "DisableFocusRecursion"),
            (visual(), "Visual"),
            (nav(NavDirection::TabBackward), "NavTabBackward"),
            (image_path(), "ImagePath"),
            (ignore_presses(), "IgnorePresses"),
            (trans_back_to(), "TransBackTo"),
            (dest_trans_index(), "DestTransIndex"),
            (default_tab(), "DefaultTab"),
            (text_presenter_data_association(), "DataAssociation"),
            (image_presenter_data_association(), "DataAssociation"),
            (sound_volume(), "Volume"),
            (sound_file(), "File"),
            (list_item_spacing(), "InterItemSpacing"),
            (scroll_bar_min_thumb(), "MinThumbSize"),
            (items_nav_path(), "ItemsNavPath"),
            (progress_value(), "Value"),
            (slider_accel_time(), "AccelTime"),
            (edit_smooth_scroll(), "SmoothScroll"),
            (text_presenter_style(), "TextStyle"),
        ];

        for (def, name) in checks {
            assert_eq!(def.name, name);
        }
    }
}

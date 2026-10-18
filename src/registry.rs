use exui_core::RegisterError;
use exui_core::Registry;
use exui_core::schema::dash;
use exui_core::schema::xui;

/// Classes registered by dash 6717: the XUI built-ins, the XAudio and HTML
/// groups and the dashboard classes.
pub fn dash_6717() -> Result<Registry, RegisterError> {
    let mut registry = Registry::new();
    xui::register_sound_xaudio(&mut registry)?;
    xui::register_html(&mut registry)?;
    dash::register(&mut registry)?;

    Ok(registry)
}

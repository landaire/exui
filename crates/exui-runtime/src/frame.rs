use core::time::Duration;

use xbox360_draw::draw::DrawList;
use xbox360_draw::texture::TextureStore;

use crate::error::FrameError;
use crate::input::Keystroke;

/// Content driven by a platform shell once per displayed frame.
pub trait Frame {
    /// Advances the simulation by `dt` of wall time. `input` holds the
    /// keystrokes received since the previous call, in order.
    fn update(&mut self, dt: Duration, input: &[Keystroke]);

    /// Appends the frame to `list`, which the shell empties before each
    /// call. Textures referenced by the list must come from `textures`.
    fn draw(
        &mut self,
        textures: &mut dyn TextureStore,
        list: &mut DrawList,
    ) -> Result<(), FrameError>;
}

//! The XUI runtime of one display: registry, resources, fonts, the element
//! tree under the root canvas, and drawing.

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::time::Duration;

use exui_core::Registry;
use xbox360_draw::draw::DrawList;
use xbox360_draw::draw::Rgba8;
use xbox360_draw::texture::TextureStore;

use crate::control::RuntimePresenterSource;
use crate::error::FrameError;
use crate::error::NavError;
use crate::frame::Frame;
use crate::input::Keystroke;
use crate::math::Matrix4;
use crate::message::Context;
use crate::message::Runtime;
use crate::resource::ResourceProvider;
use crate::resource::Resources;
use crate::scene;
use crate::scene::ElementId;
use crate::scene::ElementTree;
use crate::scene::LoadError;
use crate::scene::RenderContext;
use crate::scene::RenderScratch;
use crate::text::FontLibrary;

/// Back buffer size in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplaySize {
    pub width: u32,
    pub height: u32,
}

/// XUI runtime: owns the class registry, resources, the element tree whose
/// root is the main XuiCanvas and the runtime state, fonts included.
pub struct Ui<'p> {
    registry: Registry,
    resources: Resources<'p>,
    tree: ElementTree,
    root: ElementId,
    display: DisplaySize,
    view: Matrix4,
    clear_color: Option<Rgba8>,

    runtime: Runtime<'p>,
    presenters: RuntimePresenterSource,
    render_scratch: RenderScratch,
    /// Keystrokes not yet dispatched; one is handled per update.
    keystrokes: VecDeque<Keystroke>,
}

impl<'p> Ui<'p> {
    /// `registry` must hold every class the scenes use; it always holds the
    /// XUI built-ins. The view transform starts as the identity and the
    /// target is not cleared.
    pub fn new(
        registry: Registry,
        provider: &'p dyn ResourceProvider,
        display: DisplaySize,
    ) -> Self {
        let mut tree = ElementTree::new();
        let canvas = registry
            .find("XuiCanvas")
            .and_then(|id| registry.class(id))
            .expect("Registry::new registers XuiCanvas");
        let root = tree.create(canvas);

        let runtime = Runtime::new(&registry);
        let mut ui = Self {
            registry,
            resources: Resources::new(provider),
            tree,
            root,
            display,
            view: Matrix4::IDENTITY,
            clear_color: None,
            runtime,
            presenters: RuntimePresenterSource::new(),
            render_scratch: RenderScratch::default(),
            keystrokes: VecDeque::new(),
        };
        ui.set_display_size(display);

        ui
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn tree(&self) -> &ElementTree {
        &self.tree
    }

    pub fn tree_mut(&mut self) -> &mut ElementTree {
        &mut self.tree
    }

    pub fn resources(&self) -> &Resources<'p> {
        &self.resources
    }

    pub fn resources_mut(&mut self) -> &mut Resources<'p> {
        &mut self.resources
    }

    pub fn fonts_mut(&mut self) -> &mut FontLibrary {
        &mut self.runtime.fonts
    }

    pub fn runtime(&self) -> &Runtime<'p> {
        &self.runtime
    }

    pub fn runtime_mut(&mut self) -> &mut Runtime<'p> {
        &mut self.runtime
    }

    /// Message dispatch, focus, timelines and navigation over the tree.
    pub fn context(&mut self) -> Context<'_, 'p> {
        Context::new(
            &mut self.tree,
            &self.registry,
            &self.resources,
            &mut self.runtime,
        )
    }

    /// The main canvas (`XuiApp_CreateMainCanvas`).
    pub fn root(&self) -> ElementId {
        self.root
    }

    pub fn display(&self) -> DisplaySize {
        self.display
    }

    /// Sets the back buffer size. The root canvas takes the back buffer
    /// size [INFERENCE: render context slot 0xE0]; XuiCanvas does not lay
    /// out its children, so scenes keep their authored size.
    pub fn set_display_size(&mut self, display: DisplaySize) {
        self.display = display;
        self.tree
            .set_bounds(self.root, display.width as f32, display.height as f32);
    }

    /// The DC view transform from root canvas units to target pixels.
    pub fn set_view(&mut self, view: Matrix4) {
        self.view = view;
    }

    /// Color the target is cleared to before drawing; `None` leaves the
    /// list's clear color unchanged.
    pub fn set_clear_color(&mut self, color: Option<Rgba8>) {
        self.clear_color = color;
    }

    /// Loads `file` (a locator or a name relative to `base_path`, such as
    /// `section://0,scenes#`) and returns the instantiated document root,
    /// detached, with timelines autoplaying and the init pass run. The root
    /// is kept as is; use [`Ui::create_scene`] for `XuiSceneCreate`, which
    /// returns the scene inside the root.
    pub fn load_scene(&mut self, base_path: &str, file: &str) -> Result<ElementId, LoadError> {
        let base: Vec<u16> = base_path.encode_utf16().collect();
        let file: Vec<u16> = file.encode_utf16().collect();
        let base = self.tree.add_base_path(&base);

        self.context().load_document(Some(base), &file)
    }

    /// `XuiSceneCreate`: loads `file` relative to `base_path` and returns
    /// the first child of its root that is a scene, detached and
    /// initialized; the file's root element is discarded. Attach it with
    /// [`Context::navigate_first`].
    pub fn create_scene(&mut self, base_path: &str, file: &str) -> Result<ElementId, NavError> {
        let base: Vec<u16> = base_path.encode_utf16().collect();
        let file: Vec<u16> = file.encode_utf16().collect();
        let base = self.tree.add_base_path(&base);

        self.context().create_scene(Some(base), &file)
    }

    /// Queues keystrokes; [`Ui::update`] dispatches one per call, as the
    /// XUI application loop polls one keystroke per frame.
    pub fn queue_keystrokes(&mut self, keystrokes: &[Keystroke]) {
        self.keystrokes.extend(keystrokes.iter().copied());
    }

    /// Runs one frame: one keystroke, the timers, then every timeline
    /// advanced by `dt`. The caller applies any frame time clamp.
    pub fn update(&mut self, dt: Duration) {
        let dt_ms = (dt.as_secs_f64() * 1000.0) as f32;
        let keystroke = self.keystrokes.pop_front();

        let mut cx = Context::new(
            &mut self.tree,
            &self.registry,
            &self.resources,
            &mut self.runtime,
        );
        cx.runtime.timers.advance_clock(dt_ms);
        if let Some(keystroke) = keystroke {
            cx.process_keystroke(&keystroke);
        }
        cx.process_timers();
        cx.tick_timelines(dt_ms);
        cx.flush_size_changes();
    }

    /// Draws the root canvas into `list`.
    pub fn draw(
        &mut self,
        textures: &mut dyn TextureStore,
        list: &mut DrawList,
    ) -> Result<(), FrameError> {
        if let Some(color) = self.clear_color {
            list.set_clear_color(Some(color));
        }

        let released = self.runtime.texture_releases.take().into_iter();
        for texture in released.chain(self.resources.texture_releases().take()) {
            // The backend may already have dropped it; nothing is left to free.
            let _ = textures.destroy_texture(texture);
        }

        // Controls create their visuals on the first render that reaches
        // them, and presenters poll their source on every render.
        let mut cx = Context::new(
            &mut self.tree,
            &self.registry,
            &self.resources,
            &mut self.runtime,
        );
        cx.prepare_visuals(self.root);
        cx.flush_size_changes();
        cx.pull_presenters(self.root, &mut self.presenters);

        let view = self.view;
        let mut context = RenderContext {
            resources: &mut self.resources,
            fonts: &self.runtime.fonts,
            text: &mut self.runtime.text,
            textures: &mut *textures,
            presenters: &mut self.presenters,
        };
        scene::render(
            &mut self.tree,
            self.root,
            view,
            &mut context,
            &mut self.render_scratch,
            list,
        )?;
        self.runtime.text.flush(textures)?;

        Ok(())
    }

    /// Destroys every texture the UI created in `textures`, the store it
    /// draws with. A later draw loads resource textures and glyphs again;
    /// texture objects uploaded before the release draw nothing.
    pub fn release_textures(&mut self, textures: &mut dyn TextureStore) {
        let mut released = Vec::new();
        self.runtime.texture_releases.take_all(&mut released);
        self.resources.release_textures(&mut released);
        self.tree.release_textures(&mut released);
        self.runtime.text.release_textures(&mut released);

        for texture in released {
            // The backend may already have dropped it; nothing is left to free.
            let _ = textures.destroy_texture(texture);
        }
    }
}

impl Frame for Ui<'_> {
    fn update(&mut self, dt: Duration, input: &[Keystroke]) {
        self.queue_keystrokes(input);
        Ui::update(self, dt);
    }

    fn draw(
        &mut self,
        textures: &mut dyn TextureStore,
        list: &mut DrawList,
    ) -> Result<(), FrameError> {
        Ui::draw(self, textures, list)
    }
}

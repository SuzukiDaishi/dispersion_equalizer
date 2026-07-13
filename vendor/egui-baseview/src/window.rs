use std::cell::RefCell;
use std::time::Instant;

use baseview::dpi::{PhysicalSize, Size};
use baseview::{
    Event, EventStatus, MouseCursor, Window, WindowContext, WindowHandle, WindowHandler,
    WindowOpenOptions, WindowScalePolicy, WindowSize,
};
use copypasta::ClipboardProvider;
use egui::{pos2, vec2, Pos2, Rect, Rgba, ViewportCommand};
use keyboard_types::Modifiers;
use raw_window_handle::HasWindowHandle;

use crate::{renderer::Renderer, GraphicsConfig};

pub struct Queue<'a> {
    bg_color: &'a mut Rgba,
    close_requested: &'a mut bool,
    resize_request: &'a mut Option<Size>,
}

impl<'a> Queue<'a> {
    pub(crate) fn new(
        bg_color: &'a mut Rgba,
        close_requested: &'a mut bool,
        resize_request: &'a mut Option<Size>,
    ) -> Self {
        Self {
            bg_color,
            close_requested,
            resize_request,
        }
    }

    /// Set the background color.
    pub fn bg_color(&mut self, bg_color: Rgba) {
        *self.bg_color = bg_color;
    }

    /// Set size of the window. The size can be given in either physical or logical
    /// pixels (see [`baseview::dpi`]).
    ///
    /// The resize is deferred until after the current callback returns, and the new
    /// size takes effect through the platform's resize notification.
    pub fn resize(&mut self, size: impl Into<Size>) {
        *self.resize_request = Some(size.into());
    }

    /// Close the window.
    pub fn close_window(&mut self) {
        *self.close_requested = true;
    }
}

struct OpenSettings {
    scale_policy: WindowScalePolicy,
    title: String,
}

impl OpenSettings {
    fn new(settings: &WindowOpenOptions) -> Self {
        Self {
            scale_policy: settings.scale,
            title: settings.title.clone(),
        }
    }
}

/// The mutable state of the egui window. `baseview`'s `WindowHandler` methods take
/// `&self`, so all of this lives behind a `RefCell` in [`EguiWindow`].
struct Inner<State, U>
where
    State: 'static + Send,
    U: FnMut(&egui::Context, &mut Queue, &mut State),
    U: 'static + Send,
{
    user_state: Option<State>,
    user_update: U,

    egui_ctx: egui::Context,
    viewport_id: egui::ViewportId,
    start_time: Instant,
    egui_input: egui::RawInput,
    pointer_pos_in_points: Option<egui::Pos2>,
    current_cursor_icon: MouseCursor,

    renderer: Renderer,

    clipboard_ctx: Option<copypasta::ClipboardContext>,

    physical_size: PhysicalSize<u32>,
    pixels_per_point: f32,
    points_per_pixel: f32,
    bg_color: Rgba,
    repaint_after: Option<Instant>,
}

/// Handles an egui-baseview application
pub struct EguiWindow<State, U>
where
    State: 'static + Send,
    U: FnMut(&egui::Context, &mut Queue, &mut State),
    U: 'static + Send,
{
    window_ctx: WindowContext,
    scale_policy: WindowScalePolicy,
    inner: RefCell<Inner<State, U>>,
}

impl<State, U> EguiWindow<State, U>
where
    State: 'static + Send,
    U: FnMut(&egui::Context, &mut Queue, &mut State),
    U: 'static + Send,
{
    fn new<B>(
        window_ctx: WindowContext,
        open_settings: OpenSettings,
        graphics_config: GraphicsConfig,
        mut build: B,
        update: U,
        mut state: State,
    ) -> EguiWindow<State, U>
    where
        B: FnMut(&egui::Context, &mut Queue, &mut State),
        B: 'static + Send,
    {
        let renderer = Renderer::new(&window_ctx, graphics_config).unwrap_or_else(|err| {
            // TODO: better error log and not panicking, but that's gonna require baseview changes
            log::error!("oops! the gpu backend couldn't initialize! \n {err}");
            panic!("gpu backend failed to initialize: \n {err}")
        });
        let egui_ctx = egui::Context::default();

        let window_size = window_ctx.size();
        let pixels_per_point = match open_settings.scale_policy {
            WindowScalePolicy::ScaleFactor(scale) => scale,
            WindowScalePolicy::SystemScaleFactor => window_size.scale_factor,
        } as f32;
        let points_per_pixel = pixels_per_point.recip();

        let physical_size = window_size.physical;
        let screen_rect = calculate_screen_rect(physical_size, points_per_pixel);

        let viewport_info = egui::ViewportInfo {
            parent: None,
            title: Some(open_settings.title),
            native_pixels_per_point: Some(pixels_per_point),
            focused: Some(true),
            inner_rect: Some(screen_rect),
            ..Default::default()
        };
        let viewport_id = egui::ViewportId::default();

        let mut egui_input = egui::RawInput {
            max_texture_side: Some(renderer.max_texture_side()),
            screen_rect: Some(screen_rect),
            ..Default::default()
        };
        let _ = egui_input.viewports.insert(viewport_id, viewport_info);

        let mut bg_color = Rgba::BLACK;
        let mut close_requested = false;
        let mut resize_request: Option<Size> = None;
        let mut queue = Queue::new(&mut bg_color, &mut close_requested, &mut resize_request);
        (build)(&egui_ctx, &mut queue, &mut state);

        if let Some(size) = resize_request {
            window_ctx.resize(size);
        }
        if close_requested {
            window_ctx.request_close();
        }

        let clipboard_ctx = match copypasta::ClipboardContext::new() {
            Ok(clipboard_ctx) => Some(clipboard_ctx),
            Err(e) => {
                log::error!("Failed to initialize clipboard: {}", e);
                None
            }
        };

        let start_time = Instant::now();

        Self {
            window_ctx,
            scale_policy: open_settings.scale_policy,
            inner: RefCell::new(Inner {
                user_state: Some(state),
                user_update: update,

                egui_ctx,
                viewport_id,
                start_time,
                egui_input,
                pointer_pos_in_points: None,
                current_cursor_icon: MouseCursor::Default,

                renderer,

                clipboard_ctx,

                physical_size,
                pixels_per_point,
                points_per_pixel,
                bg_color,
                repaint_after: Some(start_time),
            }),
        }
    }

    /// Open a new child window.
    ///
    /// * `parent` - The parent window.
    /// * `settings` - The settings of the window.
    /// * `state` - The initial state of your application.
    /// * `build` - Called once before the first frame. Allows you to do setup code and to
    ///   call `ctx.set_fonts()`. Optional.
    /// * `update` - Called before each frame. Here you should update the state of your
    ///   application and build the UI.
    pub fn open_parented<P, B>(
        parent: &P,
        #[allow(unused_mut)] mut settings: WindowOpenOptions,
        graphics_config: GraphicsConfig,
        state: State,
        build: B,
        update: U,
    ) -> WindowHandle
    where
        P: HasWindowHandle,
        B: FnMut(&egui::Context, &mut Queue, &mut State),
        B: 'static + Send,
    {
        #[cfg(feature = "opengl")]
        if settings.gl_config.is_none() {
            settings.gl_config = Some(Default::default());
        }

        let open_settings = OpenSettings::new(&settings);

        Window::open_parented(
            parent,
            settings,
            move |window_ctx: WindowContext| -> EguiWindow<State, U> {
                EguiWindow::new(
                    window_ctx,
                    open_settings,
                    graphics_config,
                    build,
                    update,
                    state,
                )
            },
        )
    }

    /// Open a new window that blocks the current thread until the window is destroyed.
    ///
    /// * `settings` - The settings of the window.
    /// * `state` - The initial state of your application.
    /// * `build` - Called once before the first frame. Allows you to do setup code and to
    ///   call `ctx.set_fonts()`. Optional.
    /// * `update` - Called before each frame. Here you should update the state of your
    ///   application and build the UI.
    pub fn open_blocking<B>(
        #[allow(unused_mut)] mut settings: WindowOpenOptions,
        graphics_config: GraphicsConfig,
        state: State,
        build: B,
        update: U,
    ) where
        B: FnMut(&egui::Context, &mut Queue, &mut State),
        B: 'static + Send,
    {
        #[cfg(feature = "opengl")]
        if settings.gl_config.is_none() {
            settings.gl_config = Some(Default::default());
        }

        let open_settings = OpenSettings::new(&settings);

        Window::open_blocking(
            settings,
            move |window_ctx: WindowContext| -> EguiWindow<State, U> {
                EguiWindow::new(
                    window_ctx,
                    open_settings,
                    graphics_config,
                    build,
                    update,
                    state,
                )
            },
        )
    }
}

impl<State, U> Inner<State, U>
where
    State: 'static + Send,
    U: FnMut(&egui::Context, &mut Queue, &mut State),
    U: 'static + Send,
{
    /// Update the pressed key modifiers when a mouse event has sent a new set of modifiers.
    fn update_modifiers(&mut self, modifiers: &Modifiers) {
        self.egui_input.modifiers.alt = !(*modifiers & Modifiers::ALT).is_empty();
        self.egui_input.modifiers.shift = !(*modifiers & Modifiers::SHIFT).is_empty();
        self.egui_input.modifiers.command = !(*modifiers & Modifiers::CONTROL).is_empty();
    }
}

impl<State, U> WindowHandler for EguiWindow<State, U>
where
    State: 'static + Send,
    U: FnMut(&egui::Context, &mut Queue, &mut State),
    U: 'static + Send,
{
    fn on_frame(&self) {
        // Window operations are deferred until the `RefCell` borrow is released: on some
        // platforms (Win32 in particular) resizing or focusing the window synchronously
        // re-enters the event handler, which would panic while `inner` is borrowed.
        let mut close_requested = false;
        let mut resize_request: Option<Size> = None;
        #[cfg_attr(
            not(all(feature = "windows_keyboard_workaround", target_os = "windows")),
            allow(unused_mut)
        )]
        let mut focus_requested = false;

        {
            let inner = &mut *self.inner.borrow_mut();
            let Some(state) = &mut inner.user_state else {
                return;
            };

            inner.egui_input.time = Some(inner.start_time.elapsed().as_secs_f64());
            inner.egui_input.screen_rect = Some(calculate_screen_rect(
                inner.physical_size,
                inner.points_per_pixel,
            ));

            inner.egui_ctx.begin_pass(inner.egui_input.take());

            let mut queue = Queue::new(
                &mut inner.bg_color,
                &mut close_requested,
                &mut resize_request,
            );

            (inner.user_update)(&inner.egui_ctx, &mut queue, state);

            // Prevent data from being allocated every frame by storing this
            // in a member field.
            let mut full_output = inner.egui_ctx.end_pass();

            let Some(viewport_output) = full_output.viewport_output.get(&inner.viewport_id) else {
                // The main window was closed by egui.
                self.window_ctx.request_close();
                return;
            };

            for command in viewport_output.commands.iter() {
                match command {
                    ViewportCommand::Close => {
                        close_requested = true;
                    }
                    ViewportCommand::InnerSize(size) => {
                        resize_request = Some(
                            baseview::dpi::LogicalSize::new(
                                f64::from(size.x.max(1.0)),
                                f64::from(size.y.max(1.0)),
                            )
                            .into(),
                        );
                    }
                    _ => {}
                }
            }

            let now = Instant::now();
            let do_repaint_now = if let Some(t) = inner.repaint_after {
                now >= t || viewport_output.repaint_delay.is_zero()
            } else {
                viewport_output.repaint_delay.is_zero()
            };

            if do_repaint_now {
                let physical_size = inner.physical_size;
                let pixels_per_point = inner.pixels_per_point;
                let bg_color = inner.bg_color;
                inner.renderer.render(
                    &self.window_ctx,
                    bg_color,
                    physical_size,
                    pixels_per_point,
                    &mut inner.egui_ctx,
                    &mut full_output,
                );

                inner.repaint_after = None;
            } else if let Some(repaint_after) = now.checked_add(viewport_output.repaint_delay) {
                // Schedule to repaint after the requested time has elapsed.
                inner.repaint_after = Some(repaint_after);
            }

            for command in full_output.platform_output.commands {
                match command {
                    egui::OutputCommand::CopyText(text) => {
                        if let Some(clipboard_ctx) = &mut inner.clipboard_ctx {
                            if let Err(err) = clipboard_ctx.set_contents(text) {
                                log::error!("Copy/Cut error: {}", err);
                            }
                        }
                    }
                    egui::OutputCommand::CopyImage(_) => {
                        log::warn!("Copying images is not supported in egui_baseview.");
                    }
                    egui::OutputCommand::OpenUrl(open_url) => {
                        if let Err(err) = open::that_detached(&open_url.url) {
                            log::error!("Open error: {}", err);
                        }
                    }
                }
            }

            let cursor_icon =
                crate::translate::translate_cursor_icon(full_output.platform_output.cursor_icon);
            if inner.current_cursor_icon != cursor_icon {
                inner.current_cursor_icon = cursor_icon;
                self.window_ctx.set_mouse_cursor(cursor_icon);
            }

            // A temporary workaround for keyboard input not working sometimes in Windows.
            // See https://github.com/BillyDM/egui-baseview/issues/20
            #[cfg(all(feature = "windows_keyboard_workaround", target_os = "windows"))]
            {
                if !full_output.platform_output.events.is_empty()
                    || full_output.platform_output.ime.is_some()
                {
                    focus_requested = true;
                }
            }
        }

        if let Some(size) = resize_request {
            self.window_ctx.resize(size);
        }
        if close_requested {
            self.window_ctx.request_close();
        }
        if focus_requested {
            self.window_ctx.focus();
        }
    }

    fn resized(&self, new_size: WindowSize) {
        let inner = &mut *self.inner.borrow_mut();

        inner.pixels_per_point = match self.scale_policy {
            WindowScalePolicy::ScaleFactor(scale) => scale,
            WindowScalePolicy::SystemScaleFactor => new_size.scale_factor,
        } as f32;
        inner.points_per_pixel = inner.pixels_per_point.recip();

        inner.physical_size = new_size.physical;

        let screen_rect = calculate_screen_rect(inner.physical_size, inner.points_per_pixel);

        inner.egui_input.screen_rect = Some(screen_rect);

        let viewport_info = inner
            .egui_input
            .viewports
            .get_mut(&inner.viewport_id)
            .unwrap();
        viewport_info.native_pixels_per_point = Some(inner.pixels_per_point);
        viewport_info.inner_rect = Some(screen_rect);

        // Schedule to repaint on the next frame.
        inner.repaint_after = Some(Instant::now());
    }

    fn on_event(&self, event: Event) -> EventStatus {
        let inner = &mut *self.inner.borrow_mut();

        match &event {
            baseview::Event::Mouse(event) => match event {
                baseview::MouseEvent::CursorMoved {
                    position,
                    modifiers,
                } => {
                    inner.update_modifiers(modifiers);

                    // `baseview` reports mouse positions in physical pixels; egui expects
                    // logical points.
                    let pos = pos2(
                        position.x as f32 * inner.points_per_pixel,
                        position.y as f32 * inner.points_per_pixel,
                    );
                    inner.pointer_pos_in_points = Some(pos);
                    inner.egui_input.events.push(egui::Event::PointerMoved(pos));
                }
                baseview::MouseEvent::ButtonPressed { button, modifiers } => {
                    inner.update_modifiers(modifiers);

                    if let Some(pos) = inner.pointer_pos_in_points {
                        if let Some(button) = crate::translate::translate_mouse_button(*button) {
                            let modifiers = inner.egui_input.modifiers;
                            inner.egui_input.events.push(egui::Event::PointerButton {
                                pos,
                                button,
                                pressed: true,
                                modifiers,
                            });
                        }
                    }
                }
                baseview::MouseEvent::ButtonReleased { button, modifiers } => {
                    inner.update_modifiers(modifiers);

                    if let Some(pos) = inner.pointer_pos_in_points {
                        if let Some(button) = crate::translate::translate_mouse_button(*button) {
                            let modifiers = inner.egui_input.modifiers;
                            inner.egui_input.events.push(egui::Event::PointerButton {
                                pos,
                                button,
                                pressed: false,
                                modifiers,
                            });
                        }
                    }
                }
                baseview::MouseEvent::WheelScrolled {
                    delta: scroll_delta,
                    modifiers,
                } => {
                    inner.update_modifiers(modifiers);

                    #[allow(unused_mut)]
                    let (unit, mut delta) = match scroll_delta {
                        baseview::ScrollDelta::Lines { x, y } => {
                            (egui::MouseWheelUnit::Line, egui::vec2(*x, *y))
                        }

                        baseview::ScrollDelta::Pixels { x, y } => (
                            egui::MouseWheelUnit::Point,
                            egui::vec2(*x, *y) * inner.points_per_pixel,
                        ),
                    };

                    if cfg!(target_os = "macos") {
                        // This is still buggy in winit despite
                        // https://github.com/rust-windowing/winit/issues/1695 being closed
                        //
                        // TODO: See if this is an issue in baseview as well.
                        delta.x *= -1.0;
                    }

                    let modifiers = inner.egui_input.modifiers;
                    inner.egui_input.events.push(egui::Event::MouseWheel {
                        unit,
                        delta,
                        modifiers,
                    });
                }
                baseview::MouseEvent::CursorLeft => {
                    inner.pointer_pos_in_points = None;
                    inner.egui_input.events.push(egui::Event::PointerGone);
                }
                _ => {}
            },
            baseview::Event::Keyboard(event) => {
                use keyboard_types::Code;

                let pressed = event.state == keyboard_types::KeyState::Down;

                match event.code {
                    Code::ShiftLeft | Code::ShiftRight => {
                        inner.egui_input.modifiers.shift = pressed
                    }
                    Code::ControlLeft | Code::ControlRight => {
                        inner.egui_input.modifiers.ctrl = pressed;

                        #[cfg(not(target_os = "macos"))]
                        {
                            inner.egui_input.modifiers.command = pressed;
                        }
                    }
                    Code::AltLeft | Code::AltRight => inner.egui_input.modifiers.alt = pressed,
                    Code::MetaLeft | Code::MetaRight => {
                        #[cfg(target_os = "macos")]
                        {
                            inner.egui_input.modifiers.mac_cmd = pressed;
                            inner.egui_input.modifiers.command = pressed;
                        }
                        // prevent `rustfmt` from breaking this
                    }
                    _ => (),
                }

                if let Some(key) = crate::translate::translate_virtual_key(&event.key) {
                    let modifiers = inner.egui_input.modifiers;
                    inner.egui_input.events.push(egui::Event::Key {
                        key,
                        physical_key: None,
                        pressed,
                        repeat: event.repeat,
                        modifiers,
                    });
                }

                if pressed {
                    // VirtualKeyCode::Paste etc in winit are broken/untrustworthy,
                    // so we detect these things manually:
                    //
                    // TODO: See if this is an issue in baseview as well.
                    if is_cut_command(inner.egui_input.modifiers, event.code) {
                        inner.egui_input.events.push(egui::Event::Cut);
                    } else if is_copy_command(inner.egui_input.modifiers, event.code) {
                        inner.egui_input.events.push(egui::Event::Copy);
                    } else if is_paste_command(inner.egui_input.modifiers, event.code) {
                        if let Some(clipboard_ctx) = &mut inner.clipboard_ctx {
                            match clipboard_ctx.get_contents() {
                                Ok(contents) => {
                                    inner.egui_input.events.push(egui::Event::Text(contents))
                                }
                                Err(err) => {
                                    log::error!("Paste error: {}", err);
                                }
                            }
                        }
                    } else if let keyboard_types::Key::Character(written) = &event.key {
                        if !inner.egui_input.modifiers.ctrl && !inner.egui_input.modifiers.command {
                            inner
                                .egui_input
                                .events
                                .push(egui::Event::Text(written.to_string()));
                        }
                    }
                }
            }
            baseview::Event::Window(event) => match event {
                baseview::WindowEvent::Focused => {
                    inner
                        .egui_input
                        .events
                        .push(egui::Event::WindowFocused(true));
                    inner
                        .egui_input
                        .viewports
                        .get_mut(&inner.viewport_id)
                        .unwrap()
                        .focused = Some(true);
                }
                baseview::WindowEvent::Unfocused => {
                    inner
                        .egui_input
                        .events
                        .push(egui::Event::WindowFocused(false));
                    inner
                        .egui_input
                        .viewports
                        .get_mut(&inner.viewport_id)
                        .unwrap()
                        .focused = Some(false);
                }
                baseview::WindowEvent::WillClose => {}
                _ => {}
            },
            _ => {}
        }

        EventStatus::Captured
    }
}

fn is_cut_command(modifiers: egui::Modifiers, keycode: keyboard_types::Code) -> bool {
    (modifiers.command && keycode == keyboard_types::Code::KeyX)
        || (cfg!(target_os = "windows")
            && modifiers.shift
            && keycode == keyboard_types::Code::Delete)
}

fn is_copy_command(modifiers: egui::Modifiers, keycode: keyboard_types::Code) -> bool {
    (modifiers.command && keycode == keyboard_types::Code::KeyC)
        || (cfg!(target_os = "windows")
            && modifiers.ctrl
            && keycode == keyboard_types::Code::Insert)
}

fn is_paste_command(modifiers: egui::Modifiers, keycode: keyboard_types::Code) -> bool {
    (modifiers.command && keycode == keyboard_types::Code::KeyV)
        || (cfg!(target_os = "windows")
            && modifiers.shift
            && keycode == keyboard_types::Code::Insert)
}

/// Calculate screen rectangle in logical size.
fn calculate_screen_rect(physical_size: PhysicalSize<u32>, points_per_pixel: f32) -> Rect {
    let logical_size = (
        physical_size.width as f32 * points_per_pixel,
        physical_size.height as f32 * points_per_pixel,
    );
    Rect::from_min_size(Pos2::new(0f32, 0f32), vec2(logical_size.0, logical_size.1))
}

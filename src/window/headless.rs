#![allow(unsafe_code)]

use crate::core::Context;
use crate::WindowError;
use glutin::config::{ConfigSurfaceTypes, ConfigTemplateBuilder};
use glutin::context::{ContextAttributesBuilder, NotCurrentGlContextSurfaceAccessor};
use glutin::display::GlDisplay;
use glutin::surface::{PbufferSurface, Surface, SurfaceAttributesBuilder};
use raw_window_handle::HasRawDisplayHandle;
#[cfg(target_os = "windows")]
use raw_window_handle::HasRawWindowHandle;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;
use winit::event_loop::EventLoop;

/// An offscreen graphics context that keeps its GL surface alive across clones.
#[derive(Clone)]
pub struct HeadlessContext {
    context: Context,
    _native: Rc<NativeContext>,
}

#[allow(dead_code)]
enum NativeContext {
    Pbuffer(
        glutin::context::PossiblyCurrentContext,
        Surface<PbufferSurface>,
        Option<EventLoop<()>>,
    ),
    #[cfg(target_os = "windows")]
    HiddenWindow(
        glutin::context::PossiblyCurrentContext,
        Surface<glutin::surface::WindowSurface>,
        winit::window::Window,
        EventLoop<()>,
    ),
}

impl HeadlessContext {
    /// Creates a native OpenGL context with an offscreen pbuffer surface.
    pub fn new() -> Result<Self, WindowError> {
        #[cfg(target_os = "windows")]
        return Self::from_hidden_window();

        #[cfg(not(target_os = "windows"))]
        Self::from_pbuffer()
    }

    #[cfg(not(target_os = "windows"))]
    fn from_pbuffer() -> Result<Self, WindowError> {
        #[cfg(target_os = "linux")]
        let device_display = glutin::api::egl::device::Device::query_devices()
            .ok()
            .and_then(|mut devices| {
                devices.find_map(|device| unsafe {
                    glutin::api::egl::display::Display::with_device(&device, None).ok()
                })
            })
            .map(glutin::display::Display::Egl);
        #[cfg(not(target_os = "linux"))]
        let device_display: Option<glutin::display::Display> = None;

        let (display, event_loop) = if let Some(display) = device_display {
            (display, None)
        } else {
            let event_loop = new_event_loop();
            #[cfg(target_os = "linux")]
            let preference = glutin::display::DisplayApiPreference::EglThenGlx(Box::new(
                winit::platform::x11::register_xlib_error_hook,
            ));
            #[cfg(target_os = "macos")]
            let preference = glutin::display::DisplayApiPreference::Cgl;
            (
                unsafe {
                    glutin::display::Display::new(event_loop.raw_display_handle(), preference)?
                },
                Some(event_loop),
            )
        };

        let template = ConfigTemplateBuilder::new()
            .with_surface_type(ConfigSurfaceTypes::PBUFFER)
            .build();
        let config = unsafe { display.find_configs(template)? }
            .next()
            .ok_or(WindowError::SurfaceCreationError)?;
        let attributes = ContextAttributesBuilder::new().build(None);
        let surface_attributes = SurfaceAttributesBuilder::<PbufferSurface>::new().build(
            std::num::NonZeroU32::new(1).unwrap(),
            std::num::NonZeroU32::new(1).unwrap(),
        );
        let gl_context = unsafe { display.create_context(&config, &attributes)? };
        let surface = unsafe { display.create_pbuffer_surface(&config, &surface_attributes)? };
        let gl_context = gl_context.make_current(&surface)?;
        let context = Context::from_gl_context(Arc::new(unsafe {
            crate::context::Context::from_loader_function(|symbol| {
                let symbol = std::ffi::CString::new(symbol).unwrap();
                display.get_proc_address(&symbol)
            })
        }))?;

        Ok(Self {
            context,
            _native: Rc::new(NativeContext::Pbuffer(gl_context, surface, event_loop)),
        })
    }

    #[cfg(target_os = "windows")]
    fn from_hidden_window() -> Result<Self, WindowError> {
        use glutin::surface::WindowSurface;

        let event_loop = new_event_loop();
        let window = winit::window::WindowBuilder::new()
            .with_visible(false)
            .with_inner_size(winit::dpi::PhysicalSize::new(1, 1))
            .build(&event_loop)?;
        let raw_window = window.raw_window_handle();
        let display = unsafe {
            glutin::display::Display::new(
                window.raw_display_handle(),
                glutin::display::DisplayApiPreference::WglThenEgl(Some(raw_window)),
            )?
        };
        let template = ConfigTemplateBuilder::new()
            .with_surface_type(ConfigSurfaceTypes::WINDOW)
            .compatible_with_native_window(raw_window)
            .build();
        let config = unsafe { display.find_configs(template)? }
            .next()
            .ok_or(WindowError::SurfaceCreationError)?;
        let attributes = ContextAttributesBuilder::new().build(Some(raw_window));
        let surface_attributes = SurfaceAttributesBuilder::<WindowSurface>::new().build(
            raw_window,
            std::num::NonZeroU32::new(1).unwrap(),
            std::num::NonZeroU32::new(1).unwrap(),
        );
        let gl_context = unsafe { display.create_context(&config, &attributes)? };
        let surface = unsafe { display.create_window_surface(&config, &surface_attributes)? };
        let gl_context = gl_context.make_current(&surface)?;
        let context = Context::from_gl_context(Arc::new(unsafe {
            crate::context::Context::from_loader_function(|symbol| {
                let symbol = std::ffi::CString::new(symbol).unwrap();
                display.get_proc_address(&symbol)
            })
        }))?;

        Ok(Self {
            context,
            _native: Rc::new(NativeContext::HiddenWindow(
                gl_context, surface, window, event_loop,
            )),
        })
    }
}

impl Deref for HeadlessContext {
    type Target = Context;

    fn deref(&self) -> &Self::Target {
        &self.context
    }
}

#[cfg(target_os = "windows")]
fn new_event_loop() -> EventLoop<()> {
    use winit::platform::windows::EventLoopBuilderExtWindows;
    winit::event_loop::EventLoopBuilder::new()
        .with_any_thread(true)
        .build()
}

#[cfg(not(target_os = "windows"))]
fn new_event_loop() -> EventLoop<()> {
    EventLoop::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::HasContext;
    use crate::context::{COLOR_ATTACHMENT0, FRAMEBUFFER, RENDERBUFFER, RGBA8};
    use crate::core::{ClearState, RenderTarget, ScissorBox};

    #[test]
    fn creates_offscreen_context_and_reads_framebuffer() {
        let context = HeadlessContext::new().expect("offscreen GL context");
        assert!(!context.version().is_embedded);
        let (framebuffer, renderbuffer) = unsafe {
            let framebuffer = context.create_framebuffer().unwrap();
            let renderbuffer = context.create_renderbuffer().unwrap();
            context.bind_renderbuffer(RENDERBUFFER, Some(renderbuffer));
            context.renderbuffer_storage(RENDERBUFFER, RGBA8, 1, 2);
            context.bind_framebuffer(FRAMEBUFFER, Some(framebuffer));
            context.framebuffer_renderbuffer(
                FRAMEBUFFER,
                COLOR_ATTACHMENT0,
                RENDERBUFFER,
                Some(renderbuffer),
            );
            (framebuffer, renderbuffer)
        };
        let target = RenderTarget::from_framebuffer(&context, 1, 2, framebuffer);
        target.clear_partially(
            ScissorBox::new_at_origo(1, 1),
            ClearState::color(1.0, 0.0, 0.0, 1.0),
        );
        let mut upper = ScissorBox::new_at_origo(1, 1);
        upper.y = 1;
        target.clear_partially(upper, ClearState::color(0.0, 1.0, 0.0, 1.0));

        let bottom_first: Vec<[u8; 4]> = target.read_framebuffer(false);
        assert_eq!(bottom_first, [[255, 0, 0, 255], [0, 255, 0, 255]]);
        let top_first: Vec<[u8; 4]> = target.read_framebuffer(true);
        assert_eq!(top_first, [[0, 255, 0, 255], [255, 0, 0, 255]]);

        drop(target);
        unsafe { context.delete_renderbuffer(renderbuffer) };
    }
}

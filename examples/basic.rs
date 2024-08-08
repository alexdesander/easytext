use easytext::area::TextArea;
use easytext::{EasyText, TextAreaHandle};
use pollster::FutureExt;
use wgpu::{
    Adapter, Device, Instance, MemoryHints, PresentMode, Queue, Surface, SurfaceConfiguration,
    SurfaceTargetUnsafe,
};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::Key;
use winit::window::{Window, WindowId};

#[derive(Default)]
struct AppWrapper {
    app: Option<App>,
    window: Option<Window>,
}

impl ApplicationHandler for AppWrapper {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            self.window = Some(
                event_loop
                    .create_window(
                        Window::default_attributes().with_inner_size(PhysicalSize::new(720, 720)),
                    )
                    .unwrap(),
            );
            self.app = Some(App::new(self.window.as_ref().unwrap()));
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
                let app = self.app.as_mut().unwrap();
                match app.render() {
                    Ok(_) => {}
                    Err(wgpu::SurfaceError::Lost) => {
                        app.resize(self.window.as_ref().unwrap().inner_size())
                    }
                    Err(wgpu::SurfaceError::OutOfMemory) => event_loop.exit(),
                    Err(e) => eprintln!("{:?}", e),
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            WindowEvent::Resized(new_size) => {
                self.app.as_mut().unwrap().resize(new_size);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                self.app.as_mut().unwrap().handle_key_event(event);
            }
            _ => (),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum FontID {
    Default,
}

struct App {
    _instance: Instance,
    surface: Surface<'static>,
    _adapter: Adapter,
    device: Device,
    queue: Queue,
    surface_config: SurfaceConfiguration,
    easy_text: EasyText<FontID>,
    text_area_handle: TextAreaHandle,
}

impl App {
    pub fn new(window: &Window) -> Self {
        let size = window.inner_size();
        let instance = Instance::default();
        let surface = unsafe {
            instance
                .create_surface_unsafe(SurfaceTargetUnsafe::from_window(&window).unwrap())
                .unwrap()
        };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .block_on()
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    label: None,
                    memory_hints: MemoryHints::Performance,
                },
                None,
            )
            .block_on()
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);
        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: PresentMode::AutoVsync,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 1,
        };

        let mut easy_text = EasyText::new(size.width, size.height, &device, surface_format);
        easy_text.add_font(FontID::Default, include_bytes!("../m5x7.ttf").to_vec());

        let text_area_handle = easy_text.add_text_area(TextArea {
            x: 100.0,
            y: 100.0,
            width: 500.0,
            height: 500.0,
            text: "Press a to debug-show the glyph texture atlas, press b to debug-show text area borders. Press d to add a char.".to_string(),
            font: FontID::Default,
            size: 64.0,
            line_height_factor: 0.8,
            top_offset: 0.0,
            left_offset: 0.0,
        });

        Self {
            easy_text,
            _instance: instance,
            surface,
            _adapter: adapter,
            device,
            queue,
            surface_config,
            text_area_handle,
        }
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.surface_config.width = new_size.width;
            self.surface_config.height = new_size.height;
            self.surface.configure(&self.device, &self.surface_config);
            self.easy_text
                .resize(&self.queue, new_size.width, new_size.height);
        }
    }

    pub fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let output = self.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });
            self.easy_text
                .render(&self.device, &self.queue, &mut render_pass);
        }
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }

    fn handle_key_event(&mut self, event: KeyEvent) {
        if event.state.is_pressed() {
            match event.logical_key.as_ref() {
                Key::Character("a") => self.easy_text.toggle_debug_show_atlas(),
                Key::Character("b") => self.easy_text.toggle_debug_show_area_borders(),
                Key::Character("d") => {
                    let area = self.easy_text.text_area_mut(self.text_area_handle).unwrap();
                    area.text.push('d');
                }
                _ => {}
            }
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = AppWrapper::default();
    event_loop.run_app(&mut app).unwrap();
}

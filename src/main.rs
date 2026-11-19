mod editor;
mod plan;

use std::{sync::Arc, time::Instant};

use editor::Editor;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};
use wolfcraft_assets::AtlasManager;
use wolfcraft_common::gpu::GpuContext;
use wolfcraft_engine::renderer::WorldRenderer;
use wolfcraft_ui::GuiRenderer;

struct Graphics {
    gpu: GpuContext,
    gui: GuiRenderer,
    world: WorldRenderer,
}

impl Graphics {
    fn new(window: Arc<Window>) -> Result<Self, String> {
        let gpu = GpuContext::new(window.clone(), true, false)?;
        let atlases = AtlasManager::new(None)?;
        let gui = GuiRenderer::new(&gpu.device, gpu.config.format, &window, &atlases);
        let (width, height) = gpu.size();
        let world = WorldRenderer::new(
            &gpu.device,
            &gpu.queue,
            gpu.config.format,
            &atlases,
            width,
            height,
        );
        Ok(Self { gpu, gui, world })
    }

    fn frame(&mut self, editor: &mut Editor, elapsed: f32) {
        let Some(frame) = self.gpu.acquire() else {
            return;
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.gpu.device.create_command_encoder(&Default::default());
        let gui = self.gui.run(&self.gpu.window, |ctx| editor.ui(ctx));
        let (width, height) = self.gpu.size();
        if editor.preview {
            let params = editor.camera(width as f32 / height as f32, elapsed);
            let model = plan::model(&editor.plan, params.eye.as_vec3(), editor.cutaway);
            self.world
                .set_model(&self.gpu.device, &self.gpu.queue, &model);
            self.world.render(
                &self.gpu.device,
                &self.gpu.queue,
                &mut encoder,
                &view,
                &params,
                [0.075, 0.11, 0.14],
                [0.075, 0.11, 0.14],
            );
        } else {
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
        }
        let extra = self.gui.render(
            &self.gpu.device,
            &self.gpu.queue,
            &mut encoder,
            &view,
            [width, height],
            gui,
        );
        self.gpu
            .queue
            .submit(extra.into_iter().chain(std::iter::once(encoder.finish())));
        frame.present();
    }
}

struct App {
    editor: Editor,
    graphics: Option<Graphics>,
    started: Instant,
    error: Option<String>,
    smoke_frames: Option<usize>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.graphics.is_some() {
            return;
        }
        let result = event_loop
            .create_window(
                Window::default_attributes()
                    .with_title("WolfCraft Home Planner")
                    .with_inner_size(LogicalSize::new(1150.0, 800.0))
                    .with_min_inner_size(LogicalSize::new(760.0, 540.0)),
            )
            .map_err(|error| error.to_string())
            .and_then(|window| Graphics::new(Arc::new(window)));
        match result {
            Ok(graphics) => {
                graphics.gpu.window.request_redraw();
                self.graphics = Some(graphics);
            }
            Err(error) => {
                self.error = Some(error);
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(graphics) = self.graphics.as_mut() else {
            return;
        };
        if id != graphics.gpu.window.id() {
            return;
        }
        let response = graphics.gui.on_event(&graphics.gpu.window, &event);
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) if size.width != 0 && size.height != 0 => {
                graphics.gpu.resize(size.width, size.height);
                graphics
                    .world
                    .resize(&graphics.gpu.device, size.width, size.height);
                graphics.gpu.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                graphics.frame(&mut self.editor, self.started.elapsed().as_secs_f32());
                if let Some(frames) = &mut self.smoke_frames {
                    *frames += 1;
                    self.editor.preview = true;
                    if *frames >= 3 {
                        graphics.gpu.device.poll(wgpu::Maintain::Wait);
                        event_loop.exit();
                    } else {
                        graphics.gpu.window.request_redraw();
                    }
                } else if graphics.gui.ctx.has_requested_repaint() {
                    graphics.gpu.window.request_redraw();
                }
            }
            _ if response.repaint => graphics.gpu.window.request_redraw(),
            _ => {}
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    let mut editor = Editor::default();
    if smoke {
        editor.load_example();
    }
    let mut app = App {
        editor,
        graphics: None,
        started: Instant::now(),
        error: None,
        smoke_frames: smoke.then_some(0),
    };
    EventLoop::new()?.run_app(&mut app)?;
    if let Some(error) = app.error {
        return Err(error.into());
    }
    Ok(())
}

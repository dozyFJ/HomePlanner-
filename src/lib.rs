mod editor;
mod furnishings;
mod modeling;
mod plan;
mod scan;

use std::{sync::Arc, time::Instant};

use editor::Editor;
use furnishings::FurnishingAtlas;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};
use wolfcraft_assets::{AtlasKind, AtlasManager};
use wolfcraft_common::gpu::GpuContext;
use wolfcraft_engine::renderer::WorldRenderer;
use wolfcraft_ui::GuiRenderer;

struct Graphics {
    gpu: GpuContext,
    gui: GuiRenderer,
    world: WorldRenderer,
    furnishings: FurnishingAtlas,
}

impl Graphics {
    fn new(window: Arc<Window>) -> Result<Self, String> {
        let gpu = GpuContext::new(window.clone(), true, false)?;
        let mut atlases = AtlasManager::new(None)?;
        *atlases.get_mut(AtlasKind::Entities) = FurnishingAtlas::build()?;
        let gui = GuiRenderer::new(&gpu.device, gpu.config.format, &window, &atlases);
        let furnishings = FurnishingAtlas::upload(&gui.ctx, atlases.get(AtlasKind::Entities));
        let (width, height) = gpu.size();
        let world = WorldRenderer::new(
            &gpu.device,
            &gpu.queue,
            gpu.config.format,
            &atlases,
            width,
            height,
        );
        Ok(Self {
            gpu,
            gui,
            world,
            furnishings,
        })
    }

    fn frame(&mut self, editor: &mut Editor, elapsed: f32) {
        let Some(frame) = self.gpu.acquire() else {
            return;
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.gpu.device.create_command_encoder(&Default::default());
        let gui = self
            .gui
            .run(&self.gpu.window, |ctx| editor.ui(ctx, &self.furnishings));
        let (width, height) = self.gpu.size();
        if editor.preview || editor.modeling_open {
            let aspect = width as f32 / height as f32;
            let (params, model) = if editor.modeling_open {
                let params = editor.modeling.frame_params(aspect, elapsed);
                let model = editor.modeling.vertices(params.eye.as_vec3());
                (params, model)
            } else {
                let params = editor.camera(aspect, elapsed);
                let model = plan::model(
                    &editor.plan,
                    params.eye.as_vec3(),
                    editor.cutaway,
                    Some(&self.furnishings),
                );
                (params, model)
            };
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
    window: Option<Arc<Window>>,
    graphics: Option<Graphics>,
    started: Instant,
    error: Option<String>,
    smoke_frames: Option<usize>,
}

impl ApplicationHandler for App {
    fn new_events(&mut self, _event_loop: &ActiveEventLoop, cause: winit::event::StartCause) {
        if matches!(cause, winit::event::StartCause::ResumeTimeReached { .. })
            && self.editor.photo_wait.is_some()
            && let Some(graphics) = &self.graphics
        {
            graphics.gpu.window.request_redraw();
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        if self
            .editor
            .photo_wait
            .is_some_and(|deadline| deadline <= now)
        {
            self.editor.photo_wait = None;
        }
        let control = if self.graphics.is_some() && self.editor.photo_wait.is_some() {
            winit::event_loop::ControlFlow::WaitUntil(now + std::time::Duration::from_millis(300))
        } else {
            winit::event_loop::ControlFlow::Wait
        };
        event_loop.set_control_flow(control);
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.graphics.is_some() {
            return;
        }
        let result = (|| {
            let window = match &self.window {
                Some(window) => window.clone(),
                None => {
                    let window = Arc::new(
                        event_loop
                            .create_window(
                                Window::default_attributes()
                                    .with_title("WolfCraft Home Planner")
                                    .with_inner_size(LogicalSize::new(1150.0, 800.0))
                                    .with_min_inner_size(LogicalSize::new(760.0, 540.0)),
                            )
                            .map_err(|error| error.to_string())?,
                    );
                    self.window = Some(window.clone());
                    window
                }
            };
            Graphics::new(window)
        })();
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

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        if cfg!(target_os = "android") {
            self.graphics = None;
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
                    self.editor.preview = *frames < 3;
                    self.editor.modeling_open = *frames >= 3;
                    if *frames >= 5 {
                        graphics.gpu.device.poll(wgpu::Maintain::Wait);
                        event_loop.exit();
                    } else {
                        graphics.gpu.window.request_redraw();
                    }
                } else if graphics.gui.ctx.has_requested_repaint() {
                    graphics.gpu.window.request_redraw();
                }
            }
            WindowEvent::Touch(_) => graphics.gpu.window.request_redraw(),
            _ if response.repaint => graphics.gpu.window.request_redraw(),
            _ => {}
        }
    }
}

pub fn run_desktop() -> Result<(), Box<dyn std::error::Error>> {
    let smoke = std::env::args().any(|arg| arg == "--smoke-test");
    run(EventLoop::new()?, Editor::default(), smoke)
}

fn run(
    event_loop: EventLoop<()>,
    mut editor: Editor,
    smoke: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if smoke {
        editor.load_example();
    }
    let mut app = App {
        editor,
        window: None,
        graphics: None,
        started: Instant::now(),
        error: None,
        smoke_frames: smoke.then_some(0),
    };
    event_loop.run_app(&mut app)?;
    if let Some(error) = app.error {
        return Err(error.into());
    }
    Ok(())
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    use winit::platform::android::EventLoopBuilderExtAndroid;

    let Some(data) = app.internal_data_path() else {
        return;
    };
    let log = data.join("planner.log");
    let activity = app.clone();
    let mut editor = Editor::for_android(data.join("house.json"));
    editor.open_updates = Some(Box::new(move || {
        call_android_activity(&activity, jni::jni_str!("showUpdates"))
    }));
    let activity = app.clone();
    editor.open_photo = Some(Box::new(move || {
        call_android_activity(&activity, jni::jni_str!("pickPlanPhoto"))
    }));
    let result = EventLoop::builder()
        .with_android_app(app)
        .build()
        .map_err(|error| error.to_string())
        .and_then(|event_loop| run(event_loop, editor, false).map_err(|error| error.to_string()));
    if let Err(error) = result {
        let _ = std::fs::write(log, error);
    }
}

#[cfg(target_os = "android")]
fn call_android_activity(
    activity: &winit::platform::android::activity::AndroidApp,
    method: &jni::strings::JNIStr,
) -> Result<(), String> {
    use jni::{JavaVM, jni_sig, objects::JObject, refs::Global};
    // AndroidApp keeps the Activity global reference alive during this call.
    let vm = unsafe { JavaVM::from_raw(activity.vm_as_ptr().cast()) };
    let raw = activity.activity_as_ptr() as jni::sys::jobject;
    vm.attach_current_thread(|env| -> jni::errors::Result<()> {
        let java_activity = unsafe { env.as_cast_raw::<Global<JObject>>(&raw)? };
        env.call_method(java_activity, method, jni_sig!("()V"), &[])?;
        Ok(())
    })
    .map_err(|error| error.to_string())
}

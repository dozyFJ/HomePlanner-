mod editor;
mod furnishings;
mod modeling;
mod navigation;
mod plan;
mod scan;
mod sheet_edit;
mod theme;

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
    fn new(window: Arc<Window>, dark: bool) -> Result<Self, String> {
        let gpu = GpuContext::new(window.clone(), true, false)?;
        let mut atlases = AtlasManager::new(None)?;
        *atlases.get_mut(AtlasKind::Entities) = FurnishingAtlas::build()?;
        let gui = GuiRenderer::new(&gpu.device, gpu.config.format, &window, &atlases);
        theme::apply(&gui.ctx, dark);
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
                let model = editor.modeling.scene_vertices(
                    &editor.plan,
                    params.eye.as_vec3(),
                    editor.cutaway,
                    &self.furnishings,
                );
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
                theme::VIEWPORT,
                theme::VIEWPORT,
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
    dark: bool,
    #[cfg(target_os = "android")]
    android: Option<AndroidSession>,
}

impl App {
    fn new(mut editor: Editor, smoke: bool) -> Self {
        if smoke {
            editor.load_example();
        }
        Self {
            editor,
            window: None,
            graphics: None,
            started: Instant::now(),
            error: None,
            smoke_frames: smoke.then_some(0),
            dark: false,
            #[cfg(target_os = "android")]
            android: None,
        }
    }

    #[cfg(target_os = "android")]
    fn sync_android(&mut self) {
        let Some(session) = &mut self.android else {
            return;
        };
        let dark = session.data.join("dark-mode").is_file();
        if dark != self.dark {
            self.dark = dark;
            if let Some(graphics) = &self.graphics {
                theme::apply(&graphics.gui.ctx, dark);
            }
        }
        let Ok(key) = android_string(&session.app, jni::jni_str!("projectKey")) else {
            return;
        };
        if session.current.as_deref() == Some(&key) {
            return;
        }
        let Some((modeling, id)) = key.split_once(':') else {
            return;
        };
        let directory = if id == "legacy" {
            session.data.clone()
        } else if valid_project_id(id) {
            session.data.join("projects").join(id)
        } else {
            return;
        };
        if !directory.is_dir() {
            return;
        }
        if session.current.is_some()
            && let Err(error) = self.editor.persist_project()
        {
            let _ = std::fs::write(session.data.join("planner.log"), error.to_string());
            return;
        }

        let mut editor = Editor::for_android(directory.join("house.json"));
        editor.modeling_open = modeling == "m";
        if let Ok(title) = android_string(&session.app, jni::jni_str!("projectTitle"))
            && !title.is_empty()
        {
            editor.project_title = title;
        }
        editor.open_updates = Some(android_action(&session.app, jni::jni_str!("showUpdates")));
        editor.open_projects = Some(android_action(&session.app, jni::jni_str!("showProjects")));
        editor.close_project = Some(android_action(&session.app, jni::jni_str!("closeProject")));
        editor.open_new_project = Some(android_action(
            &session.app,
            jni::jni_str!("showNewProject"),
        ));
        editor.open_photo = Some(android_action(&session.app, jni::jni_str!("pickPlanPhoto")));
        self.editor = editor;
        session.current = Some(key);
    }
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
        #[cfg(target_os = "android")]
        self.sync_android();
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
            Graphics::new(window, self.dark)
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
        #[cfg(target_os = "android")]
        if let Some(session) = &self.android {
            if session.current.is_some()
                && let Err(error) = self.editor.persist_project()
            {
                let _ = std::fs::write(session.data.join("planner.log"), error.to_string());
            }
            self.graphics = None;
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        #[cfg(target_os = "android")]
        if matches!(event, WindowEvent::Focused(true)) {
            self.sync_android();
        }
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
    run(EventLoop::new()?, App::new(Editor::default(), smoke))
}

fn run(event_loop: EventLoop<()>, mut app: App) -> Result<(), Box<dyn std::error::Error>> {
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
    let mut planner = App::new(Editor::default(), false);
    planner.android = Some(AndroidSession {
        app: app.clone(),
        data,
        current: None,
    });
    let result = EventLoop::builder()
        .with_android_app(app)
        .build()
        .map_err(|error| error.to_string())
        .and_then(|event_loop| run(event_loop, planner).map_err(|error| error.to_string()));
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

#[cfg(target_os = "android")]
struct AndroidSession {
    app: winit::platform::android::activity::AndroidApp,
    data: std::path::PathBuf,
    current: Option<String>,
}

#[cfg(target_os = "android")]
fn android_action(
    app: &winit::platform::android::activity::AndroidApp,
    method: &'static jni::strings::JNIStr,
) -> Box<dyn Fn() -> Result<(), String>> {
    let app = app.clone();
    Box::new(move || call_android_activity(&app, method))
}

#[cfg(target_os = "android")]
fn android_string(
    activity: &winit::platform::android::activity::AndroidApp,
    method: &jni::strings::JNIStr,
) -> Result<String, String> {
    use jni::{
        JavaVM, jni_sig,
        objects::{JObject, JString},
        refs::Global,
    };
    // AndroidApp keeps the Activity global reference alive during this call.
    let vm = unsafe { JavaVM::from_raw(activity.vm_as_ptr().cast()) };
    let raw = activity.activity_as_ptr() as jni::sys::jobject;
    vm.attach_current_thread(|env| -> jni::errors::Result<String> {
        let activity = unsafe { env.as_cast_raw::<Global<JObject>>(&raw)? };
        let object = env
            .call_method(activity, method, jni_sig!("()Ljava/lang/String;"), &[])?
            .l()?;
        let string = env.cast_local::<JString>(object)?;
        string.try_to_string(env)
    })
    .map_err(|error| error.to_string())
}

#[cfg(target_os = "android")]
fn valid_project_id(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, value)| {
            if [8, 13, 18, 23].contains(&index) {
                value == b'-'
            } else {
                value.is_ascii_hexdigit()
            }
        })
}

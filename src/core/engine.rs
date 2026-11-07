use crate::core::scene::Scene;
use crate::render::Renderer;
use crate::platform::CustomWindow;
use crate::scene_parser::{self, SceneSettings, SceneObject, RenderMode};
use crate::animation::Animator;
use crate::math::Vec3;
use crate::physics::PhysicsWorld;
use crate::physics::PhysicsProps;
use rapier3d::prelude::RigidBodyHandle;
use winit::keyboard::KeyCode;
use winit::application::ApplicationHandler;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::event::{WindowEvent, ElementState, MouseButton};
use rapier3d::na::vector;
use std::time::{SystemTime, Duration};

pub struct Engine {
    settings: SceneSettings,
    scene: Scene,
    window: Option<CustomWindow>,
    renderer: Option<Renderer>,
    animator: Animator,
    physics: Option<PhysicsWorld>,
    scene_path: String,
    last_mtime: Option<SystemTime>,
    reload_check: Duration,
    camera_pos: Vec3,
    camera_target: Vec3,
    yaw: f32,
    pitch: f32,
    camera_active: bool,
    #[allow(dead_code)]
    dragging: bool,
    #[allow(dead_code)]
    drag_ox: f32,
    #[allow(dead_code)]
    drag_oy: f32,
    #[allow(dead_code)]
    drag_wx: i32,
    #[allow(dead_code)]
    drag_wy: i32,
    #[allow(dead_code)]
    prev_mouse_down: bool,
}

impl Engine {
    pub fn run_from(path: String) {
        let (settings, objects) = scene_parser::load_scene(&path);
        let mut engine = Engine::new_with_path(settings, path);
        engine.add_objects(objects);
        engine.run();
    }

    pub fn new(settings: SceneSettings) -> Self {
        Self::new_with_path(settings, String::new())
    }

    pub fn new_with_path(settings: SceneSettings, scene_path: String) -> Self {
        let camera_active = settings.camera_enabled;
        let mut engine = Self {
            settings,
            scene: Scene::new(),
            window: None,
            renderer: None,
            animator: Animator::new(),
            physics: Some(PhysicsWorld::new()),
            scene_path,
            last_mtime: None,
            reload_check: Duration::from_secs(0),
            camera_pos: Vec3::new(0.0, 0.0, 5.0),
            camera_target: Vec3::new(0.0, 0.0, 0.0),
            yaw: -90.0_f32.to_radians(),
            pitch: 0.0,
            camera_active,
            dragging: false,
            drag_ox: 0.0,
            drag_oy: 0.0,
            drag_wx: 0,
            drag_wy: 0,
            prev_mouse_down: false,
        };
        // Сцену загружает вызывающий (run_from), здесь только запоминаем mtime,
        // чтобы hot-reload не сработал на первом же кадре.
        if !engine.scene_path.is_empty() {
            engine.last_mtime = std::fs::metadata(&engine.scene_path)
                .and_then(|m| m.modified()).ok();
        }
        // Стартовая камера из настроек сцены: строим yaw/pitch так, чтобы камера
        // смотрела из camera_pos на camera_target (по умолчанию совпадает с
        // прежним поведением — спереди на центр).
        {
            let (px, py, pz) = engine.settings.camera_pos;
            let (tx, ty, tz) = engine.settings.camera_target;
            engine.camera_pos = Vec3::new(px, py, pz);
            engine.camera_target = Vec3::new(tx, ty, tz);
            let (dx, dy, dz) = (tx - px, ty - py, tz - pz);
            let horiz = (dx * dx + dz * dz).sqrt();
            engine.pitch = if horiz > 1e-6 {
                dy.atan2(horiz)
            } else if dy >= 0.0 {
                std::f32::consts::FRAC_PI_2
            } else {
                -std::f32::consts::FRAC_PI_2
            };
            engine.yaw = dz.atan2(dx);
        }
        engine
    }

    // Перезагружает сцену из .mff: объекты, анимации, физика, настройки окна.
    // Камера и её позиция сохраняются.
    fn reload_scene(&mut self) {
        if self.scene_path.is_empty() { return; }
        println!("[Engine] Reloading scene: {}", self.scene_path);
        match scene_parser::load_scene(&self.scene_path) {
            (settings, objects) => {
                self.settings = settings;
                self.camera_active = self.settings.camera_enabled;
                self.scene = Scene::new();
                self.animator = Animator::new();
                self.physics = Some(PhysicsWorld::new());
                self.add_objects(objects);
                // Применяем новые настройки к уже созданному окну/рендереру
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.set_background(self.settings.background);
                    renderer.set_wireframe(self.settings.wireframe);
                    renderer.set_thermal(self.settings.thermal);
                }
                println!("[Engine] Scene reloaded: {} objects", self.scene.len());
            }
        }
    }

    // Проверяет изменился ли файл сцены; если да — перезагружает.
    // Проверка не чаще раза в ~0.25 сек.
    fn check_reload(&mut self, dt: Duration) {
        if self.scene_path.is_empty() { return; }
        self.reload_check += dt;
        if self.reload_check < Duration::from_millis(250) { return; }
        self.reload_check = Duration::from_secs(0);

        let mtime = std::fs::metadata(&self.scene_path)
            .and_then(|m| m.modified()).ok();
        if let Some(mt) = mtime {
            if self.last_mtime != Some(mt) {
                // Файл мог быть ещё не дописан — ждём 150мс и пробуем снова
                std::thread::sleep(Duration::from_millis(150));
                self.last_mtime = std::fs::metadata(&self.scene_path)
                    .and_then(|m| m.modified()).ok();
                self.reload_scene();
            }
        } else if self.last_mtime.is_none() {
            // файл ещё не существует — первый раз запомним
            self.last_mtime = SystemTime::now().into();
        }
    }

    pub fn add_objects(&mut self, objects: Vec<SceneObject>) {
        for obj in objects.into_iter() {
            if let SceneObject::Animated(anim, _) = &obj {
                self.animator.add_animation(&anim.name, anim.keyframes.clone());
            }
            self.scene.add_object(obj);
        }

        if let Some(physics) = self.physics.as_mut() {
            for obj in self.scene.get_objects_mut() {
                if let Some(props) = obj.physics.as_ref() {
                    let (px, py, pz) = obj.transform.position;
                    let (sx, sy, sz) = obj.transform.scale;
                    // Полуразмеры берём из локального AABB меша и умножаем на масштаб.
                    let (aabb_min, aabb_max) = (obj.aabb_min, obj.aabb_max);
                    let hx = ((aabb_max.x - aabb_min.x) * 0.5 * sx).max(0.01);
                    let hy = ((aabb_max.y - aabb_min.y) * 0.5 * sy).max(0.01);
                    let hz = ((aabb_max.z - aabb_min.z) * 0.5 * sz).max(0.01);
                    let h: RigidBodyHandle = physics.add_body(
                        vector![px, py, pz],
                        vector![hx, hy, hz],
                        props.mass,
                        props.restitution,
                    );
                    obj.physics_handle = Some(h);
                }
            }
        }
    }

    pub fn run(&mut self) {
        println!("[Engine] Starting event loop");
        let event_loop = EventLoop::new().unwrap();
        let _ = event_loop.run_app(self);
    }

    fn render_frame(&mut self) {
        let Some(mut window) = self.window.take() else { return };
        let Some(mut renderer) = self.renderer.take() else { self.window = Some(window); return };

        window.time.update();
        let speed = 5.0 * window.time.delta();

        if self.camera_active {
            if window.input.mouse_buttons.contains(&winit::event::MouseButton::Right) {
                let dx = window.input.mouse_delta_x;
                let dy = window.input.mouse_delta_y;
                self.yaw += dx * 0.002;
                self.pitch -= dy * 0.002;
                self.pitch = self.pitch.clamp(-1.5, 1.5);
            }

            let front = Vec3::new(
                self.yaw.cos() * self.pitch.cos(),
                self.pitch.sin(),
                self.yaw.sin() * self.pitch.cos(),
            ).normalize();
            self.camera_target = self.camera_pos + front;

            let forward = front;
            let right = forward.cross(&Vec3::new(0.0, 1.0, 0.0)).normalize();

            if window.input.is_key_pressed(KeyCode::KeyW) {
                self.camera_pos = self.camera_pos + forward * speed;
            }
            if window.input.is_key_pressed(KeyCode::KeyS) {
                self.camera_pos = self.camera_pos - forward * speed;
            }
            if window.input.is_key_pressed(KeyCode::KeyA) {
                self.camera_pos = self.camera_pos - right * speed;
            }
            if window.input.is_key_pressed(KeyCode::KeyD) {
                self.camera_pos = self.camera_pos + right * speed;
            }
            if window.input.is_key_pressed(KeyCode::KeyQ) {
                self.camera_pos.y += speed;
            }
            if window.input.is_key_pressed(KeyCode::KeyE) {
                self.camera_pos.y -= speed;
            }
            self.camera_target = self.camera_pos + front;
            renderer.set_camera(self.camera_pos, self.camera_target);
        }

        let dt = window.time.delta();
        self.check_reload(Duration::from_secs_f32(dt));

        for object in self.scene.get_objects_mut() {
            if object.is_animated {
                let an = object.anim_name.clone();
                self.animator.apply(&mut object.transform, &an, dt);
                if let Some(c) = self.animator.get_current_color(&an) {
                    object.color = c;
                }
            }
        }

        if let Some(physics) = self.physics.as_mut() {
            physics.step();
            for object in self.scene.get_objects_mut() {
                if let Some(handle) = object.physics_handle {
                    let pos = physics.get_body_pos(handle);
                    object.transform.set_position(pos.x, pos.y, pos.z);
                }
            }
        }

        let ui_layout = crate::render::ui::layout(window.width as f32, window.height as f32);
        let (nx, ny) = crate::render::ui::mouse_to_ndc(
            window.width as f32,
            window.height as f32,
            window.input.mouse_x,
            window.input.mouse_y,
        );
        let hover = match crate::render::ui::hit_test(&ui_layout, nx, ny) {
            crate::render::ui::UiHit::Close => 2,
            crate::render::ui::UiHit::Minimize => 1,
            _ => 0,
        };
        renderer.render(&self.scene, window.width, window.height, hover, dt);
        window.input.reset_delta();

        self.renderer = Some(renderer);
        self.window = Some(window);
    }
}

impl ApplicationHandler for Engine {
    fn resumed(&mut self, elwt: &ActiveEventLoop) {
        if self.window.is_some() { return; }

        let mut window = CustomWindow::new(&self.settings);
        window.init_window(elwt);
        let renderer = Renderer::new(
            &window,
            self.settings.render_mode == RenderMode::Heavy,
            self.settings.aa_type,
            self.settings.thermal,
        );

        println!("[Engine] Window + Vulkan ready");
        self.window = Some(window);
        self.renderer = Some(renderer);
    }

    fn window_event(&mut self, elwt: &ActiveEventLoop, _: winit::window::WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => elwt.exit(),
            WindowEvent::RedrawRequested => self.render_frame(),
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(window) = self.window.as_mut() {
                    window.handle_event(&event);
                }
                // Собственная панель окна: кнопки закрытия/сворачивания и перетаскивание.
                if state == ElementState::Pressed && button == MouseButton::Left {
                    if let Some(window) = self.window.as_ref() {
                        let l = crate::render::ui::layout(window.width as f32, window.height as f32);
                        let (nx, ny) = crate::render::ui::mouse_to_ndc(
                            window.width as f32,
                            window.height as f32,
                            window.input.mouse_x,
                            window.input.mouse_y,
                        );
                        match crate::render::ui::hit_test(&l, nx, ny) {
                            crate::render::ui::UiHit::Close => elwt.exit(),
                            crate::render::ui::UiHit::Minimize => {
                                if let Some(raw) = window.raw.as_ref() {
                                    raw.set_minimized(true);
                                }
                            }
                            crate::render::ui::UiHit::Panel => {
                                if let Some(raw) = window.raw.as_ref() {
                                    let _ = raw.drag_window();
                                }
                            }
                            crate::render::ui::UiHit::None => {}
                        }
                    }
                }
            }
            _ => {
                if let Some(window) = self.window.as_mut() {
                    window.handle_event(&event);
                }
            }
        }
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        // Непрерывный рендер: просим следующий кадр у окна. Сама отрисовка
        // происходит в WindowEvent::RedrawRequested, темп задаёт композитор
        // (present-mode FIFO синхронизирован с вертикальной развёрткой).
        if let Some(window) = self.window.as_ref() {
            if let Some(raw) = window.raw.as_ref() {
                raw.request_redraw();
            }
        }
    }
}
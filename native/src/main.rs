//! Minimal transparent orb — Rust + egui, no WebView.
//! Scenes match Vue WeatherBall; live data from Open-Meteo.

// Release: GUI-only process — no extra console window beside the orb.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

mod display;
mod settings;
mod weather;

use eframe::egui::{
    self, Color32, ColorImage, Pos2, Rect, Sense, Shape, Stroke, TextureHandle, TextureOptions,
    Vec2,
};
use std::f32::consts::{PI, TAU};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};
use weather::{refresh_blocking, HourlyPoint, LiveWeather, WeatherState};

const W: f32 = 160.0;
const H_DETAIL: f32 = 520.0;
const BALL_R: f32 = 50.0;
/// Ball center from window top — same in compact & detail (Vue: stays put while height grows down).
const BALL_CENTER_Y: f32 = 140.0;
const REFRESH_SECS: u64 = 20 * 60;
const DRAG_THRESHOLD: f32 = 5.0;
/// Match Vue panel: height ~280ms, opacity a bit quicker.
const DETAIL_ANIM_SECS: f32 = 0.28;

/// Matches Vue `PrecipIntensity`
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Intensity {
    Light,
    Moderate,
    Heavy,
}

impl Intensity {
    fn label(self) -> &'static str {
        match self {
            Intensity::Light => "小",
            Intensity::Moderate => "中",
            Intensity::Heavy => "大",
        }
    }
}

/// Matches Vue weather visuals (clear→sunny; drizzle as its own entry).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scene {
    Sunny,
    Cloudy,
    Overcast,
    Drizzle,
    Rain(Intensity),
    Snow(Intensity),
    Storm(Intensity),
}

impl Scene {
    fn label(self) -> String {
        match self {
            Scene::Sunny => "晴天".into(),
            Scene::Cloudy => "多云".into(),
            Scene::Overcast => "阴天".into(),
            Scene::Drizzle => "毛毛雨".into(),
            Scene::Rain(i) => format!("{}雨", i.label()),
            Scene::Snow(i) => format!("{}雪", i.label()),
            Scene::Storm(i) => format!("{}雷暴", i.label()),
        }
    }

    fn orb_kind(self) -> OrbKind {
        match self {
            Scene::Sunny => OrbKind::Sunny,
            Scene::Cloudy => OrbKind::Cloudy,
            Scene::Overcast => OrbKind::Overcast,
            Scene::Drizzle | Scene::Rain(_) => OrbKind::Rain,
            Scene::Snow(_) => OrbKind::Snow,
            Scene::Storm(_) => OrbKind::Storm,
        }
    }

    fn intensity(self) -> Option<Intensity> {
        match self {
            Scene::Drizzle => Some(Intensity::Light),
            Scene::Rain(i) | Scene::Snow(i) | Scene::Storm(i) => Some(i),
            _ => None,
        }
    }

    fn glow(self) -> Color32 {
        match self.orb_kind() {
            OrbKind::Sunny => Color32::from_rgba_unmultiplied(255, 190, 90, 115),
            OrbKind::Cloudy => Color32::from_rgba_unmultiplied(160, 195, 240, 89),
            OrbKind::Overcast => Color32::from_rgba_unmultiplied(190, 200, 215, 71),
            OrbKind::Rain => match self {
                Scene::Drizzle => Color32::from_rgba_unmultiplied(90, 150, 230, 82),
                _ => Color32::from_rgba_unmultiplied(90, 150, 230, 102),
            },
            OrbKind::Snow => Color32::from_rgba_unmultiplied(200, 230, 255, 107),
            OrbKind::Storm => Color32::from_rgba_unmultiplied(150, 120, 240, 107),
        }
    }

    fn water_a(self) -> Color32 {
        match self.orb_kind() {
            OrbKind::Sunny => Color32::from_rgb(0x2a, 0xa8, 0xa2),
            OrbKind::Cloudy => Color32::from_rgb(0x3d, 0x7b, 0xa3),
            OrbKind::Overcast => Color32::from_rgb(0x4c, 0x6a, 0x80),
            OrbKind::Rain => Color32::from_rgb(0x2c, 0x5d, 0x92),
            OrbKind::Snow => Color32::from_rgb(0x7f, 0xb8, 0xd8),
            OrbKind::Storm => Color32::from_rgb(0x2e, 0x35, 0x60),
        }
    }

    fn water_b(self) -> Color32 {
        match self.orb_kind() {
            OrbKind::Sunny => Color32::from_rgb(0x3e, 0xc6, 0xc0),
            OrbKind::Cloudy => Color32::from_rgb(0x5a, 0x9c, 0xc4),
            OrbKind::Overcast => Color32::from_rgb(0x68, 0x85, 0x9a),
            OrbKind::Rain => Color32::from_rgb(0x3f, 0x78, 0xb4),
            OrbKind::Snow => Color32::from_rgb(0xa5, 0xd4, 0xec),
            OrbKind::Storm => Color32::from_rgb(0x41, 0x4a, 0x82),
        }
    }

    fn cloud_tint(self, alpha: u8) -> Color32 {
        match self.orb_kind() {
            OrbKind::Sunny | OrbKind::Cloudy => {
                Color32::from_rgba_unmultiplied(0xf4, 0xf8, 0xfd, alpha)
            }
            OrbKind::Overcast => Color32::from_rgba_unmultiplied(0xae, 0xb8, 0xc4, alpha),
            OrbKind::Rain => Color32::from_rgba_unmultiplied(0x8f, 0xa0, 0xb4, alpha),
            OrbKind::Snow => Color32::from_rgba_unmultiplied(0xdd, 0xe6, 0xef, alpha),
            OrbKind::Storm => Color32::from_rgba_unmultiplied(0x5b, 0x5f, 0x78, alpha),
        }
    }

    fn show_highlight(self) -> bool {
        matches!(self, Scene::Sunny)
    }

    /// Vue `precipParticles` dropCount
    fn drop_count(self) -> usize {
        match self {
            Scene::Drizzle => 6,
            Scene::Rain(Intensity::Light) => 6,
            Scene::Rain(Intensity::Moderate) => 10,
            Scene::Rain(Intensity::Heavy) => 14,
            Scene::Storm(Intensity::Light) => 8,
            Scene::Storm(Intensity::Moderate) => 12,
            Scene::Storm(Intensity::Heavy) => 16,
            _ => 0,
        }
    }

    fn drop_fast(self) -> bool {
        matches!(
            self,
            Scene::Rain(Intensity::Heavy)
                | Scene::Storm(Intensity::Light)
                | Scene::Storm(Intensity::Moderate)
                | Scene::Storm(Intensity::Heavy)
        )
    }

    fn flake_count(self) -> usize {
        match self {
            Scene::Snow(Intensity::Light) => 6,
            Scene::Snow(Intensity::Moderate) => 10,
            Scene::Snow(Intensity::Heavy) => 14,
            _ => 0,
        }
    }

    fn is_precip_rain(self) -> bool {
        matches!(self, Scene::Drizzle | Scene::Rain(_) | Scene::Storm(_))
    }

    fn is_snow(self) -> bool {
        matches!(self, Scene::Snow(_))
    }

    fn is_storm(self) -> bool {
        matches!(self, Scene::Storm(_))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OrbKind {
    Sunny,
    Cloudy,
    Overcast,
    Rain,
    Snow,
    Storm,
}

struct Drop {
    x: f32,
    y: f32,
    len: f32,
    width: f32,
    speed: f32,
    alpha: u8,
}

struct Flake {
    x: f32,
    y: f32,
    r: f32,
    speed: f32,
    sway: f32,
    phase: f32,
    alpha: u8,
}

struct CloudTextures {
    a: TextureHandle,
    b: TextureHandle,
    mist: TextureHandle,
}

struct OrbApp {
    quit: Arc<AtomicBool>,
    started: Instant,
    scene: Scene,
    drops: Vec<Drop>,
    flakes: Vec<Flake>,
    cloud_tex: CloudTextures,
    weather: Arc<Mutex<WeatherState>>,
    last_fetch: Instant,
    applied_scene: Option<Scene>,
    detail_open: bool,
    /// 0 = compact, 1 = fully expanded (animated).
    detail_t: f32,
    press_pos: Option<Pos2>,
    dragging: bool,
    /// CJK font bytes loaded off-thread: (bytes, ttc_index).
    pending_font: Arc<Mutex<Option<(Vec<u8>, u32)>>>,
    fonts_applied: bool,
    frames: u64,
    /// Shared with tray menu handler (Win32 show/hide).
    window_visible: Arc<AtomicBool>,
    main_hwnd: Arc<AtomicIsize>,
    open_at_login: Arc<AtomicBool>,
    saved_pos: Option<(i32, i32)>,
    last_saved_pos: Option<(i32, i32)>,
    pos_applied: bool,
    last_passthrough: Option<bool>,
    taskbar_hidden: bool,
    tray: Option<TrayUi>,
}

struct TrayUi {
    _icon: tray_icon::TrayIcon,
}

fn lock_mutex<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn spawn_weather_fetch(weather: Arc<Mutex<WeatherState>>) {
    {
        let mut w = lock_mutex(&weather);
        if w.loading {
            return;
        }
        w.loading = true;
        w.error = None;
    }
    thread::spawn(move || {
        let result = refresh_blocking();
        let mut w = lock_mutex(&weather);
        w.loading = false;
        match result {
            Ok(data) => {
                w.data = Some(data);
                w.error = None;
            }
            Err(e) => {
                w.error = Some(e);
            }
        }
    });
}

fn main() -> eframe::Result<()> {
    display::enable_per_monitor_dpi();
    if !display::take_instance_lock() {
        return Ok(());
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let _ = std::env::set_current_dir(dir);
        }
    }

    let quit = Arc::new(AtomicBool::new(false));
    let loaded = settings::AppSettings::load();
    let saved_pos = Some(
        loaded
            .position()
            .map(|(x, y)| display::clamp_saved_pos(x, y))
            .unwrap_or_else(display::default_orb_pos),
    );
    let open_at_login = Arc::new(AtomicBool::new(loaded.open_at_login));

    let mut viewport = egui::ViewportBuilder::default()
        // Logical size — OS/DPI scale turns this into physical pixels per monitor.
        .with_inner_size([W, H_DETAIL])
        .with_min_inner_size([W, H_DETAIL])
        .with_max_inner_size([W, H_DETAIL])
        .with_decorations(false)
        .with_transparent(true)
        .with_always_on_top()
        .with_resizable(false)
        .with_taskbar(false)
        .with_mouse_passthrough(false)
        .with_visible(true);
    if let Some((x, y)) = saved_pos {
        let [lx, ly] = display::physical_to_logical_pos(x, y);
        viewport = viewport.with_position([lx, ly]);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        centered: false,
        ..Default::default()
    };

    let open_at_login_ui = Arc::clone(&open_at_login);
    eframe::run_native(
        "天气球",
        native_options,
        Box::new(move |cc| {
            // Do NOT load CJK fonts here — reading msyh.ttc blocks the first frame for seconds.
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            let scene = Scene::Sunny;
            let weather = Arc::new(Mutex::new(WeatherState::default()));
            spawn_weather_fetch(Arc::clone(&weather));

            let pending_font = Arc::new(Mutex::new(None));
            let pending_font_bg = Arc::clone(&pending_font);
            thread::spawn(move || {
                if let Some(font) = load_cjk_font_bytes() {
                    *lock_mutex(&pending_font_bg) = Some(font);
                }
            });

            let window_visible = Arc::new(AtomicBool::new(true));
            let main_hwnd = Arc::new(AtomicIsize::new(0));

            Ok(Box::new(OrbApp {
                quit,
                started: Instant::now(),
                scene,
                drops: make_drops(scene),
                flakes: make_flakes(scene),
                cloud_tex: load_cloud_textures(&cc.egui_ctx),
                weather,
                last_fetch: Instant::now(),
                applied_scene: None,
                detail_open: false,
                detail_t: 0.0,
                press_pos: None,
                dragging: false,
                pending_font,
                fonts_applied: false,
                frames: 0,
                window_visible,
                main_hwnd,
                open_at_login: open_at_login_ui,
                saved_pos,
                last_saved_pos: saved_pos,
                pos_applied: false,
                last_passthrough: None,
                taskbar_hidden: false,
                tray: None,
            }))
        }),
    )
}

/// Prefer smaller CJK faces so atlas rebuild is quicker once applied.
fn load_cjk_font_bytes() -> Option<(Vec<u8>, u32)> {
    let candidates = [
        (r"C:\Windows\Fonts\simhei.ttf", 0u32),
        (r"C:\Windows\Fonts\msyh.ttc", 0u32),
        (r"C:\Windows\Fonts\simsun.ttc", 0u32),
    ];
    for (path, index) in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            eprintln!(
                "[weatherball-native] loaded font {} ({} KB)",
                path,
                bytes.len() / 1024
            );
            return Some((bytes, index));
        }
    }
    eprintln!("[weatherball-native] 未找到中文字体");
    None
}

fn apply_cjk_font(ctx: &egui::Context, bytes: Vec<u8>, index: u32) {
    let mut fonts = egui::FontDefinitions::default();
    let mut data = egui::FontData::from_owned(bytes);
    data.index = index;
    fonts.font_data.insert("cjk".to_owned(), Arc::new(data));

    if let Some(prop) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        prop.insert(0, "cjk".to_owned());
    }
    if let Some(mono) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
        mono.push("cjk".to_owned());
    }
    ctx.set_fonts(fonts);
}

impl eframe::App for OrbApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if self.quit.load(Ordering::Relaxed) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }

        if let Some(hwnd) = hwnd_from_frame(frame) {
            self.main_hwnd.store(hwnd, Ordering::Relaxed);
            if !self.pos_applied {
                if let Some((x, y)) = self.saved_pos {
                    let (x, y) = display::clamp_hwnd_pos(hwnd, x, y);
                    win_set_window_pos(hwnd, x, y);
                    self.saved_pos = Some((x, y));
                }
                self.pos_applied = true;
            } else if self.frames <= 8 && !self.dragging {
                // winit can overwrite the first placement; re-apply only if it drifted.
                if let Some((want_x, want_y)) = self.saved_pos {
                    if let Some((x, y, _, _)) = display::window_rect(hwnd) {
                        if (x - want_x).abs() > 2 || (y - want_y).abs() > 2 {
                            let pos = display::clamp_hwnd_pos(hwnd, want_x, want_y);
                            win_set_window_pos(hwnd, pos.0, pos.1);
                            self.saved_pos = Some(pos);
                        }
                    }
                }
            } else if !self.dragging && self.frames % 90 == 0 {
                // Monitor unplug / work-area change: snap the orb back on-screen.
                if let Some((x, y, _, _)) = display::window_rect(hwnd) {
                    let clamped = display::clamp_hwnd_pos_live(hwnd, x, y);
                    if (clamped.0 - x).abs() > 2 || (clamped.1 - y).abs() > 2 {
                        win_set_window_pos(hwnd, clamped.0, clamped.1);
                        self.saved_pos = Some(clamped);
                        self.persist_pos(clamped);
                    }
                }
            }
        }

        self.frames = self.frames.saturating_add(1);
        if self.frames == 1 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        }

        if self.tray.is_none() && self.frames >= 2 {
            let retry = self.frames == 2 || self.frames % 30 == 0;
            if retry && (display::shell_tray_ready() || self.frames >= 90) {
                self.tray = create_tray(
                    Arc::clone(&self.window_visible),
                    Arc::clone(&self.main_hwnd),
                    Arc::clone(&self.open_at_login),
                    ctx.clone(),
                )
                .ok();
            }
            if self.tray.is_none() && self.frames < 1200 {
                ctx.request_repaint_after(Duration::from_millis(200));
            }
        }

        let visible = self.window_visible.load(Ordering::Relaxed);

        // User hid the orb — skip painting; tray show will request_repaint.
        if !visible {
            return;
        }

        // Only touch OUR hwnd — EnumWindows waits on every top-level window and can freeze
        // the orb if Explorer or another app is busy.
        #[cfg(target_os = "windows")]
        if !self.taskbar_hidden {
            let hwnd = self.main_hwnd.load(Ordering::Relaxed);
            if hwnd != 0 && hide_hwnd_from_taskbar(hwnd) {
                self.taskbar_hidden = true;
            }
        }

        // Apply CJK font after the orb has already painted at least once.
        if !self.fonts_applied && self.frames >= 2 {
            let ready = lock_mutex(&self.pending_font).take();
            if let Some((bytes, index)) = ready {
                apply_cjk_font(ctx, bytes, index);
                self.fonts_applied = true;
            } else {
                // Keep polling until background loader finishes.
                ctx.request_repaint_after(Duration::from_millis(50));
            }
        }
        let (temp, desc, city, err, loading, new_scene, snapshot) = {
            let w = lock_mutex(&self.weather);
            let new_scene = w.data.as_ref().map(|d| d.scene).filter(|&s| {
                self.applied_scene != Some(s)
            });
            let snapshot = if self.detail_t > 0.02 {
                w.data.clone()
            } else {
                None
            };
            (
                w.data.as_ref().map(|d| d.temperature),
                w.data
                    .as_ref()
                    .map(|d| d.description.clone())
                    .unwrap_or_default(),
                w.data
                    .as_ref()
                    .map(|d| d.city.clone())
                    .unwrap_or_default(),
                w.error.clone(),
                w.loading,
                new_scene,
                snapshot,
            )
        };
        if let Some(scene) = new_scene {
            self.scene = scene;
            self.drops = make_drops(scene);
            self.flakes = make_flakes(scene);
            self.applied_scene = Some(scene);
        }

        if !loading && self.last_fetch.elapsed() > Duration::from_secs(REFRESH_SECS) {
            self.last_fetch = Instant::now();
            spawn_weather_fetch(Arc::clone(&self.weather));
        }

        let t = self.started.elapsed().as_secs_f32();
        let dt = ctx.input(|i| i.stable_dt).min(0.05);
        let bob = (t * TAU / 5.5).sin() * 2.5;

        match self.scene {
            s if s.is_precip_rain() => {
                advance_drops(&mut self.drops, dt, s.drop_fast());
            }
            s if s.is_snow() => advance_flakes(&mut self.flakes, dt, s.intensity()),
            _ => {}
        }

        // Animate detail expand/collapse (Vue-like ease + fade).
        let target_t = if self.detail_open { 1.0 } else { 0.0 };
        if (self.detail_t - target_t).abs() > 0.0005 {
            let step = dt / DETAIL_ANIM_SECS;
            if self.detail_t < target_t {
                self.detail_t = (self.detail_t + step).min(target_t);
            } else {
                self.detail_t = (self.detail_t - step).max(target_t);
            }
        } else {
            self.detail_t = target_t;
        }

        let height_ease = ease_in_out_cubic(self.detail_t);
        // Opacity finishes sooner so the panel is gone before it would feel clipped.
        let panel_opacity = smoothstep(0.0, 0.55, self.detail_t);
        // Window size stays H_DETAIL forever — only the panel clip/opacity animates.
        let win_h = H_DETAIL;
        let compact_ui = self.detail_t < 0.18;
        let still_animating = (self.detail_t - target_t).abs() > 0.0005;

        let mut pointer_busy = false;

        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                ui.set_min_size(Vec2::new(W, win_h));
                let rect = ui.max_rect();

                // Fixed client Y — window never resizes, so screen position stays put.
                let center = Pos2::new(rect.min.x + rect.width() * 0.5, BALL_CENTER_Y + bob);

                let mut panel_rect: Option<Rect> = None;
                let mut close_clicked = false;
                let mut refresh_clicked = false;

                // Draw panel under the orb so any fade residue never sits on the ball.
                if panel_opacity > 0.02 {
                    let top = BALL_CENTER_Y + BALL_R + 18.0;
                    let full_bottom = H_DETAIL - 10.0;
                    let full_h = (full_bottom - top).max(0.0);
                    let visible_h = (full_h * height_ease).max(0.0);
                    let bottom = top + visible_h;
                    if bottom > top + 4.0 {
                        let pr = Rect::from_min_max(
                            Pos2::new(8.0, top),
                            Pos2::new(W - 8.0, bottom),
                        );
                        let interactive = panel_opacity > 0.85 && self.detail_open;
                        let actions = paint_detail_panel(
                            ui,
                            pr,
                            snapshot.as_ref(),
                            loading,
                            panel_opacity,
                            interactive,
                        );
                        panel_rect = Some(pr);
                        close_clicked = actions.close;
                        refresh_clicked = actions.refresh;
                    }
                }
                if close_clicked {
                    self.detail_open = false;
                }
                if refresh_clicked && !loading {
                    self.last_fetch = Instant::now();
                    spawn_weather_fetch(Arc::clone(&self.weather));
                }

                paint_orb(
                    ui,
                    center,
                    t,
                    self.scene,
                    &self.drops,
                    &self.flakes,
                    &self.cloud_tex,
                );

                let btn_rect = Rect::NOTHING;

                let (over_ball, over_btn, over_panel) = interactive_hits(
                    self.main_hwnd.load(Ordering::Relaxed),
                    ctx,
                    center,
                    BALL_R + 2.0,
                    btn_rect,
                    panel_rect.filter(|_| panel_opacity > 0.5),
                );
                pointer_busy = over_ball || over_btn || over_panel;

                let hit = Rect::from_center_size(center, Vec2::splat(BALL_R * 2.0));
                let ball = ui.interact(hit, ui.id().with("orb-drag"), Sense::click_and_drag());

                if over_ball {
                    if ball.drag_started() {
                        self.press_pos = ball.interact_pointer_pos();
                        self.dragging = false;
                    }
                    if ball.dragged() {
                        if let (Some(start), Some(cur)) =
                            (self.press_pos, ball.interact_pointer_pos())
                        {
                            if !self.dragging && (cur - start).length() > DRAG_THRESHOLD {
                                self.dragging = true;
                                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                            }
                        }
                    }
                    if ball.drag_stopped() {
                        self.press_pos = None;
                        self.dragging = false;
                        self.persist_window_pos();
                    }
                    // egui clicked() is false when the pointer dragged
                    if ball.clicked() {
                        self.detail_open = !self.detail_open;
                    }
                }

                // Sending MousePassthrough every frame storms Win32 and can freeze the orb.
                let passthrough =
                    !self.dragging && !(over_ball || over_btn || over_panel);
                if self.last_passthrough != Some(passthrough) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(passthrough));
                    self.last_passthrough = Some(passthrough);
                }

                // Tooltip only when mostly compact
                if over_ball && compact_ui {
                    paint_tooltip(
                        ui,
                        Pos2::new(center.x, center.y - BALL_R - 10.0),
                        rect,
                        temp,
                        &desc,
                        &city,
                        err.as_deref(),
                        loading,
                    );
                }
            });

        let precip = self.scene.is_precip_rain() || self.scene.is_snow();
        let ms = if !self.fonts_applied {
            50
        } else if self.dragging || still_animating || precip || pointer_busy {
            33
        } else {
            70
        };
        ctx.request_repaint_after(Duration::from_millis(ms));
    }
}

impl OrbApp {
    fn persist_window_pos(&mut self) {
        let hwnd = self.main_hwnd.load(Ordering::Relaxed);
        if let Some((x, y, _, _)) = display::window_rect(hwnd) {
            let pos = display::clamp_hwnd_pos(hwnd, x, y);
            if pos != (x, y) {
                win_set_window_pos(hwnd, pos.0, pos.1);
            }
            self.persist_pos(pos);
        }
    }

    fn persist_pos(&mut self, pos: (i32, i32)) {
        if self.last_saved_pos == Some(pos) {
            return;
        }
        settings::save_position(pos.0, pos.1);
        self.last_saved_pos = Some(pos);
    }
}

/// Tray clicks are handled off the GUI thread so hide/quit stay responsive.
/// Menu text is applied on the GUI thread (MenuItem is not Send).
fn spawn_tray_poller(
    window_visible: Arc<AtomicBool>,
    main_hwnd: Arc<AtomicIsize>,
    open_at_login: Arc<AtomicBool>,
    tray_hmenu: Arc<AtomicIsize>,
    ctx: egui::Context,
) {
    thread::spawn(move || {
        use tray_icon::menu::MenuEvent;
        loop {
            let Ok(event) = MenuEvent::receiver().recv() else {
                break;
            };
            match event.id.as_ref() {
                "toggle" => {
                    let h = main_hwnd.load(Ordering::Relaxed);
                    if h == 0 {
                        continue;
                    }
                    let show = !window_visible.load(Ordering::Relaxed);
                    window_visible.store(show, Ordering::Relaxed);
                    win_set_window_visible(h, show);
                    ctx.request_repaint();
                    schedule_menu_text(Arc::clone(&tray_hmenu), 0, toggle_label(show));
                }
                "autostart" => {
                    let next = !open_at_login.load(Ordering::Relaxed);
                    let flag = Arc::clone(&open_at_login);
                    let hmenu = Arc::clone(&tray_hmenu);
                    thread::spawn(move || {
                        thread::sleep(Duration::from_millis(80));
                        let enabled = settings::set_open_at_login(next);
                        flag.store(enabled, Ordering::Relaxed);
                        thread::sleep(Duration::from_millis(120));
                        win_set_popup_item_checked(hmenu.load(Ordering::Relaxed), 2, enabled);
                    });
                }
                "quit" => {
                    let h = main_hwnd.load(Ordering::Relaxed);
                    if let Some((x, y, _, _)) = display::window_rect(h) {
                        let pos = display::clamp_hwnd_pos(h, x, y);
                        settings::save_position(pos.0, pos.1);
                    }
                    std::process::exit(0);
                }
                _ => {}
            }
        }
    });
}

fn schedule_menu_text(hmenu: Arc<AtomicIsize>, pos: u32, text: &'static str) {
    thread::spawn(move || {
        // TrackPopupMenu is still on the stack when MenuEvent fires; wait it out.
        thread::sleep(Duration::from_millis(200));
        win_set_popup_item_text(hmenu.load(Ordering::Relaxed), pos, text);
    });
}

fn toggle_label(visible: bool) -> &'static str {
    if visible {
        "隐藏天气球"
    } else {
        "显示天气球"
    }
}

#[cfg(target_os = "windows")]
fn win_set_popup_item_text(hmenu: isize, pos: u32, text: &str) {
    if hmenu == 0 {
        return;
    }
    use std::ffi::c_void;
    const MIIM_STRING: u32 = 0x40;
    #[repr(C)]
    struct MenuItemInfoW {
        cb_size: u32,
        f_mask: u32,
        f_type: u32,
        f_state: u32,
        w_id: u32,
        h_sub_menu: *mut c_void,
        hbmp_checked: *mut c_void,
        hbmp_unchecked: *mut c_void,
        dw_item_data: usize,
        dw_type_data: *mut u16,
        cch: u32,
        hbmp_item: *mut c_void,
    }
    extern "system" {
        fn SetMenuItemInfoW(
            hmenu: *mut c_void,
            item: u32,
            by_position: i32,
            info: *const MenuItemInfoW,
        ) -> i32;
    }
    let mut wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    let info = MenuItemInfoW {
        cb_size: std::mem::size_of::<MenuItemInfoW>() as u32,
        f_mask: MIIM_STRING,
        f_type: 0,
        f_state: 0,
        w_id: 0,
        h_sub_menu: std::ptr::null_mut(),
        hbmp_checked: std::ptr::null_mut(),
        hbmp_unchecked: std::ptr::null_mut(),
        dw_item_data: 0,
        dw_type_data: wide.as_mut_ptr(),
        cch: 0,
        hbmp_item: std::ptr::null_mut(),
    };
    unsafe {
        SetMenuItemInfoW(hmenu as *mut c_void, pos, 1, &info);
    }
}

#[cfg(not(target_os = "windows"))]
fn win_set_popup_item_text(_hmenu: isize, _pos: u32, _text: &str) {}

fn hwnd_from_frame(frame: &eframe::Frame) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let raw = frame.window_handle().ok()?.as_raw();
    match raw {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get() as isize),
        _ => None,
    }
}

#[cfg(target_os = "windows")]
fn win_set_window_visible(hwnd: isize, visible: bool) {
    use std::ffi::c_void;
    const SW_HIDE: i32 = 0;
    const SW_SHOWNA: i32 = 8;
    extern "system" {
        fn ShowWindow(h_wnd: *mut c_void, n_cmd_show: i32) -> i32;
    }
    unsafe {
        ShowWindow(
            hwnd as *mut c_void,
            if visible { SW_SHOWNA } else { SW_HIDE },
        );
    }
}

#[cfg(target_os = "windows")]
fn win_set_window_pos(hwnd: isize, x: i32, y: i32) {
    if hwnd == 0 {
        return;
    }
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_NOACTIVATE: u32 = 0x0010;
    extern "system" {
        fn SetWindowPos(
            h_wnd: *mut std::ffi::c_void,
            insert_after: *mut std::ffi::c_void,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> i32;
    }
    unsafe {
        SetWindowPos(
            hwnd as *mut std::ffi::c_void,
            std::ptr::null_mut(),
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

#[cfg(not(target_os = "windows"))]
fn win_set_window_pos(_hwnd: isize, _x: i32, _y: i32) {}

#[cfg(not(target_os = "windows"))]
fn win_set_window_visible(_hwnd: isize, _visible: bool) {}

fn ease_in_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

fn with_opacity(c: Color32, opacity: f32) -> Color32 {
    let a = (c.a() as f32 * opacity.clamp(0.0, 1.0)).round() as u8;
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

/// Screen-space cursor vs orb / button / detail panel.
/// Cursor and window rect are physical pixels; layout is logical points × window DPI.
fn interactive_hits(
    hwnd: isize,
    ctx: &egui::Context,
    ball_center: Pos2,
    ball_r: f32,
    btn_rect: Rect,
    panel_rect: Option<Rect>,
) -> (bool, bool, bool) {
    let Some(cursor) = screen_cursor_pos() else {
        return (false, false, false);
    };
    let (origin_x, origin_y, scale) = display::window_origin_and_scale(hwnd).unwrap_or_else(|| {
        let ppp = ctx.pixels_per_point().max(0.5);
        let origin = ctx
            .input(|i| i.viewport().outer_rect)
            .map(|r| r.min)
            .unwrap_or(Pos2::ZERO);
        (origin.x * ppp, origin.y * ppp, ppp)
    });
    let to_screen = |p: Pos2| Pos2::new(origin_x + p.x * scale, origin_y + p.y * scale);
    let ball_screen = to_screen(ball_center);
    let over_ball = (cursor - ball_screen).length() <= ball_r * scale;
    let over_btn = if btn_rect.width() > 1.0 {
        let min = to_screen(btn_rect.min);
        let max = to_screen(btn_rect.max);
        Rect::from_min_max(min, max).contains(cursor)
    } else {
        false
    };
    let over_panel = panel_rect
        .map(|pr| {
            let min = to_screen(pr.min);
            let max = to_screen(pr.max);
            Rect::from_min_max(min, max).contains(cursor)
        })
        .unwrap_or(false);
    (over_ball, over_btn, over_panel)
}

fn screen_cursor_pos() -> Option<Pos2> {
    #[cfg(target_os = "windows")]
    {
        #[repr(C)]
        struct Point {
            x: i32,
            y: i32,
        }
        extern "system" {
            fn GetCursorPos(point: *mut Point) -> i32;
        }
        let mut pt = Point { x: 0, y: 0 };
        unsafe {
            if GetCursorPos(&mut pt) != 0 {
                return Some(Pos2::new(pt.x as f32, pt.y as f32));
            }
        }
        None
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

struct DetailActions {
    close: bool,
    refresh: bool,
}

fn paint_detail_panel(
    ui: &mut egui::Ui,
    rect: Rect,
    data: Option<&LiveWeather>,
    loading: bool,
    opacity: f32,
    interactive: bool,
) -> DetailActions {
    let mut actions = DetailActions {
        close: false,
        refresh: false,
    };
    if opacity < 0.02 {
        return actions;
    }

    let clip = ui.clip_rect().intersect(rect);
    let old_clip = ui.clip_rect();
    ui.set_clip_rect(clip);

    {
        let p = ui.painter();
        p.rect_filled(
            rect,
            16.0,
            with_opacity(Color32::from_rgba_unmultiplied(12, 18, 32, 235), opacity),
        );
        p.rect_stroke(
            rect,
            16.0,
            Stroke::new(
                1.0_f32,
                with_opacity(Color32::from_rgba_unmultiplied(255, 255, 255, 56), opacity),
            ),
        );
    }

    let pad = 12.0;
    let inner = rect.shrink(pad);
    let mut y = inner.min.y;

    // City row + close
    let city = data.map(|d| d.city.as_str()).unwrap_or("定位中…");
    let close_r = Rect::from_min_size(
        Pos2::new(inner.max.x - 22.0, y),
        Vec2::new(22.0, 20.0),
    );
    let close = if interactive {
        ui.allocate_rect(close_r, Sense::click())
    } else {
        ui.allocate_rect(close_r, Sense::hover())
    };
    {
        let p = ui.painter();
        p.rect_filled(
            close_r,
            8.0,
            with_opacity(
                if close.hovered() && interactive {
                    Color32::from_rgba_unmultiplied(255, 255, 255, 40)
                } else {
                    Color32::from_rgba_unmultiplied(255, 255, 255, 20)
                },
                opacity,
            ),
        );
        p.text(
            close_r.center(),
            egui::Align2::CENTER_CENTER,
            "×",
            egui::FontId::proportional(14.0),
            with_opacity(Color32::from_rgba_unmultiplied(255, 255, 255, 200), opacity),
        );
        p.text(
            Pos2::new(inner.min.x, y + 2.0),
            egui::Align2::LEFT_TOP,
            city,
            egui::FontId::proportional(13.0),
            with_opacity(Color32::from_rgb(244, 247, 251), opacity),
        );
        let accent = data
            .map(|d| d.scene.glow())
            .unwrap_or(Color32::from_rgb(127, 180, 240));
        p.circle_filled(
            Pos2::new(inner.max.x - 34.0, y + 10.0),
            3.5,
            with_opacity(
                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 230),
                opacity,
            ),
        );
    }
    if interactive && close.clicked() {
        actions.close = true;
    }
    y += 24.0;

    // Temperature
    let temp_s = data
        .map(|d| format!("{}°", d.temperature.round() as i32))
        .unwrap_or_else(|| "--".into());
    {
        let p = ui.painter();
        p.text(
            Pos2::new(inner.min.x, y),
            egui::Align2::LEFT_TOP,
            temp_s,
            egui::FontId::proportional(36.0),
            with_opacity(Color32::from_rgb(244, 247, 251), opacity),
        );
    }
    y += 42.0;

    // Description
    let desc = data.map(|d| d.description.as_str()).unwrap_or("—");
    {
        let p = ui.painter();
        p.text(
            Pos2::new(inner.min.x, y),
            egui::Align2::LEFT_TOP,
            desc,
            egui::FontId::proportional(12.0),
            with_opacity(Color32::from_rgba_unmultiplied(220, 230, 245, 230), opacity),
        );
    }
    y += 20.0;

    // Meta row
    let humidity = data
        .and_then(|d| d.humidity)
        .map(|h| format!("湿度 {h}%"))
        .unwrap_or_else(|| "湿度 --".into());
    let feels = data
        .and_then(|d| d.feels_like)
        .map(|f| format!("体感 {}°", f.round() as i32))
        .unwrap_or_else(|| "体感 --".into());
    let wind = data
        .and_then(|d| d.wind_speed)
        .map(|w| format!("风速 {} km/h", w.round() as i32))
        .unwrap_or_else(|| "风速 --".into());
    let meta = format!("{humidity}  {feels}");
    {
        let p = ui.painter();
        p.text(
            Pos2::new(inner.min.x, y),
            egui::Align2::LEFT_TOP,
            meta,
            egui::FontId::proportional(10.0),
            with_opacity(Color32::from_rgba_unmultiplied(170, 185, 210, 220), opacity),
        );
        y += 14.0;
        p.text(
            Pos2::new(inner.min.x, y),
            egui::Align2::LEFT_TOP,
            wind,
            egui::FontId::proportional(10.0),
            with_opacity(Color32::from_rgba_unmultiplied(170, 185, 210, 220), opacity),
        );
    }
    y += 18.0;

    // Hourly sparkline (Vue HourlyCurve layout)
    if let Some(data) = data {
        if !data.hourly.is_empty() {
            let chart = Rect::from_min_size(
                Pos2::new(inner.min.x, y),
                Vec2::new(inner.width(), 96.0),
            );
            paint_hourly_curve(ui, chart, &data.hourly, opacity);
            y = chart.max.y + 8.0;
        }
    }

    // Updated time
    let updated = data
        .map(|d| format!("更新时间 {}", d.updated_hm))
        .unwrap_or_else(|| "更新时间 --:--".into());
    {
        let p = ui.painter();
        p.text(
            Pos2::new(inner.min.x, y),
            egui::Align2::LEFT_TOP,
            updated,
            egui::FontId::proportional(10.0),
            with_opacity(Color32::from_rgba_unmultiplied(150, 165, 190, 200), opacity),
        );
    }
    y += 22.0;

    // Refresh button
    let refresh_r = Rect::from_min_size(
        Pos2::new(inner.min.x, y.min(inner.max.y - 30.0)),
        Vec2::new(inner.width(), 28.0),
    );
    let refresh = if interactive {
        ui.allocate_rect(refresh_r, Sense::click())
    } else {
        ui.allocate_rect(refresh_r, Sense::hover())
    };
    paint_button(
        ui,
        refresh_r,
        if loading { "刷新中…" } else { "刷新天气" },
        refresh.hovered() && interactive,
        opacity,
    );
    if interactive && refresh.clicked() && !loading {
        actions.refresh = true;
    }

    ui.set_clip_rect(old_clip);
    actions
}

fn paint_hourly_curve(ui: &mut egui::Ui, rect: Rect, points: &[HourlyPoint], opacity: f32) {
    if points.len() < 2 {
        return;
    }
    let p = ui.painter();

    let mut min_t = points
        .iter()
        .map(|pt| pt.temperature)
        .fold(f32::INFINITY, f32::min);
    let mut max_t = points
        .iter()
        .map(|pt| pt.temperature)
        .fold(f32::NEG_INFINITY, f32::max);
    if max_t - min_t < 1.0 {
        min_t -= 0.5;
        max_t += 0.5;
    }
    let span = (max_t - min_t).max(1.0);
    let low = min_t.round() as i32;
    let high = max_t.round() as i32;
    let start = &points[0];
    let end = &points[points.len() - 1];

    let mut y = rect.min.y;
    p.text(
        Pos2::new(rect.min.x, y),
        egui::Align2::LEFT_TOP,
        format!("气温走势 · {}小时", points.len()),
        egui::FontId::proportional(10.0),
        with_opacity(Color32::from_rgba_unmultiplied(255, 255, 255, 178), opacity),
    );
    y += 14.0;
    p.text(
        Pos2::new(rect.min.x, y),
        egui::Align2::LEFT_TOP,
        format!("最低 {low}° · 最高 {high}°"),
        egui::FontId::proportional(9.0),
        with_opacity(Color32::from_rgba_unmultiplied(255, 255, 255, 122), opacity),
    );
    y += 14.0;

    let spark = Rect::from_min_size(Pos2::new(rect.min.x, y), Vec2::new(rect.width(), 40.0));
    let pad_x = 2.0;
    let pad_top = 6.0;
    let pad_bottom = 4.0;
    let inner = Rect::from_min_max(
        Pos2::new(spark.min.x + pad_x, spark.min.y + pad_top),
        Pos2::new(spark.max.x - pad_x, spark.max.y - pad_bottom),
    );

    let n = points.len();
    let mut path: Vec<Pos2> = Vec::with_capacity(n);
    for (i, pt) in points.iter().enumerate() {
        let x = if n == 1 {
            inner.center().x
        } else {
            inner.min.x + inner.width() * (i as f32) / (n - 1) as f32
        };
        let t = (pt.temperature - min_t) / span;
        let py = inner.max.y - inner.height() * t;
        path.push(Pos2::new(x, py));
    }

    // Soft fill under the curve
    if let (Some(&first), Some(&last)) = (path.first(), path.last()) {
        let mut fill = path.clone();
        fill.push(Pos2::new(last.x, spark.max.y - 1.0));
        fill.push(Pos2::new(first.x, spark.max.y - 1.0));
        p.add(Shape::Path(egui::epaint::PathShape {
            points: fill,
            closed: true,
            fill: with_opacity(Color32::from_rgba_unmultiplied(127, 180, 240, 56), opacity),
            stroke: Stroke::NONE.into(),
        }));
    }

    for w in path.windows(2) {
        p.line_segment(
            [w[0], w[1]],
            Stroke::new(
                1.5_f32,
                with_opacity(Color32::from_rgb(127, 180, 240), opacity),
            ),
        );
    }
    for pt in &path {
        p.circle_filled(
            *pt,
            1.6,
            with_opacity(Color32::from_rgba_unmultiplied(255, 255, 255, 235), opacity),
        );
        p.circle_stroke(
            *pt,
            1.6,
            Stroke::new(
                0.6_f32,
                with_opacity(Color32::from_rgb(127, 180, 240), opacity),
            ),
        );
    }

    y = spark.max.y + 4.0;
    let start_s = format!(
        "{}时 {}°",
        start.hour,
        start.temperature.round() as i32
    );
    let end_s = format!("{}时 {}°", end.hour, end.temperature.round() as i32);
    p.text(
        Pos2::new(rect.min.x, y),
        egui::Align2::LEFT_TOP,
        start_s,
        egui::FontId::proportional(9.0),
        with_opacity(Color32::from_rgba_unmultiplied(255, 255, 255, 128), opacity),
    );
    p.text(
        Pos2::new(rect.max.x, y),
        egui::Align2::RIGHT_TOP,
        end_s,
        egui::FontId::proportional(9.0),
        with_opacity(Color32::from_rgba_unmultiplied(255, 255, 255, 128), opacity),
    );
}

fn paint_tooltip(
    ui: &mut egui::Ui,
    anchor_bottom: Pos2,
    bounds: Rect,
    temp: Option<f32>,
    description: &str,
    city: &str,
    error: Option<&str>,
    loading: bool,
) {
    #[derive(Clone, Copy)]
    enum LineKind {
        Temp,
        Body,
    }

    let mut lines: Vec<(String, Color32, f32, LineKind)> = Vec::new();

    if loading && temp.is_none() {
        lines.push((
            "获取中…".into(),
            Color32::from_rgba_unmultiplied(180, 190, 210, 220),
            12.0,
            LineKind::Body,
        ));
    } else if let (None, Some(err)) = (temp, error) {
        lines.push((
            err.to_string(),
            Color32::from_rgba_unmultiplied(255, 180, 140, 240),
            11.0,
            LineKind::Body,
        ));
        if !city.is_empty() {
            lines.push((
                city.to_string(),
                Color32::from_rgba_unmultiplied(160, 170, 190, 200),
                11.0,
                LineKind::Body,
            ));
        }
    } else {
        let temp_s = temp
            .map(|t| format!("{}", t.round() as i32))
            .unwrap_or_else(|| "--".into());
        lines.push((
            temp_s,
            Color32::from_rgb(244, 247, 251),
            22.0,
            LineKind::Temp,
        ));
        if !description.is_empty() {
            lines.push((
                description.to_string(),
                Color32::from_rgba_unmultiplied(230, 235, 245, 235),
                12.0,
                LineKind::Body,
            ));
        }
        if !city.is_empty() {
            lines.push((
                city.to_string(),
                Color32::from_rgba_unmultiplied(160, 170, 190, 200),
                11.0,
                LineKind::Body,
            ));
        }
        if let Some(err) = error {
            lines.push((
                err.to_string(),
                Color32::from_rgba_unmultiplied(255, 180, 140, 220),
                11.0,
                LineKind::Body,
            ));
        }
    }

    let pad_x = 16.0;
    let pad_y = 10.0;
    let gap = 3.0;

    // Pre-measure: temp line is digits + "°" drawn separately for optical center
    let mut row_w = 64.0_f32;
    let mut row_h = Vec::with_capacity(lines.len());
    for (text, color, size, kind) in &lines {
        let font = egui::FontId::proportional(*size);
        match kind {
            LineKind::Temp => {
                let num = ui.fonts(|f| f.layout_no_wrap(text.clone(), font.clone(), *color));
                let deg = ui.fonts(|f| {
                    f.layout_no_wrap("°".to_owned(), font, Color32::from_rgb(244, 247, 251))
                });
                row_w = row_w.max(num.size().x + deg.size().x * 0.55);
                row_h.push(num.size().y.max(24.0));
            }
            LineKind::Body => {
                let g = ui.fonts(|f| f.layout_no_wrap(text.clone(), font, *color));
                row_w = row_w.max(g.size().x);
                row_h.push(g.size().y.max(14.0));
            }
        }
    }

    let width = row_w + pad_x * 2.0;
    let height = pad_y * 2.0 + row_h.iter().sum::<f32>() + gap * (lines.len().saturating_sub(1) as f32);

    // Prefer above the ball; if clipped by window top, shift down so full card is visible
    let margin = 6.0;
    let mut tip = Rect::from_center_size(
        Pos2::new(anchor_bottom.x, anchor_bottom.y - height * 0.5),
        Vec2::new(width, height),
    );
    if tip.min.y < bounds.min.y + margin {
        tip = tip.translate(Vec2::new(0.0, bounds.min.y + margin - tip.min.y));
    }
    if tip.max.x > bounds.max.x - margin {
        tip = tip.translate(Vec2::new(bounds.max.x - margin - tip.max.x, 0.0));
    }
    if tip.min.x < bounds.min.x + margin {
        tip = tip.translate(Vec2::new(bounds.min.x + margin - tip.min.x, 0.0));
    }

    // Draw outside default clip so rounded top isn't cut by panel edges
    let p = ui.painter().with_clip_rect(bounds.expand(2.0));
    p.rect_filled(tip, 14.0, Color32::from_rgba_unmultiplied(12, 18, 32, 242));
    p.rect_stroke(
        tip,
        14.0,
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 50)),
    );

    let cx = tip.center().x;
    let mut y = tip.min.y + pad_y;
    for (i, (text, color, size, kind)) in lines.iter().enumerate() {
        let font = egui::FontId::proportional(*size);
        let h = row_h[i];
        match kind {
            LineKind::Temp => {
                // Center on digits; place "°" tight to the right (avoids ° pulling visual center)
                let num = ui.fonts(|f| f.layout_no_wrap(text.clone(), font.clone(), *color));
                let deg = ui.fonts(|f| {
                    f.layout_no_wrap("°".to_owned(), font, *color)
                });
                let pair_w = num.size().x + deg.size().x * 0.35;
                let num_x = cx - pair_w * 0.5;
                let num_y = y + (h - num.size().y) * 0.5;
                p.galley(Pos2::new(num_x, num_y), num.clone(), *color);
                p.galley(
                    Pos2::new(num_x + num.size().x - 1.0, num_y - 1.0),
                    deg,
                    *color,
                );
            }
            LineKind::Body => {
                let g = ui.fonts(|f| f.layout_no_wrap(text.clone(), font, *color));
                let x = cx - g.size().x * 0.5;
                let gy = y + (h - g.size().y) * 0.5;
                p.galley(Pos2::new(x, gy), g, *color);
            }
        }
        y += h + gap;
    }
}

fn make_drops(scene: Scene) -> Vec<Drop> {
    let n = scene.drop_count();
    if n == 0 {
        return Vec::new();
    }
    let intensity = scene.intensity().unwrap_or(Intensity::Moderate);
    let fast = scene.drop_fast();
    let drizzle = matches!(scene, Scene::Drizzle);

    (0..n)
        .map(|i| {
            let f = i as f32;
            // Vue makeDrop height / speed by intensity
            let (len, width, speed) = if fast || intensity == Intensity::Heavy {
                (
                    8.0 + (i % 7) as f32,   // ~8–14
                    1.7_f32,
                    1.15 + (i % 5) as f32 * 0.12,
                )
            } else if intensity == Intensity::Light || drizzle {
                (
                    4.0 + (i % 5) as f32 * 0.8, // ~4–8
                    1.15_f32,
                    0.45 + (i % 5) as f32 * 0.06,
                )
            } else {
                (
                    6.0 + (i % 6) as f32 * 0.85, // ~6–11
                    1.4_f32,
                    0.7 + (i % 5) as f32 * 0.1,
                )
            };
            Drop {
                x: ((f * 0.618) % 1.0) * 1.6 - 0.8,
                y: ((f * 0.37) % 1.0) * 1.6 - 0.9,
                len,
                width,
                speed,
                alpha: 120 + ((i * 19) % 90) as u8,
            }
        })
        .collect()
}

fn make_flakes(scene: Scene) -> Vec<Flake> {
    let n = scene.flake_count();
    if n == 0 {
        return Vec::new();
    }
    let intensity = scene.intensity().unwrap_or(Intensity::Moderate);
    let (r_base, speed_base) = match intensity {
        Intensity::Light => (1.2, 0.14),
        Intensity::Moderate => (1.5, 0.2),
        Intensity::Heavy => (1.9, 0.28),
    };
    (0..n)
        .map(|i| {
            let f = i as f32;
            Flake {
                x: ((f * 0.618) % 1.0) * 1.6 - 0.8,
                y: ((f * 0.41) % 1.0) * 1.7 - 0.95,
                r: r_base + (i % 4) as f32 * 0.45,
                speed: speed_base + (i % 6) as f32 * 0.035,
                sway: 8.0 + (i % 5) as f32 * 2.0,
                phase: f * 0.9,
                alpha: 140 + ((i * 23) % 90) as u8,
            }
        })
        .collect()
}

fn advance_drops(drops: &mut [Drop], dt: f32, fast: bool) {
    let mul = if fast { 1.35 } else { 1.0 };
    for d in drops.iter_mut() {
        d.y += d.speed * dt * mul;
        d.x += d.speed * dt * if fast { 0.22 } else { 0.14 };
        if d.y > 0.9 || d.x > 0.9 {
            d.y = -0.9;
            d.x = ((d.x + 1.6) % 1.6) - 0.8;
        }
    }
}

fn advance_flakes(flakes: &mut [Flake], dt: f32, intensity: Option<Intensity>) {
    let mul = match intensity {
        Some(Intensity::Heavy) => 1.35,
        Some(Intensity::Light) => 0.85,
        _ => 1.0,
    };
    for f in flakes.iter_mut() {
        f.y += f.speed * dt * mul;
        f.phase += dt * 1.2;
        if f.y > 0.95 {
            f.y = -0.95;
            f.x = ((f.x + 1.3) % 1.6) - 0.8;
        }
    }
}

fn paint_button(ui: &mut egui::Ui, rect: Rect, label: &str, hovered: bool, opacity: f32) {
    let p = ui.painter();
    let bg = if hovered {
        Color32::from_rgba_unmultiplied(40, 55, 75, 210)
    } else {
        Color32::from_rgba_unmultiplied(28, 40, 58, 190)
    };
    p.rect_filled(rect, 8.0, with_opacity(bg, opacity));
    p.rect_stroke(
        rect,
        8.0,
        Stroke::new(
            1.0_f32,
            with_opacity(Color32::from_rgba_unmultiplied(160, 190, 230, 90), opacity),
        ),
    );
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(12.0),
        with_opacity(Color32::from_rgba_unmultiplied(230, 240, 255, 240), opacity),
    );
}

fn inside_ball(center: Pos2, p: Pos2, margin: f32) -> bool {
    (p - center).length() <= BALL_R - margin
}

fn paint_orb(
    ui: &mut egui::Ui,
    center: Pos2,
    t: f32,
    scene: Scene,
    drops: &[Drop],
    flakes: &[Flake],
    clouds: &CloudTextures,
) {
    // 1) Outer glow
    {
        let p = ui.painter();
        let glow = scene.glow();
        let pulse = 0.85 + 0.15 * (t * TAU / 7.0).sin();
        for (i, (scale, a_mul)) in [(1.35, 0.35), (1.2, 0.55), (1.08, 0.75)].iter().enumerate() {
            let a = ((glow.a() as f32) * a_mul * pulse * (1.0 - i as f32 * 0.12)) as u8;
            p.circle_filled(
                center,
                BALL_R * scale,
                Color32::from_rgba_unmultiplied(glow.r(), glow.g(), glow.b(), a),
            );
        }
    }

    // 2) Interior (water under weather FX — Vue draws waves after layers but below glass)
    // Vue order: weather layers then waves. Waves sit lower so paint water first then FX on top
    // except waves should show in lower half — paint water first, then scene.
    paint_water(ui, center, t, scene);

    match scene.orb_kind() {
        OrbKind::Sunny => paint_sunny(ui, center, t),
        OrbKind::Cloudy => paint_clouds(ui, center, t, clouds, scene, false),
        OrbKind::Overcast => paint_clouds(ui, center, t, clouds, scene, true),
        OrbKind::Rain => {
            paint_clouds(ui, center, t, clouds, scene, true);
            paint_rain_drops(ui, center, drops, scene);
        }
        OrbKind::Snow => {
            paint_clouds(ui, center, t, clouds, scene, false);
            paint_flakes(ui, center, t, flakes, scene.flake_count());
        }
        OrbKind::Storm => {
            paint_clouds(ui, center, t, clouds, scene, false);
            paint_rain_drops(ui, center, drops, scene);
            paint_storm_fx(ui, center, t);
        }
    }

    // 3) Glass highlight (sunny only) + rim
    {
        let p = ui.painter();
        if scene.show_highlight() {
            p.circle_filled(
                Pos2::new(center.x - BALL_R * 0.28, center.y - BALL_R * 0.38),
                14.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, 16),
            );
            p.circle_filled(
                Pos2::new(center.x - BALL_R * 0.34, center.y - BALL_R * 0.44),
                5.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, 32),
            );
            p.circle_filled(
                Pos2::new(center.x + BALL_R * 0.32, center.y + BALL_R * 0.42),
                8.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, 12),
            );
        }

        p.circle_stroke(
            center,
            BALL_R,
            Stroke::new(1.2_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 40)),
        );
        p.circle_stroke(
            center,
            BALL_R - 1.2,
            Stroke::new(0.8_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 16)),
        );
    }
}

fn paint_water(ui: &mut egui::Ui, center: Pos2, t: f32, scene: Scene) {
    let p = ui.painter();
    let back = water_polygon(center, t / 11.0 * TAU, 0.58);
    let front = water_polygon(center, -t / 8.0 * TAU + 0.8, 0.62);
    if back.len() >= 3 {
        let c = scene.water_a();
        p.add(Shape::convex_polygon(
            back,
            Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 190),
            Stroke::NONE,
        ));
    }
    if front.len() >= 3 {
        let c = scene.water_b();
        p.add(Shape::convex_polygon(
            front,
            Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 210),
            Stroke::NONE,
        ));
    }
}

fn water_polygon(center: Pos2, phase: f32, top_frac: f32) -> Vec<Pos2> {
    let mut pts = Vec::with_capacity(56);
    let y_base = center.y - BALL_R + BALL_R * 2.0 * top_frac;

    for i in 0..=28 {
        let a = i as f32 / 28.0;
        let x = center.x - BALL_R * 0.92 + a * BALL_R * 1.84;
        let wave = (a * TAU * 2.0 + phase * 2.0).sin() * 3.8 + (a * TAU + phase).sin() * 2.2;
        let pt = Pos2::new(x, y_base + wave);
        if inside_ball(center, pt, 1.5) {
            pts.push(pt);
        }
    }

    for i in 0..=24 {
        let a = i as f32 / 24.0;
        let theta = 0.12 * PI + a * (PI - 0.24 * PI);
        pts.push(Pos2::new(
            center.x + BALL_R * 0.96 * theta.cos(),
            center.y + BALL_R * 0.96 * theta.sin(),
        ));
    }
    pts
}

fn paint_sunny(ui: &mut egui::Ui, center: Pos2, t: f32) {
    let p = ui.painter();
    let sun = Pos2::new(center.x, center.y - BALL_R * 0.28);
    let pulse = 0.9 + 0.1 * (t * TAU / 4.5).sin();

    let ray_rot = t / 26.0 * TAU;
    for i in 0..12 {
        let ang = ray_rot + i as f32 * (TAU / 12.0);
        let a = Pos2::new(sun.x + ang.cos() * 14.0, sun.y + ang.sin() * 14.0);
        let b = Pos2::new(sun.x + ang.cos() * 28.0, sun.y + ang.sin() * 28.0);
        if inside_ball(center, a, 4.0) && inside_ball(center, b, 4.0) {
            p.line_segment(
                [a, b],
                Stroke::new(2.2_f32, Color32::from_rgba_unmultiplied(255, 205, 90, 90)),
            );
        }
    }

    p.circle_filled(
        sun,
        22.0 * pulse,
        Color32::from_rgba_unmultiplied(255, 190, 80, 55),
    );
    p.circle_filled(sun, 16.0, Color32::from_rgba_unmultiplied(255, 170, 60, 70));
    p.circle_filled(sun, 14.0, Color32::from_rgb(0xff, 0xab, 0x3d));
    p.circle_filled(sun, 10.0, Color32::from_rgb(0xff, 0xd8, 0x73));
    p.circle_filled(
        Pos2::new(sun.x - 2.0, sun.y - 2.0),
        5.5,
        Color32::from_rgb(0xff, 0xf7, 0xd6),
    );

    for i in 0..5 {
        let phase = t * (0.4 + i as f32 * 0.07) + i as f32;
        let mx = center.x - BALL_R * 0.35 + (i as f32 * 17.0) % (BALL_R * 0.7);
        let my = center.y + BALL_R * 0.05 + phase.sin() * 10.0;
        let m = Pos2::new(mx, my);
        if inside_ball(center, m, 8.0) {
            p.circle_filled(
                m,
                1.8 + (i % 3) as f32 * 0.6,
                Color32::from_rgba_unmultiplied(255, 225, 140, 180),
            );
        }
    }
}

fn load_cloud_textures(ctx: &egui::Context) -> CloudTextures {
    let opts = TextureOptions::LINEAR;
    CloudTextures {
        a: ctx.load_texture(
            "cloud_a",
            load_png_as_white_mask(include_bytes!("../assets/cloud_a.png")),
            opts,
        ),
        b: ctx.load_texture(
            "cloud_b",
            load_png_as_white_mask(include_bytes!("../assets/cloud_b.png")),
            opts,
        ),
        mist: ctx.load_texture(
            "cloud_mist",
            load_png_as_white_mask(include_bytes!("../assets/cloud_mist.png")),
            opts,
        ),
    }
}

/// Keep soft alpha; force RGB white so scene tint colors match Vue CSS.
fn load_png_as_white_mask(bytes: &'static [u8]) -> ColorImage {
    let img = image::load_from_memory(bytes)
        .expect("cloud PNG decode")
        .into_rgba8();
    let (w, h) = (img.width() as usize, img.height() as usize);
    let mut rgba = Vec::with_capacity(w * h * 4);
    for px in img.pixels() {
        let [_, _, _, a] = px.0;
        rgba.extend_from_slice(&[255, 255, 255, a]);
    }
    ColorImage::from_rgba_unmultiplied([w, h], &rgba)
}

fn paint_clouds(
    ui: &mut egui::Ui,
    center: Pos2,
    t: f32,
    clouds: &CloudTextures,
    scene: Scene,
    with_mist: bool,
) {
    let p = ui.painter();
    let clip = Rect::from_center_size(center, Vec2::splat(BALL_R * 2.0 - 4.0));
    let p = p.with_clip_rect(clip);
    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));

    if with_mist || matches!(scene.orb_kind(), OrbKind::Overcast | OrbKind::Rain | OrbKind::Storm)
    {
        let mist_alpha = match scene.orb_kind() {
            OrbKind::Overcast => 140,
            OrbKind::Rain => 100,
            OrbKind::Storm => 90,
            _ => 80,
        };
        // Vue mist bands
        for (ry, period, amp, w_scale) in [
            (-0.20, 8.0, 3.0, 1.75),
            (-0.05, 11.0, 2.2, 1.55),
        ] {
            let drift = (t / period * TAU).sin() * amp;
            let mist_rect = Rect::from_center_size(
                Pos2::new(center.x + drift, center.y + ry * BALL_R),
                Vec2::new(BALL_R * w_scale, BALL_R * 0.42),
            );
            p.image(
                clouds.mist.id(),
                mist_rect,
                uv,
                scene.cloud_tint(mist_alpha),
            );
        }
    }

    // c1 / c2 / (c3 for cloudy & overcast)
    let sprites: &[(usize, f32, f32, f32, f32, f32, f32, u8)] = match scene.orb_kind() {
        OrbKind::Cloudy | OrbKind::Overcast => &[
            (0, -0.28, -0.48, 58.0, 34.0, 7.0, 2.4, 230),
            (1, 0.18, -0.28, 48.0, 22.0, 9.0, 2.8, 220),
            (0, -0.16, -0.02, 40.0, 24.0, 11.0, 1.8, 200),
        ],
        OrbKind::Rain | OrbKind::Snow | OrbKind::Storm => &[
            (0, -0.22, -0.55, 60.0, 34.0, 7.0, 2.0, 230),
            (1, 0.20, -0.40, 52.0, 22.0, 9.0, 2.6, 220),
        ],
        OrbKind::Sunny => &[],
    };

    for &(tex_i, rx, ry, w, h, period, amp, alpha) in sprites {
        let tex = if tex_i == 0 { &clouds.a } else { &clouds.b };
        let drift = (t / period * TAU).sin() * amp;
        let c = Pos2::new(center.x + rx * BALL_R + drift, center.y + ry * BALL_R);
        if !inside_ball(center, c, 8.0) {
            continue;
        }
        let rect = Rect::from_center_size(c, Vec2::new(w, h));
        p.image(tex.id(), rect, uv, scene.cloud_tint(alpha));
    }
}

fn paint_rain_drops(ui: &mut egui::Ui, center: Pos2, drops: &[Drop], scene: Scene) {
    let p = ui.painter();
    let storm = scene.is_storm();
    let slant = if storm || scene.drop_fast() {
        Vec2::new(0.40, 1.0).normalized()
    } else if matches!(scene, Scene::Drizzle | Scene::Rain(Intensity::Light)) {
        Vec2::new(0.18, 1.0).normalized()
    } else {
        Vec2::new(0.28, 1.0).normalized()
    };

    for d in drops.iter().take(scene.drop_count()) {
        let pos = Pos2::new(
            center.x + d.x * BALL_R * 0.78,
            center.y + d.y * BALL_R * 0.78,
        );
        let end = pos + slant * d.len;
        if inside_ball(center, pos, 7.0) && inside_ball(center, end, 5.0) {
            let col = if storm {
                Color32::from_rgba_unmultiplied(185, 200, 255, d.alpha)
            } else {
                Color32::from_rgba_unmultiplied(200, 225, 255, d.alpha)
            };
            p.line_segment([pos, end], Stroke::new(d.width, col));
        }
    }
}

fn paint_flakes(ui: &mut egui::Ui, center: Pos2, t: f32, flakes: &[Flake], count: usize) {
    let p = ui.painter();
    for f in flakes.iter().take(count) {
        let sway = (t * 1.1 + f.phase).sin() * (f.sway / BALL_R);
        let pos = Pos2::new(
            center.x + (f.x + sway) * BALL_R * 0.78,
            center.y + f.y * BALL_R * 0.78,
        );
        if inside_ball(center, pos, 8.0) {
            p.circle_filled(
                pos,
                f.r,
                Color32::from_rgba_unmultiplied(240, 248, 255, f.alpha),
            );
            p.circle_filled(
                pos,
                f.r * 0.45,
                Color32::from_rgba_unmultiplied(255, 255, 255, 220),
            );
        }
    }
}

fn paint_storm_fx(ui: &mut egui::Ui, center: Pos2, t: f32) {
    let p = ui.painter();
    // Periodic flash + bolt (Vue .flash / .bolt)
    let cycle = (t * 0.55).fract();
    let flash_on = cycle < 0.08 || (0.14..0.18).contains(&cycle);
    if flash_on {
        let a = if cycle < 0.08 { 55 } else { 35 };
        p.circle_filled(
            center,
            BALL_R * 0.92,
            Color32::from_rgba_unmultiplied(200, 190, 255, a),
        );
    }

    let bolt_on = (0.02..0.11).contains(&cycle) || (0.15..0.19).contains(&cycle);
    if bolt_on {
        let ox = center.x + 6.0;
        let oy = center.y - BALL_R * 0.35;
        let pts = [
            Pos2::new(ox, oy),
            Pos2::new(ox + 6.0, oy + 14.0),
            Pos2::new(ox - 2.0, oy + 14.0),
            Pos2::new(ox + 8.0, oy + 32.0),
            Pos2::new(ox - 1.0, oy + 22.0),
            Pos2::new(ox + 4.0, oy + 22.0),
        ];
        // Zigzag as connected segments approximating a bolt
        let segs = [
            (pts[0], pts[1]),
            (pts[1], pts[2]),
            (pts[2], pts[3]),
        ];
        for (a, b) in segs {
            if inside_ball(center, a, 4.0) && inside_ball(center, b, 4.0) {
                p.line_segment(
                    [a, b],
                    Stroke::new(2.4_f32, Color32::from_rgba_unmultiplied(230, 220, 255, 230)),
                );
                p.line_segment(
                    [a, b],
                    Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 255)),
                );
            }
        }
        let _ = (pts[4], pts[5]);
    }
}

fn build_tray_menu(
    toggle_text: &str,
    autostart: bool,
) -> Result<tray_icon::menu::Menu, Box<dyn std::error::Error>> {
    use tray_icon::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};

    let menu = Menu::new();
    let toggle = MenuItem::with_id("toggle", toggle_text, true, None);
    let auto = CheckMenuItem::with_id("autostart", "开机自启", true, autostart, None);
    let quit_item = MenuItem::with_id("quit", "退出", true, None);
    menu.append(&toggle)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&auto)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&quit_item)?;
    Ok(menu)
}

#[cfg(target_os = "windows")]
fn win_set_popup_item_checked(hmenu: isize, pos: u32, checked: bool) {
    if hmenu == 0 {
        return;
    }
    const MF_BYPOSITION: u32 = 0x0400;
    const MF_CHECKED: u32 = 0x0008;
    const MF_UNCHECKED: u32 = 0x0000;
    extern "system" {
        fn CheckMenuItem(hmenu: *mut std::ffi::c_void, item: u32, flags: u32) -> u32;
    }
    let flags = MF_BYPOSITION | if checked { MF_CHECKED } else { MF_UNCHECKED };
    unsafe {
        CheckMenuItem(hmenu as *mut std::ffi::c_void, pos, flags);
    }
}

#[cfg(not(target_os = "windows"))]
fn win_set_popup_item_checked(_hmenu: isize, _pos: u32, _checked: bool) {}

fn make_tray_icon() -> Result<tray_icon::Icon, Box<dyn std::error::Error>> {
    use tray_icon::Icon;
    // 32×32 with coverage AA — reads cleanly when Windows scales to 16px.
    const N: u32 = 32;
    let mut rgba = vec![0u8; (N * N * 4) as usize];
    let cx = (N as f32) * 0.5;
    let cy = cx;
    let r = N as f32 * 0.46;
    for y in 0..N {
        for x in 0..N {
            let dx = (x as f32 + 0.5 - cx) / r;
            let dy = (y as f32 + 0.5 - cy) / r;
            let d = (dx * dx + dy * dy).sqrt();
            let alpha = smoothstep(1.06, 0.94, d);
            if alpha <= 0.01 {
                continue;
            }
            let nz = (1.0 - (dx * dx + dy * dy).min(1.0)).sqrt();
            let lit = (0.42 + 0.58 * (-dx * 0.32 - dy * 0.55 + nz * 0.62)).clamp(0.15, 1.0);
            let gy = (dy * 0.5 + 0.5).clamp(0.0, 1.0);
            let mut cr = lerp(0x48 as f32, 0x1e as f32, gy) * lit;
            let mut cg = lerp(0xce as f32, 0x8a as f32, gy) * lit;
            let mut cb = lerp(0xc4 as f32, 0x86 as f32, gy) * lit;

            let sdx = dx - 0.16;
            let sdy = dy + 0.24;
            let sun = smoothstep(0.40, 0.18, (sdx * sdx + sdy * sdy).sqrt());
            cr = lerp(cr, 255.0, sun * 0.92);
            cg = lerp(cg, 188.0, sun * 0.90);
            cb = lerp(cb, 72.0, sun * 0.82);

            let hdx = dx + 0.34;
            let hdy = dy + 0.42;
            let spec = smoothstep(0.30, 0.02, (hdx * hdx + hdy * hdy).sqrt());
            cr = (cr + 255.0 * spec * 0.62).min(255.0);
            cg = (cg + 255.0 * spec * 0.62).min(255.0);
            cb = (cb + 255.0 * spec * 0.62).min(255.0);

            let rim = smoothstep(0.80, 1.02, d) * 0.45;
            cr = (cr + 210.0 * rim).min(255.0);
            cg = (cg + 230.0 * rim).min(255.0);
            cb = (cb + 235.0 * rim).min(255.0);

            let i = ((y * N + x) * 4) as usize;
            rgba[i] = cr.round() as u8;
            rgba[i + 1] = cg.round() as u8;
            rgba[i + 2] = cb.round() as u8;
            rgba[i + 3] = (alpha * 255.0).round() as u8;
        }
    }
    Ok(Icon::from_rgba(rgba, N, N)?)
}

fn create_tray(
    window_visible: Arc<AtomicBool>,
    main_hwnd: Arc<AtomicIsize>,
    open_at_login: Arc<AtomicBool>,
    ctx: egui::Context,
) -> Result<TrayUi, Box<dyn std::error::Error>> {
    use tray_icon::TrayIconBuilder;

    let icon = make_tray_icon()?;

    let autostart = open_at_login.load(Ordering::Relaxed);
    let menu = build_tray_menu(toggle_label(true), autostart)?;
    let tray_hmenu = {
        use tray_icon::menu::ContextMenu;
        Arc::new(AtomicIsize::new(menu.hpopupmenu()))
    };

    let tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("天气球")
        .with_title("天气球")
        .with_icon(icon)
        .build()?;

    spawn_tray_poller(
        window_visible,
        main_hwnd,
        open_at_login,
        tray_hmenu,
        ctx,
    );

    Ok(TrayUi { _icon: tray_icon })
}

/// Apply WS_EX_TOOLWINDOW to our hwnd only. EnumWindows can freeze the GUI
/// thread if any other top-level window's thread is hung.
#[cfg(target_os = "windows")]
fn hide_hwnd_from_taskbar(hwnd_val: isize) -> bool {
    use std::ffi::c_void;

    if hwnd_val == 0 {
        return false;
    }

    type Hwnd = *mut c_void;
    const GWL_EXSTYLE: i32 = -20;
    const WS_EX_APPWINDOW: isize = 0x0004_0000;
    const WS_EX_TOOLWINDOW: isize = 0x0000_0080;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_FRAMECHANGED: u32 = 0x0020;

    extern "system" {
        fn IsWindow(h_wnd: Hwnd) -> i32;
        fn GetWindowLongPtrW(h_wnd: Hwnd, n_index: i32) -> isize;
        fn SetWindowLongPtrW(h_wnd: Hwnd, n_index: i32, dw_new_long: isize) -> isize;
        fn SetWindowPos(
            h_wnd: Hwnd,
            insert_after: Hwnd,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> i32;
    }

    let hwnd = hwnd_val as Hwnd;
    unsafe {
        if IsWindow(hwnd) == 0 {
            return false;
        }
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let next = (style | WS_EX_TOOLWINDOW) & !WS_EX_APPWINDOW;
        if next != style {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next);
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
        true
    }
}

#[cfg(not(target_os = "windows"))]
fn hide_hwnd_from_taskbar(_hwnd: isize) -> bool {
    true
}

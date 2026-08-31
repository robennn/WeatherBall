//! Minimal transparent orb — Rust + egui, no WebView.
//! Scenes match Vue WeatherBall; live data from Open-Meteo.

// Release: GUI-only process — no extra console window beside the orb.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

// Hybrid laptops: do not request the NVIDIA dGPU. DWM composites on the iGPU;
// OpenGL on NVIDIA drops per-pixel alpha and shows an opaque gray box.
#[cfg(target_os = "windows")]
#[used]
#[no_mangle]
#[allow(non_upper_case_globals)]
pub static NvOptimusEnablement: u32 = 0;

#[cfg(target_os = "windows")]
#[used]
#[no_mangle]
#[allow(non_upper_case_globals)]
pub static AmdPowerXpressRequestHighPerformance: i32 = 0;

mod cities;
mod display;
mod settings;
mod weather;

use eframe::egui::{
    self, Color32, ColorImage, PointerButton, Pos2, Rect, Sense, Shape, Stroke, TextureHandle,
    TextureOptions, Vec2,
};
use std::f32::consts::{PI, TAU};
use std::io::Read;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};
use weather::{local_is_day_guess, HourlyPoint, LiveWeather, PrecipSoon, PrecipSoonKind, WeatherState};

const W: f32 = 160.0;
const H_DETAIL: f32 = 520.0;
const ORB_R: f32 = 50.0;
/// Tenths of a logical pixel so paint helpers can read the live radius.
static PAINT_ORB_R_TENTHS: AtomicU32 = AtomicU32::new(500);

pub(crate) fn ball_r() -> f32 {
    PAINT_ORB_R_TENTHS.load(Ordering::Relaxed) as f32 * 0.1
}

fn ball_s() -> f32 {
    ball_r() / ORB_R
}

fn set_paint_ball_r(r: f32) {
    let tenths = (r * 10.0).round().clamp(1.0, 900.0) as u32;
    PAINT_ORB_R_TENTHS.store(tenths, Ordering::Relaxed);
}

/// Ball center from window top — same in compact & detail (Vue: stays put while height grows down).
const BALL_CENTER_Y: f32 = 140.0;
const REFRESH_SECS: u64 = 5 * 60;
const DRAG_THRESHOLD: f32 = 5.0;
/// Match Vue panel: height ~280ms, opacity a bit quicker.
const DETAIL_ANIM_SECS: f32 = 0.28;
/// Vue `FX_MS`: fade particles/sun out, swap, fade in.
const SCENE_FADE_SECS: f32 = 0.30;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PanelKind {
    Detail,
    Settings,
    Skins,
}

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

    fn glow(self, is_day: bool) -> Color32 {
        if !is_day {
            return match self.orb_kind() {
                OrbKind::Sunny => Color32::from_rgba_unmultiplied(170, 195, 255, 0),
                OrbKind::Cloudy => Color32::from_rgba_unmultiplied(110, 145, 195, 72),
                OrbKind::Overcast => Color32::from_rgba_unmultiplied(90, 110, 145, 55),
                OrbKind::Rain => match self {
                    Scene::Drizzle => Color32::from_rgba_unmultiplied(80, 130, 210, 70),
                    _ => Color32::from_rgba_unmultiplied(80, 130, 210, 88),
                },
                OrbKind::Snow => Color32::from_rgba_unmultiplied(150, 180, 230, 92),
                OrbKind::Storm => Color32::from_rgba_unmultiplied(130, 110, 220, 98),
            };
        }
        match self.orb_kind() {
            OrbKind::Sunny => Color32::from_rgba_unmultiplied(255, 190, 90, 115),
            OrbKind::Cloudy => Color32::from_rgba_unmultiplied(160, 195, 240, 89),
            OrbKind::Overcast => Color32::from_rgba_unmultiplied(190, 200, 215, 71),
            OrbKind::Rain => match self {
                Scene::Drizzle => Color32::from_rgba_unmultiplied(90, 150, 230, 82),
                _ => Color32::from_rgba_unmultiplied(90, 150, 230, 102),
            },
            OrbKind::Snow => Color32::from_rgba_unmultiplied(130, 175, 220, 100),
            OrbKind::Storm => Color32::from_rgba_unmultiplied(150, 120, 240, 107),
        }
    }

    fn water_a(self, is_day: bool) -> Color32 {
        if !is_day {
            return match self.orb_kind() {
                OrbKind::Sunny => Color32::from_rgb(0x1a, 0x3a, 0x5c),
                OrbKind::Cloudy => Color32::from_rgb(0x1c, 0x35, 0x48),
                OrbKind::Overcast => Color32::from_rgb(0x1a, 0x2c, 0x38),
                OrbKind::Rain => Color32::from_rgb(0x12, 0x24, 0x3e),
                OrbKind::Snow => Color32::from_rgb(0x2a, 0x48, 0x60),
                OrbKind::Storm => Color32::from_rgb(0x12, 0x10, 0x2a),
            };
        }
        match self.orb_kind() {
            OrbKind::Sunny => Color32::from_rgb(0x2a, 0xa8, 0xa2),
            OrbKind::Cloudy => Color32::from_rgb(0x3d, 0x7b, 0xa3),
            OrbKind::Overcast => Color32::from_rgb(0x4c, 0x6a, 0x80),
            OrbKind::Rain => Color32::from_rgb(0x2c, 0x5d, 0x92),
            OrbKind::Snow => Color32::from_rgb(0x4e, 0x8a, 0xb0),
            OrbKind::Storm => Color32::from_rgb(0x2e, 0x35, 0x60),
        }
    }

    fn water_b(self, is_day: bool) -> Color32 {
        if !is_day {
            return match self.orb_kind() {
                OrbKind::Sunny => Color32::from_rgb(0x24, 0x50, 0x7a),
                OrbKind::Cloudy => Color32::from_rgb(0x2a, 0x4a, 0x62),
                OrbKind::Overcast => Color32::from_rgb(0x24, 0x38, 0x48),
                OrbKind::Rain => Color32::from_rgb(0x1c, 0x38, 0x60),
                OrbKind::Snow => Color32::from_rgb(0x3a, 0x60, 0x80),
                OrbKind::Storm => Color32::from_rgb(0x22, 0x1e, 0x48),
            };
        }
        match self.orb_kind() {
            OrbKind::Sunny => Color32::from_rgb(0x3e, 0xc6, 0xc0),
            OrbKind::Cloudy => Color32::from_rgb(0x5a, 0x9c, 0xc4),
            OrbKind::Overcast => Color32::from_rgb(0x68, 0x85, 0x9a),
            OrbKind::Rain => Color32::from_rgb(0x3f, 0x78, 0xb4),
            OrbKind::Snow => Color32::from_rgb(0x68, 0xa8, 0xcc),
            OrbKind::Storm => Color32::from_rgb(0x41, 0x4a, 0x82),
        }
    }

    /// Opaque sky disc behind water so light wallpapers cannot punch through the orb.
    fn sky(self, is_day: bool) -> Color32 {
        if !is_day {
            return match self.orb_kind() {
                OrbKind::Sunny => Color32::from_rgb(0x0e, 0x14, 0x26),
                OrbKind::Cloudy => Color32::from_rgb(0x16, 0x20, 0x2c),
                OrbKind::Overcast => Color32::from_rgb(0x14, 0x18, 0x22),
                OrbKind::Rain => Color32::from_rgb(0x10, 0x18, 0x28),
                OrbKind::Snow => Color32::from_rgb(0x18, 0x26, 0x36),
                OrbKind::Storm => Color32::from_rgb(0x10, 0x0e, 0x20),
            };
        }
        match self.orb_kind() {
            OrbKind::Sunny => Color32::from_rgb(0x6e, 0xb8, 0xdc),
            OrbKind::Cloudy => Color32::from_rgb(0x6a, 0x96, 0xb4),
            OrbKind::Overcast => Color32::from_rgb(0x62, 0x76, 0x8a),
            OrbKind::Rain => Color32::from_rgb(0x48, 0x68, 0x8c),
            OrbKind::Snow => Color32::from_rgb(0x52, 0x7e, 0x9e),
            OrbKind::Storm => Color32::from_rgb(0x3a, 0x42, 0x66),
        }
    }

    fn cloud_tint(self, is_day: bool, alpha: u8) -> Color32 {
        let a = if is_day {
            alpha
        } else {
            ((alpha as u16 * 175) / 255) as u8
        };
        if !is_day {
            return match self.orb_kind() {
                OrbKind::Sunny | OrbKind::Cloudy => {
                    Color32::from_rgba_unmultiplied(0xc5, 0xd0, 0xe8, a)
                }
                OrbKind::Overcast => Color32::from_rgba_unmultiplied(0x7a, 0x88, 0x98, a),
                OrbKind::Rain => Color32::from_rgba_unmultiplied(0x6a, 0x7a, 0x90, a),
                OrbKind::Snow => Color32::from_rgba_unmultiplied(0xb8, 0xc8, 0xd8, a),
                OrbKind::Storm => Color32::from_rgba_unmultiplied(0x4a, 0x4e, 0x62, a),
            };
        }
        match self.orb_kind() {
            OrbKind::Sunny | OrbKind::Cloudy => {
                Color32::from_rgba_unmultiplied(0xf4, 0xf8, 0xfd, a)
            }
            OrbKind::Overcast => Color32::from_rgba_unmultiplied(0xae, 0xb8, 0xc4, a),
            OrbKind::Rain => Color32::from_rgba_unmultiplied(0x8f, 0xa0, 0xb4, a),
            OrbKind::Snow => Color32::from_rgba_unmultiplied(0xdd, 0xe6, 0xef, a),
            OrbKind::Storm => Color32::from_rgba_unmultiplied(0x5b, 0x5f, 0x78, a),
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

/// Click-to-try orb scenes (mirrors Vue `PREVIEW_ITEMS`). Right-click restores live weather.
#[derive(Clone, Copy)]
struct PreviewScene {
    scene: Scene,
    label: &'static str,
    rain_soon: Option<PrecipSoon>,
}

const PREVIEW_SCENES: [PreviewScene; 14] = [
    PreviewScene { scene: Scene::Sunny, label: "晴", rain_soon: None },
    PreviewScene { scene: Scene::Cloudy, label: "多云", rain_soon: None },
    PreviewScene { scene: Scene::Overcast, label: "阴", rain_soon: None },
    PreviewScene { scene: Scene::Drizzle, label: "小毛毛雨", rain_soon: None },
    PreviewScene { scene: Scene::Rain(Intensity::Light), label: "小雨", rain_soon: None },
    PreviewScene { scene: Scene::Rain(Intensity::Moderate), label: "中雨", rain_soon: None },
    PreviewScene { scene: Scene::Rain(Intensity::Heavy), label: "大雨", rain_soon: None },
    PreviewScene { scene: Scene::Snow(Intensity::Light), label: "小雪", rain_soon: None },
    PreviewScene { scene: Scene::Snow(Intensity::Heavy), label: "大雪", rain_soon: None },
    PreviewScene { scene: Scene::Storm(Intensity::Moderate), label: "雷阵雨", rain_soon: None },
    PreviewScene { scene: Scene::Storm(Intensity::Heavy), label: "强雷暴", rain_soon: None },
    PreviewScene {
        scene: Scene::Sunny,
        label: "雨预警",
        rain_soon: Some(PrecipSoon { minutes: 60, kind: PrecipSoonKind::Rain }),
    },
    PreviewScene {
        scene: Scene::Sunny,
        label: "雷雨预警",
        rain_soon: Some(PrecipSoon { minutes: 60, kind: PrecipSoonKind::Storm }),
    },
    PreviewScene {
        scene: Scene::Cloudy,
        label: "雪预警",
        rain_soon: Some(PrecipSoon { minutes: 60, kind: PrecipSoonKind::Snow }),
    },
];

/// Click-to-try water tint. Right-click restores live temperature.
const PREVIEW_TEMPS: [f32; 5] = [0.0, 8.0, 18.0, 28.0, 36.0];

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
    api_is_day: bool,
    is_day: bool,
    day_night: settings::DayNightPref,
    drops: Vec<Drop>,
    flakes: Vec<Flake>,
    cloud_tex: CloudTextures,
    silk_tex: TextureHandle,
    changli_a_tex: TextureHandle,
    changli_b_tex: TextureHandle,
    cartethyia_a_tex: TextureHandle,
    cartethyia_b_tex: TextureHandle,
    weather: Arc<Mutex<WeatherState>>,
    last_fetch: Instant,
    applied_scene: Option<Scene>,
    /// None = live weather; otherwise index into `PREVIEW_SCENES`.
    preview_index: Option<usize>,
    /// None = live temperature; otherwise index into `PREVIEW_TEMPS`.
    preview_temp_index: Option<usize>,
    /// 1 = weather FX fully visible; 0 = swap point (Vue fade-out then fade-in).
    fx_alpha: f32,
    fx_out: bool,
    fx_pending: Option<(Scene, bool)>,
    /// Vue keeps clouds up while only rain/snow/storm particles crossfade.
    fx_keep_clouds: bool,
    detail_open: bool,
    settings_open: bool,
    /// Which panel to show while expanding/collapsing.
    panel_kind: PanelKind,
    /// 0 = compact, 1 = fully expanded (animated).
    detail_t: f32,
    /// Visual 0..=1 for the autostart pill knob.
    autostart_t: f32,
    debug_mode: bool,
    debug_t: f32,
    lock_position: bool,
    lock_t: f32,
    fix_gray_box: bool,
    fix_gray_box_t: f32,
    ball_scale: f32,
    ball_opacity: f32,
    panel_skin: String,
    settings_scroll: f32,
    /// When the settings header skin icon started being hovered (for tip delay).
    skin_tip_hover_since: Option<Instant>,
    press_pos: Option<Pos2>,
    dragging: bool,
    /// Cursor minus ball-center in physical pixels, while dragging.
    drag_grab: Option<(i32, i32)>,
    suppress_click: bool,
    /// CJK font bytes loaded off-thread: (bytes, ttc_index).
    pending_font: Arc<Mutex<Option<(Vec<u8>, u32)>>>,
    font_job_done: Arc<AtomicBool>,
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
    /// True while the NVIDIA DWM layered-alpha hwnd styles are currently on.
    nvidia_hwnd_fix_on: bool,
    fullscreen_occluded: Arc<AtomicBool>,
    tray: Option<TrayUi>,
    city_picker: CityPicker,
    is_manual_city: bool,
}

struct TrayUi {
    /// On Windows the `TrayIcon` lives on a dedicated thread so `TrackPopupMenu`
    /// cannot stall orb drawing. Holding it here would drop it from the GUI thread.
    _icon: Option<tray_icon::TrayIcon>,
}

fn lock_mutex<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

static FETCH_GEN: AtomicU64 = AtomicU64::new(0);

enum FetchJob {
    Auto,
    ForceLocate,
    At {
        lat: f64,
        lon: f64,
        city: String,
    },
}

fn spawn_weather_fetch(weather: Arc<Mutex<WeatherState>>) {
    spawn_weather_job(weather, FetchJob::Auto);
}

fn spawn_weather_job(weather: Arc<Mutex<WeatherState>>, job: FetchJob) {
    let gen = FETCH_GEN.fetch_add(1, Ordering::Relaxed) + 1;
    {
        let mut w = lock_mutex(&weather);
        w.loading = true;
        w.error = None;
    }
    thread::spawn(move || {
        let result = match job {
            FetchJob::Auto => weather::refresh_blocking(),
            FetchJob::ForceLocate => weather::refresh_force_locate(),
            FetchJob::At { lat, lon, city } => weather::refresh_at_coords(lat, lon, city),
        };
        if FETCH_GEN.load(Ordering::Relaxed) != gen {
            return;
        }
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

struct CityPicker {
    open: bool,
    query: String,
    issued: String,
    last_edit: Instant,
    results: Vec<cities::CityOption>,
    searching: bool,
    gen: u64,
    inbox: Arc<Mutex<Option<(u64, Vec<cities::CityOption>)>>>,
    focus_search: bool,
}

impl CityPicker {
    fn new() -> Self {
        Self {
            open: false,
            query: String::new(),
            issued: String::new(),
            last_edit: Instant::now(),
            results: cities::common_cities(),
            searching: false,
            gen: 0,
            inbox: Arc::new(Mutex::new(None)),
            focus_search: false,
        }
    }

    fn open_now(&mut self) {
        self.open = true;
        self.query.clear();
        self.issued.clear();
        self.searching = false;
        self.results = cities::common_cities();
        self.gen = self.gen.wrapping_add(1);
        self.focus_search = true;
        *lock_mutex(&self.inbox) = None;
    }

    fn close(&mut self) {
        self.open = false;
        self.query.clear();
        self.issued.clear();
        self.searching = false;
        self.results = cities::common_cities();
        self.gen = self.gen.wrapping_add(1);
        self.focus_search = false;
        *lock_mutex(&self.inbox) = None;
    }

    fn poll(&mut self, ctx: &egui::Context) {
        if let Some((gen, list)) = lock_mutex(&self.inbox).take() {
            if gen == self.gen {
                self.results = list;
                self.searching = false;
            }
        }
        if !self.open {
            return;
        }
        let q = self.query.trim();
        if q.is_empty() {
            return;
        }
        if q == self.issued {
            return;
        }
        if self.last_edit.elapsed() < Duration::from_millis(280) {
            return;
        }
        self.issued = q.to_string();
        self.searching = true;
        self.gen = self.gen.wrapping_add(1);
        let gen = self.gen;
        let query = self.issued.clone();
        let inbox = Arc::clone(&self.inbox);
        let ctx = ctx.clone();
        thread::spawn(move || {
            let list = cities::search_cities(&query);
            *lock_mutex(&inbox) = Some((gen, list));
            ctx.request_repaint();
        });
    }
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
        .with_close_button(false)
        .with_minimize_button(false)
        .with_maximize_button(false)
        .with_mouse_passthrough(false)
        .with_visible(true)
        // Never use an empty title: some NVIDIA DWM paths substitute the
        // caption-font subfamily "Normal". NBSP is invisible if chrome leaks.
        .with_title(SILENT_WINDOW_TITLE);
    if let Some((x, y)) = saved_pos {
        let [lx, ly] = display::physical_to_logical_pos(x, y);
        viewport = viewport.with_position([lx, ly]);
    }

    let native_options = eframe::NativeOptions {
        viewport,
        centered: false,
        persist_window: false,
        renderer: eframe::Renderer::Glow,
        // MSAA + per-pixel alpha is broken on some GPUs (opaque gray box around the orb).
        multisampling: 0,
        depth_buffer: 0,
        stencil_buffer: 0,
        ..Default::default()
    };

    let open_at_login_ui = Arc::clone(&open_at_login);
    eframe::run_native(
        "WeatherBall",
        native_options,
        Box::new(move |cc| {
            // Dark widgets, but never fill the whole hwnd — leftover panel_fill shows as a
            // gray rectangle around the orb when DWM transparency fails.
            cc.egui_ctx.set_theme(egui::ThemePreference::Dark);
            let mut visuals = egui::Visuals::dark();
            visuals.panel_fill = Color32::TRANSPARENT;
            visuals.window_fill = Color32::TRANSPARENT;
            visuals.extreme_bg_color = Color32::TRANSPARENT;
            visuals.faint_bg_color = Color32::TRANSPARENT;
            cc.egui_ctx.set_visuals(visuals);
            let scene = Scene::Sunny;
            let day_night = loaded.day_night;
            let api_is_day = local_is_day_guess();
            let is_day = effective_is_day(day_night, api_is_day);
            let weather = Arc::new(Mutex::new(WeatherState::default()));
            spawn_weather_fetch(Arc::clone(&weather));

            let pending_font = Arc::new(Mutex::new(None));
            let font_job_done = Arc::new(AtomicBool::new(false));
            let pending_font_bg = Arc::clone(&pending_font);
            let font_job_done_bg = Arc::clone(&font_job_done);
            thread::spawn(move || {
                let found = ensure_cjk_font();
                *lock_mutex(&pending_font_bg) = found;
                font_job_done_bg.store(true, Ordering::Relaxed);
            });

            let window_visible = Arc::new(AtomicBool::new(true));
            let main_hwnd = Arc::new(AtomicIsize::new(0));
            let fullscreen_occluded = Arc::new(AtomicBool::new(false));
            spawn_fullscreen_poller(
                Arc::clone(&window_visible),
                Arc::clone(&main_hwnd),
                Arc::clone(&fullscreen_occluded),
                cc.egui_ctx.clone(),
            );

            Ok(Box::new(OrbApp {
                quit,
                started: Instant::now(),
                scene,
                api_is_day,
                is_day,
                day_night,
                drops: make_drops(scene),
                flakes: make_flakes(scene),
                cloud_tex: load_cloud_textures(&cc.egui_ctx),
                silk_tex: load_skin_texture(&cc.egui_ctx, "skin_silk", include_bytes!("../assets/skin_silk.png")),
                changli_a_tex: load_skin_texture(&cc.egui_ctx, "skin_changli_a", include_bytes!("../assets/skin_changli.png")),
                changli_b_tex: load_skin_texture(&cc.egui_ctx, "skin_changli_b", include_bytes!("../assets/skin_changli_b.png")),
                cartethyia_a_tex: load_skin_texture(&cc.egui_ctx, "skin_cartethyia_a", include_bytes!("../assets/skin_cartethyia_a.png")),
                cartethyia_b_tex: load_skin_texture(&cc.egui_ctx, "skin_cartethyia_b", include_bytes!("../assets/skin_cartethyia_b.png")),
                weather,
                last_fetch: Instant::now(),
                applied_scene: None,
                preview_index: None,
                preview_temp_index: None,
                fx_alpha: 1.0,
                fx_out: false,
                fx_pending: None,
                fx_keep_clouds: false,
                detail_open: false,
                settings_open: false,
                panel_kind: PanelKind::Detail,
                detail_t: 0.0,
                autostart_t: if loaded.open_at_login { 1.0 } else { 0.0 },
                debug_mode: loaded.debug_mode,
                debug_t: if loaded.debug_mode { 1.0 } else { 0.0 },
                lock_position: loaded.lock_position,
                lock_t: if loaded.lock_position { 1.0 } else { 0.0 },
                fix_gray_box: loaded.fix_gray_box,
                fix_gray_box_t: if loaded.fix_gray_box { 1.0 } else { 0.0 },
                ball_scale: settings::clamp_ball_scale(loaded.ball_scale),
                ball_opacity: settings::clamp_ball_opacity(loaded.ball_opacity),
                panel_skin: {
                    let id = loaded.panel_skin.trim();
                    if id.is_empty() || id == "flower" {
                        if id == "flower" {
                            settings::save_panel_skin("default");
                        }
                        "default".into()
                    } else if id == "changli" {
                        settings::save_panel_skin("changli_a");
                        "changli_a".into()
                    } else {
                        loaded.panel_skin
                    }
                },
                settings_scroll: 0.0,
                skin_tip_hover_since: None,
                press_pos: None,
                dragging: false,
                drag_grab: None,
                suppress_click: false,
                pending_font,
                font_job_done,
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
                nvidia_hwnd_fix_on: false,
                fullscreen_occluded,
                tray: None,
                city_picker: CityPicker::new(),
                is_manual_city: settings::load_manual_city().is_some(),
            }))
        }),
    )
}

/// Invisible hwnd title. Empty titles make some DWM paths paint "Normal".
const SILENT_WINDOW_TITLE: &str = "\u{00A0}";

/// Reject faces that cannot actually draw CJK. A broken TTC face often maps
/// every character to one glyph and rasterizes the style name "Normal".
fn face_has_distinct_cjk(bytes: &[u8], index: u32) -> bool {
    use ab_glyph::{Font, FontRef, ScaleFont};
    let Ok(font) = FontRef::try_from_slice_and_index(bytes, index) else {
        return false;
    };
    let cjk = ['晴', '天', '气', '设', '湿'];
    let mut ids = [ab_glyph::GlyphId(0); 5];
    for (i, ch) in cjk.iter().copied().enumerate() {
        let id = font.glyph_id(ch);
        if id.0 == 0 {
            return false;
        }
        ids[i] = id;
    }
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            if ids[i] == ids[j] {
                return false;
            }
        }
    }
    for ch in ['N', 'o', 'r', 'm', 'a', 'l'] {
        let latin = font.glyph_id(ch);
        if ids.iter().any(|&id| id == latin) {
            return false;
        }
    }
    let scaled = font.as_scaled(32.0);
    for ch in cjk {
        let gid = font.glyph_id(ch);
        let adv = scaled.h_advance(gid);
        // A face that bakes the word "Normal" into the CJK slot is far wider
        // than a square ideograph.
        if !(8.0..=48.0).contains(&adv) {
            return false;
        }
        let Some(outlined) = font.outline_glyph(gid.with_scale(32.0)) else {
            return false;
        };
        let bounds = outlined.px_bounds();
        let w = bounds.width();
        let h = bounds.height();
        if w < 3.0 || h < 3.0 {
            return false;
        }
        let aspect = w / h.max(0.1);
        if aspect < 0.4 || aspect > 1.7 {
            return false;
        }
    }
    true
}

const CJK_CACHE_NAME: &str = "NotoSansSC-Regular.otf";

fn cached_cjk_font_path() -> Option<PathBuf> {
    Some(settings::fonts_dir()?.join(CJK_CACHE_NAME))
}

fn sidecar_cjk_font_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            out.push(dir.join(CJK_CACHE_NAME));
            out.push(dir.join("fonts").join(CJK_CACHE_NAME));
            out.push(dir.join("NotoSansSC-Regular.ttf"));
        }
    }
    out
}

fn try_font_file(path: &std::path::Path) -> Option<(Vec<u8>, u32)> {
    let bytes = std::fs::read(path).ok()?;
    for index in 0..16u32 {
        if face_has_distinct_cjk(&bytes, index) {
            if index == 0 {
                eprintln!(
                    "[weatherball-native] loaded font {} ({} KB)",
                    path.display(),
                    bytes.len() / 1024
                );
            } else {
                eprintln!(
                    "[weatherball-native] loaded font {}#{} ({} KB)",
                    path.display(),
                    index,
                    bytes.len() / 1024
                );
            }
            return Some((bytes, index));
        }
    }
    None
}

fn load_system_cjk_ttf() -> Option<(Vec<u8>, u32)> {
    let ttf = [
        r"C:\Windows\Fonts\simhei.ttf",
        r"C:\Windows\Fonts\DengXian.ttf",
        r"C:\Windows\Fonts\Deng.ttf",
        r"C:\Windows\Fonts\simkai.ttf",
        r"C:\Windows\Fonts\simfang.ttf",
        r"C:\Windows\Fonts\msyh.ttf",
        r"C:\Windows\Fonts\msjh.ttf",
        r"C:\Windows\Fonts\malgun.ttf",
        r"C:\Windows\Fonts\meiryo.ttf",
        r"C:\Windows\Fonts\msgothic.ttf",
        r"C:\Windows\Fonts\YuGothR.ttf",
    ];
    for path in ttf {
        if let Some(font) = try_font_file(std::path::Path::new(path)) {
            return Some(font);
        }
    }
    None
}

fn load_system_cjk_ttc() -> Option<(Vec<u8>, u32)> {
    let ttc = [
        r"C:\Windows\Fonts\msyh.ttc",
        r"C:\Windows\Fonts\msjh.ttc",
        r"C:\Windows\Fonts\simsun.ttc",
        r"C:\Windows\Fonts\YuGothR.ttc",
        r"C:\Windows\Fonts\malgun.ttc",
    ];
    for path in ttc {
        if let Some(font) = try_font_file(std::path::Path::new(path)) {
            return Some(font);
        }
    }
    None
}

fn download_cjk_font() -> Option<Vec<u8>> {
    const URLS: &[&str] = &[
        "https://cdn.jsdelivr.net/gh/notofonts/noto-cjk@main/Sans/SubsetOTF/SC/NotoSansSC-Regular.otf",
        "https://github.com/notofonts/noto-cjk/raw/main/Sans/SubsetOTF/SC/NotoSansSC-Regular.otf",
    ];
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(120))
        .build();
    for url in URLS {
        eprintln!("[weatherball-native] downloading CJK font from {url}");
        let Ok(resp) = agent
            .get(url)
            .set("User-Agent", "WeatherBall/0.3")
            .call()
        else {
            continue;
        };
        let mut buf = Vec::new();
        let read = resp
            .into_reader()
            .take(24 * 1024 * 1024)
            .read_to_end(&mut buf);
        if read.is_ok() && buf.len() > 80_000 && face_has_distinct_cjk(&buf, 0) {
            eprintln!(
                "[weatherball-native] downloaded CJK font ({} KB)",
                buf.len() / 1024
            );
            return Some(buf);
        }
    }
    eprintln!("[weatherball-native] CJK font download failed");
    None
}

fn save_cached_cjk_font(bytes: &[u8]) {
    let Some(path) = cached_cjk_font_path() else {
        return;
    };
    if std::fs::write(&path, bytes).is_ok() {
        eprintln!(
            "[weatherball-native] installed font cache {}",
            path.display()
        );
        install_user_font(&path);
    }
}

/// Register the font for this user so it is available on later launches.
fn install_user_font(path: &std::path::Path) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        const FR_PRIVATE: u32 = 0x10;
        extern "system" {
            fn AddFontResourceExW(
                name: *const u16,
                flags: u32,
                reserved: *mut std::ffi::c_void,
            ) -> i32;
        }
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        unsafe {
            AddFontResourceExW(wide.as_ptr(), FR_PRIVATE, std::ptr::null_mut());
        }
        if let (Some(local), Some(name)) = (std::env::var_os("LOCALAPPDATA"), path.file_name()) {
            let dest = PathBuf::from(local)
                .join("Microsoft")
                .join("Windows")
                .join("Fonts")
                .join(name);
            if dest != path {
                if let Some(parent) = dest.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let _ = std::fs::copy(path, dest);
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
    }
}

/// System TTF first, then a cached/downloaded Noto SC face. TTC is last because
/// a bad collection face can rasterize the style name "Normal".
fn ensure_cjk_font() -> Option<(Vec<u8>, u32)> {
    if let Some(font) = load_system_cjk_ttf() {
        return Some(font);
    }
    for path in sidecar_cjk_font_paths() {
        if let Some(font) = try_font_file(&path) {
            return Some(font);
        }
    }
    if let Some(path) = cached_cjk_font_path() {
        if let Some(font) = try_font_file(&path) {
            return Some(font);
        }
    }
    if let Some(bytes) = download_cjk_font() {
        save_cached_cjk_font(&bytes);
        return Some((bytes, 0));
    }
    if let Some(font) = load_system_cjk_ttc() {
        return Some(font);
    }
    eprintln!("[weatherball-native] 未找到可用中文字体");
    None
}

fn apply_cjk_font(ctx: &egui::Context, bytes: Vec<u8>, index: u32) {
    let mut fonts = egui::FontDefinitions::default();
    let mut data = egui::FontData::from_owned(bytes);
    data.index = index;
    fonts.font_data.insert("cjk".to_owned(), Arc::new(data));

    // Keep Ubuntu-Light first so Latin is never stolen by a bad CJK face.
    // CJK glyphs fall through to this face; never insert at index 0.
    if let Some(prop) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        prop.retain(|name| name != "cjk");
        if !prop.is_empty() {
            prop.insert(1.min(prop.len()), "cjk".to_owned());
        }
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

        set_paint_ball_r(ORB_R * self.ball_scale);

        ctx.send_viewport_cmd(egui::ViewportCommand::Title(
            SILENT_WINDOW_TITLE.to_owned(),
        ));

        if let Some(hwnd) = hwnd_from_frame(frame) {
            self.main_hwnd.store(hwnd, Ordering::Relaxed);
            if self.dragging {
                #[cfg(target_os = "windows")]
                {
                    if !win_primary_down() {
                        self.dragging = false;
                        self.drag_grab = None;
                        self.suppress_click = true;
                        if !self.lock_position {
                            self.persist_window_pos();
                        }
                    } else if let Some(grab) = self.drag_grab {
                        follow_cursor_drag(hwnd, grab);
                    }
                }
            } else if !self.pos_applied {
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
                    Arc::clone(&self.fullscreen_occluded),
                    ctx.clone(),
                )
                .ok();
            }
            if self.tray.is_none() && self.frames < 1200 {
                ctx.request_repaint_after(Duration::from_millis(200));
            }
        }

        // NVIDIA DWM "sheet of glass" fixes the gray box on some dGPUs, but paints
        // a glass rectangle on others where GL alpha already worked (v0.3.0).
        // Opt-in via 设置 → 修复灰框. Apply before caption suppress so the
        // FRAMECHANGED from extend cannot leave WS_CAPTION on for this present.
        #[cfg(target_os = "windows")]
        if let Some(is_nv) = gl_vendor_is_nvidia(frame) {
            let want = is_nv && self.fix_gray_box;
            let hwnd = self.main_hwnd.load(Ordering::Relaxed);
            if hwnd != 0 {
                if want && !self.nvidia_hwnd_fix_on {
                    enable_nvidia_per_pixel_alpha(hwnd);
                    self.nvidia_hwnd_fix_on = true;
                } else if !want && self.nvidia_hwnd_fix_on {
                    disable_nvidia_per_pixel_alpha(hwnd);
                    self.nvidia_hwnd_fix_on = false;
                }
            }
        }

        // winit keeps WS_CAPTION for aero snap and restores it after DPI/style
        // refreshes. On some NVIDIA DWM setups that paints the window title in the
        // client area (often the font subfamily "Normal") at the top of the hwnd.
        // Run even while hidden so the first ShowWindow cannot flash the caption.
        #[cfg(target_os = "windows")]
        {
            let hwnd = self.main_hwnd.load(Ordering::Relaxed);
            if hwnd != 0 {
                hide_hwnd_from_taskbar(hwnd);
                ensure_orb_click_guard(hwnd);
                self.taskbar_hidden = true;
            }
        }

        let visible = self.window_visible.load(Ordering::Relaxed);

        // User hid the orb — skip painting; tray show will request_repaint.
        if !visible {
            return;
        }

        // Fullscreen hide/show is driven by spawn_fullscreen_poller (the GUI loop
        // often stops while the hwnd is SW_HIDE, so we cannot restore from here).
        if self.fullscreen_occluded.load(Ordering::Relaxed) {
            ctx.request_repaint_after(Duration::from_millis(200));
            return;
        }

        // Apply CJK font after the orb has already painted at least once.
        if !self.fonts_applied && self.frames >= 2 {
            if self.font_job_done.load(Ordering::Relaxed) {
                if let Some((bytes, index)) = lock_mutex(&self.pending_font).take() {
                    apply_cjk_font(ctx, bytes, index);
                }
                self.fonts_applied = true;
            } else {
                ctx.request_repaint_after(Duration::from_millis(50));
            }
        }
        let (temp, desc, city, err, loading, new_scene, new_is_day, snapshot, rain_soon) = {
            let w = lock_mutex(&self.weather);
            let new_scene = w.data.as_ref().map(|d| d.scene).filter(|&s| {
                self.applied_scene != Some(s)
            });
            let new_is_day = w.data.as_ref().map(|d| d.is_day);
            let snapshot = if self.detail_t > 0.02 {
                w.data.clone()
            } else {
                None
            };
            let rain_soon = w.data.as_ref().and_then(|d| d.rain_soon);
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
                new_is_day,
                snapshot,
                rain_soon,
            )
        };
        if let Some(scene) = new_scene {
            self.applied_scene = Some(scene);
        }
        if let Some(is_day) = new_is_day {
            self.api_is_day = is_day;
        }

        if !loading && self.last_fetch.elapsed() > Duration::from_secs(REFRESH_SECS) {
            self.last_fetch = Instant::now();
            spawn_weather_fetch(Arc::clone(&self.weather));
        }

        let t = self.started.elapsed().as_secs_f32();
        let dt = ctx.input(|i| i.stable_dt).min(0.05);
        let want_day = if self.debug_mode {
            effective_is_day(self.day_night, self.api_is_day)
        } else if env_force_night() {
            false
        } else {
            self.api_is_day
        };
        let want_scene = if self.debug_mode {
            match self.preview_index {
                Some(i) => PREVIEW_SCENES[i].scene,
                None => self.applied_scene.unwrap_or(self.scene),
            }
        } else {
            self.applied_scene.unwrap_or(self.scene)
        };
        self.tick_scene_fade(dt, want_scene, want_day);
        let bob = (t * TAU / 5.5).sin() * 2.5;
        let rain_hint = if self.debug_mode {
            match self.preview_index {
                Some(i) => PREVIEW_SCENES[i].rain_soon,
                None => rain_soon.filter(|_| {
                    !self.scene.is_precip_rain() && !self.scene.is_snow()
                }),
            }
        } else {
            rain_soon.filter(|_| {
                !self.scene.is_precip_rain() && !self.scene.is_snow()
            })
        };

        match self.scene {
            s if s.is_precip_rain() => {
                advance_drops(&mut self.drops, dt, s.drop_fast());
            }
            s if s.is_snow() => advance_flakes(&mut self.flakes, dt, s.intensity()),
            _ => {}
        }

        // Animate panel expand/collapse (Vue-like ease + fade).
        let panel_open = self.detail_open || self.settings_open;
        let target_t = if panel_open { 1.0 } else { 0.0 };
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

        let autostart_on = self.open_at_login.load(Ordering::Relaxed);
        let autostart_want = if autostart_on { 1.0 } else { 0.0 };
        tick_toward(&mut self.autostart_t, autostart_want, dt, 0.16);
        let debug_want = if self.debug_mode { 1.0 } else { 0.0 };
        tick_toward(&mut self.debug_t, debug_want, dt, 0.16);
        let lock_want = if self.lock_position { 1.0 } else { 0.0 };
        tick_toward(&mut self.lock_t, lock_want, dt, 0.16);
        let gray_want = if self.fix_gray_box { 1.0 } else { 0.0 };
        tick_toward(&mut self.fix_gray_box_t, gray_want, dt, 0.16);

        let height_ease = ease_in_out_cubic(self.detail_t);
        // Opacity finishes sooner so the panel is gone before it would feel clipped.
        let panel_opacity = smoothstep(0.0, 0.55, self.detail_t);
        // Window size stays H_DETAIL forever — only the panel clip/opacity animates.
        let win_h = H_DETAIL;
        let compact_ui = self.detail_t < 0.18;
        let scene_fading = self.fx_out || self.fx_alpha < 0.999;
        let still_animating = (self.detail_t - target_t).abs() > 0.0005
            || scene_fading
            || self.city_picker.open
            || (self.autostart_t - autostart_want).abs() > 0.002
            || (self.debug_t - debug_want).abs() > 0.002
            || (self.lock_t - lock_want).abs() > 0.002
            || (self.fix_gray_box_t - gray_want).abs() > 0.002;

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
                let mut cycle_day_night = false;
                let mut open_picker = false;
                let mut close_picker = false;
                let mut pick_city: Option<cities::CityOption> = None;
                let mut use_locate = false;
                let mut toggle_autostart = false;
                let mut toggle_debug = false;
                let mut toggle_lock = false;
                let mut toggle_gray_box = false;
                let mut new_ball_scale: Option<f32> = None;
                let mut new_ball_opacity: Option<f32> = None;
                let mut open_skins = false;
                let mut skins_back = false;
                let mut pick_skin: Option<&'static str> = None;

                if !self.detail_open && self.city_picker.open {
                    self.city_picker.close();
                }
                if self.city_picker.open {
                    self.city_picker.poll(ctx);
                }

                // Draw panel under the orb so any fade residue never sits on the ball.
                if panel_opacity > 0.02 {
                    let top = BALL_CENTER_Y + ball_r() + 18.0;
                    let full_bottom = H_DETAIL - 10.0;
                    let full_h = (full_bottom - top).max(0.0);
                    let visible_h = (full_h * height_ease).max(0.0);
                    let bottom = top + visible_h;
                    if bottom > top + 4.0 {
                        let pr = Rect::from_min_max(
                            Pos2::new(8.0, top),
                            Pos2::new(W - 8.0, bottom),
                        );
                        let interactive = panel_opacity > 0.85 && panel_open;
                        let theme = PanelTheme::from_id(self.panel_skin.as_str());
                        let skin_tex = self.panel_tex();
                        let skin_tex_ref = skin_tex.as_ref();
                        match self.panel_kind {
                            PanelKind::Settings => {
                                let actions = paint_settings_panel(
                                    ui,
                                    pr,
                                    panel_opacity,
                                    interactive,
                                    self.autostart_t,
                                    self.debug_t,
                                    self.lock_t,
                                    self.fix_gray_box_t,
                                    self.ball_scale,
                                    self.ball_opacity,
                                    &mut self.settings_scroll,
                                    &mut self.skin_tip_hover_since,
                                    theme,
                                    skin_tex_ref,
                                );
                                close_clicked = actions.close;
                                toggle_autostart = actions.toggle_autostart;
                                toggle_debug = actions.toggle_debug;
                                toggle_lock = actions.toggle_lock;
                                toggle_gray_box = actions.toggle_gray_box;
                                new_ball_scale = actions.ball_scale;
                                new_ball_opacity = actions.ball_opacity;
                                open_skins = actions.open_skins;
                            }
                            PanelKind::Skins => {
                                let actions = paint_skins_panel(
                                    ui,
                                    pr,
                                    panel_opacity,
                                    interactive,
                                    self.panel_skin.as_str(),
                                    theme,
                                    skin_tex_ref,
                                );
                                skins_back = actions.back;
                                pick_skin = actions.pick;
                            }
                            PanelKind::Detail => {
                                let actions = paint_detail_panel(
                                    ui,
                                    pr,
                                    snapshot.as_ref(),
                                    loading,
                                    panel_opacity,
                                    interactive,
                                    self.is_day,
                                    self.day_night,
                                    self.is_manual_city,
                                    self.debug_mode,
                                    rain_hint,
                                    &mut self.city_picker,
                                    theme,
                                    skin_tex_ref,
                                );
                                close_clicked = actions.close;
                                refresh_clicked = actions.refresh;
                                cycle_day_night = actions.cycle_day_night;
                                open_picker = actions.open_picker;
                                close_picker = actions.close_picker;
                                pick_city = actions.pick;
                                use_locate = actions.use_locate;
                            }
                        }
                        panel_rect = Some(pr);
                    }
                }
                if close_clicked {
                    self.detail_open = false;
                    self.settings_open = false;
                    self.city_picker.close();
                    self.panel_kind = PanelKind::Detail;
                }
                if skins_back {
                    self.panel_kind = PanelKind::Settings;
                    self.settings_open = true;
                    ctx.request_repaint();
                }
                if open_skins {
                    self.panel_kind = PanelKind::Skins;
                    self.settings_open = true;
                    self.detail_open = false;
                    ctx.request_repaint();
                }
                if self.panel_kind != PanelKind::Settings {
                    self.skin_tip_hover_since = None;
                }
                if let Some(id) = pick_skin {
                    if self.panel_skin != id {
                        self.panel_skin = id.to_string();
                        settings::save_panel_skin(id);
                    }
                    ctx.request_repaint();
                }
                if toggle_autostart {
                    let next = !autostart_on;
                    let enabled = settings::set_open_at_login(next);
                    self.open_at_login.store(enabled, Ordering::Relaxed);
                    ctx.request_repaint();
                }
                if toggle_debug {
                    self.debug_mode = !self.debug_mode;
                    settings::save_debug_mode(self.debug_mode);
                    if !self.debug_mode {
                        self.preview_index = None;
                        self.preview_temp_index = None;
                    }
                    ctx.request_repaint();
                }
                if toggle_lock {
                    self.lock_position = !self.lock_position;
                    settings::save_lock_position(self.lock_position);
                    ctx.request_repaint();
                }
                if toggle_gray_box {
                    self.fix_gray_box = !self.fix_gray_box;
                    settings::save_fix_gray_box(self.fix_gray_box);
                    ctx.request_repaint();
                }
                if let Some(v) = new_ball_scale {
                    let v = settings::clamp_ball_scale(v);
                    if (v - self.ball_scale).abs() > 0.0005 {
                        self.ball_scale = v;
                        set_paint_ball_r(ORB_R * self.ball_scale);
                        settings::save_ball_scale(self.ball_scale);
                    }
                }
                if let Some(v) = new_ball_opacity {
                    let v = settings::clamp_ball_opacity(v);
                    if (v - self.ball_opacity).abs() > 0.0005 {
                        self.ball_opacity = v;
                        settings::save_ball_opacity(self.ball_opacity);
                    }
                }
                if close_picker {
                    self.city_picker.close();
                }
                if open_picker {
                    self.city_picker.open_now();
                }
                if let Some(c) = pick_city {
                    settings::save_manual_city(settings::ManualCity {
                        latitude: c.latitude,
                        longitude: c.longitude,
                        label: c.name.clone(),
                    });
                    self.is_manual_city = true;
                    self.city_picker.close();
                    self.last_fetch = Instant::now();
                    spawn_weather_job(
                        Arc::clone(&self.weather),
                        FetchJob::At {
                            lat: c.latitude,
                            lon: c.longitude,
                            city: c.name,
                        },
                    );
                }
                if use_locate {
                    settings::clear_manual_city();
                    self.is_manual_city = false;
                    self.city_picker.close();
                    self.last_fetch = Instant::now();
                    spawn_weather_job(Arc::clone(&self.weather), FetchJob::ForceLocate);
                }
                if refresh_clicked && !loading {
                    self.last_fetch = Instant::now();
                    spawn_weather_fetch(Arc::clone(&self.weather));
                }
                if cycle_day_night {
                    self.cycle_day_night();
                }

                ui.scope(|ui| {
                    ui.multiply_opacity(self.ball_opacity);
                    paint_orb(
                        ui,
                        center,
                        t,
                        self.scene,
                        self.is_day,
                        if self.debug_mode {
                            self.preview_temp().or(temp)
                        } else {
                            temp
                        },
                        &self.drops,
                        &self.flakes,
                        &self.cloud_tex,
                        ease_in_out_cubic(self.fx_alpha),
                        self.fx_keep_clouds,
                        rain_hint,
                    );
                });

                let btn_rect = if self.debug_mode {
                    let y0 = if compact_ui {
                        BALL_CENTER_Y + ball_r() + 18.0
                    } else {
                        16.0
                    };
                    let day_rect = Rect::from_center_size(
                        Pos2::new(rect.center().x, y0),
                        Vec2::new(96.0, 26.0),
                    );
                    let scene_rect = Rect::from_center_size(
                        Pos2::new(rect.center().x, y0 + 32.0),
                        Vec2::new(96.0, 26.0),
                    );
                    let temp_rect = Rect::from_center_size(
                        Pos2::new(rect.center().x, y0 + 64.0),
                        Vec2::new(96.0, 26.0),
                    );

                    let day_btn = ui.interact(day_rect, ui.id().with("day-night-btn"), Sense::click());
                    paint_button(
                        ui,
                        day_rect,
                        &format!("外观 {}", self.day_night.label()),
                        day_btn.hovered(),
                        1.0,
                        PanelTheme::dark(),
                    );
                    if day_btn.clicked() {
                        self.cycle_day_night();
                    }

                    let scene_btn =
                        ui.interact(scene_rect, ui.id().with("preview-scene-btn"), Sense::click());
                    paint_button(
                        ui,
                        scene_rect,
                        &self.preview_button_label(),
                        scene_btn.hovered(),
                        1.0,
                        PanelTheme::dark(),
                    );
                    if scene_btn.clicked() {
                        self.cycle_preview_scene();
                    }
                    if scene_btn.secondary_clicked() {
                        self.clear_preview_scene();
                    }

                    let temp_btn =
                        ui.interact(temp_rect, ui.id().with("preview-temp-btn"), Sense::click());
                    paint_button(
                        ui,
                        temp_rect,
                        &self.preview_temp_label(),
                        temp_btn.hovered(),
                        1.0,
                        PanelTheme::dark(),
                    );
                    if temp_btn.clicked() {
                        self.cycle_preview_temp();
                    }
                    if temp_btn.secondary_clicked() {
                        self.preview_temp_index = None;
                    }
                    day_rect.union(scene_rect).union(temp_rect)
                } else {
                    Rect::from_min_size(Pos2::ZERO, Vec2::ZERO)
                };

                let (over_ball, over_btn, over_panel) = interactive_hits(
                    self.main_hwnd.load(Ordering::Relaxed),
                    ctx,
                    center,
                    ball_r() + 2.0,
                    btn_rect,
                    panel_rect.filter(|_| panel_opacity > 0.5),
                );
                pointer_busy = over_ball || over_btn || over_panel;

                let hit = Rect::from_center_size(center, Vec2::splat(ball_r() * 2.0));
                let ball = ui.interact(hit, ui.id().with("orb-drag"), Sense::click_and_drag());

                if over_ball && !over_btn {
                    if ball.drag_started_by(PointerButton::Primary) {
                        self.press_pos = ball.interact_pointer_pos();
                        self.suppress_click = false;
                    }
                    if !self.lock_position
                        && ball.dragged_by(PointerButton::Primary)
                        && !self.dragging
                    {
                        if let (Some(start), Some(cur)) =
                            (self.press_pos, ball.interact_pointer_pos())
                        {
                            if (cur - start).length() > DRAG_THRESHOLD {
                                self.dragging = true;
                                self.drag_grab = drag_grab_from_cursor(
                                    self.main_hwnd.load(Ordering::Relaxed),
                                );
                                #[cfg(target_os = "windows")]
                                if let Some(grab) = self.drag_grab {
                                    follow_cursor_drag(
                                        self.main_hwnd.load(Ordering::Relaxed),
                                        grab,
                                    );
                                }
                                #[cfg(not(target_os = "windows"))]
                                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                            }
                        }
                    }
                    if ball.drag_stopped() {
                        self.press_pos = None;
                        #[cfg(not(target_os = "windows"))]
                        {
                            if self.dragging {
                                self.dragging = false;
                                if !self.lock_position {
                                    self.persist_window_pos();
                                }
                            }
                        }
                    }
                    // egui clicked() is false when the pointer dragged
                    if ball.clicked() {
                        if self.suppress_click {
                            self.suppress_click = false;
                        } else if self.detail_open {
                            self.detail_open = false;
                        } else {
                            self.detail_open = true;
                            self.settings_open = false;
                            self.panel_kind = PanelKind::Detail;
                            self.city_picker.close();
                        }
                    }
                    if ball.secondary_clicked() {
                        if self.settings_open {
                            self.settings_open = false;
                        } else {
                            self.settings_open = true;
                            self.detail_open = false;
                            self.panel_kind = PanelKind::Settings;
                            self.city_picker.close();
                        }
                    }
                }

                // Sending MousePassthrough every frame storms Win32 and can freeze the orb.
                let passthrough = !self.dragging
                    && !(over_ball
                        || over_btn
                        || over_panel
                        || self.city_picker.open
                        || self.settings_open);
                if self.last_passthrough != Some(passthrough) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(passthrough));
                    self.last_passthrough = Some(passthrough);
                }

                // Tooltip only when mostly compact
                if over_ball && compact_ui && !over_btn {
                    let force_err = env_force_weather_error();
                    paint_tooltip(
                        ui,
                        Pos2::new(center.x, center.y - ball_r() + 6.0),
                        rect,
                        if force_err {
                            None
                        } else if self.debug_mode {
                            self.preview_temp().or(temp)
                        } else {
                            temp
                        },
                        &desc,
                        &city,
                        if force_err {
                            Some("天气请求失败")
                        } else {
                            err.as_deref()
                        },
                        loading && !force_err,
                        rain_hint.map(|s| s.hint()),
                    );
                }
            });

        let precip = self.scene.is_precip_rain() || self.scene.is_snow();
        let ms = if !self.fonts_applied {
            50
        } else if self.dragging {
            16
        } else if still_animating || precip || pointer_busy || rain_hint.is_some() {
            33
        } else {
            70
        };
        ctx.request_repaint_after(Duration::from_millis(ms));
    }
}

impl OrbApp {
    fn panel_tex(&self) -> Option<TextureHandle> {
        match self.panel_skin.as_str() {
            "silk" => Some(self.silk_tex.clone()),
            "changli" | "changli_a" => Some(self.changli_a_tex.clone()),
            "changli_b" => Some(self.changli_b_tex.clone()),
            "cartethyia_a" => Some(self.cartethyia_a_tex.clone()),
            "cartethyia_b" => Some(self.cartethyia_b_tex.clone()),
            _ => None,
        }
    }

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

    fn cycle_day_night(&mut self) {
        self.day_night = self.day_night.cycle();
        settings::save_day_night(self.day_night);
    }

    fn apply_scene(&mut self, scene: Scene) {
        self.scene = scene;
        self.drops = make_drops(scene);
        self.flakes = make_flakes(scene);
    }

    fn tick_scene_fade(&mut self, dt: f32, want_scene: Scene, want_day: bool) {
        let want = (want_scene, want_day);
        let showing = (self.scene, self.is_day);

        if showing != want {
            if self.fx_pending != Some(want) {
                self.fx_keep_clouds = same_precip_clouds(self.scene, want_scene);
            }
            self.fx_pending = Some(want);
            self.fx_out = true;
        }

        if self.fx_out {
            if showing == want {
                self.fx_out = false;
                self.fx_pending = None;
            } else {
                self.fx_alpha = (self.fx_alpha - dt / SCENE_FADE_SECS).max(0.0);
                if self.fx_alpha <= 0.0 {
                    if let Some((scene, is_day)) = self.fx_pending.take() {
                        self.apply_scene(scene);
                        self.is_day = is_day;
                    }
                    self.fx_out = false;
                    self.fx_alpha = 0.0;
                }
                return;
            }
        }

        if self.fx_alpha < 1.0 {
            self.fx_alpha = (self.fx_alpha + dt / SCENE_FADE_SECS).min(1.0);
            if self.fx_alpha >= 0.999 {
                self.fx_alpha = 1.0;
                self.fx_keep_clouds = false;
            }
        }
    }

    fn preview_button_label(&self) -> String {
        match self.preview_index {
            None => "试样式".into(),
            Some(i) => {
                let name = PREVIEW_SCENES[i].label;
                format!("{name} {}/{}", i + 1, PREVIEW_SCENES.len())
            }
        }
    }

    fn cycle_preview_scene(&mut self) {
        let next = match self.preview_index {
            None => Some(0),
            Some(i) if i + 1 < PREVIEW_SCENES.len() => Some(i + 1),
            Some(_) => None,
        };
        match next {
            Some(i) => {
                self.preview_index = Some(i);
            }
            None => self.clear_preview_scene(),
        }
    }

    fn clear_preview_scene(&mut self) {
        self.preview_index = None;
    }

    fn preview_temp(&self) -> Option<f32> {
        self.preview_temp_index.map(|i| PREVIEW_TEMPS[i])
    }

    fn preview_temp_label(&self) -> String {
        match self.preview_temp_index {
            None => "试气温".into(),
            Some(i) => format!("{}° {}/{}", PREVIEW_TEMPS[i].round() as i32, i + 1, PREVIEW_TEMPS.len()),
        }
    }

    fn cycle_preview_temp(&mut self) {
        self.preview_temp_index = match self.preview_temp_index {
            None => Some(0),
            Some(i) if i + 1 < PREVIEW_TEMPS.len() => Some(i + 1),
            Some(_) => None,
        };
    }
}

/// Hide/show while another app is fullscreen. Must not live on the GUI thread:
/// SW_HIDE stops eframe from ticking, so restore would never run.
fn spawn_fullscreen_poller(
    window_visible: Arc<AtomicBool>,
    main_hwnd: Arc<AtomicIsize>,
    fullscreen_occluded: Arc<AtomicBool>,
    ctx: egui::Context,
) {
    thread::spawn(move || {
        let mut hidden_by_fs = false;
        let mut clear_ticks = 0u8;
        loop {
            thread::sleep(Duration::from_millis(180));
            let hwnd = main_hwnd.load(Ordering::Relaxed);
            if hwnd == 0 {
                continue;
            }
            let user_on = window_visible.load(Ordering::Relaxed);
            let fs = display::should_auto_hide(hwnd);
            if fs {
                clear_ticks = 0;
                fullscreen_occluded.store(true, Ordering::Relaxed);
                if user_on && !hidden_by_fs {
                    win_set_window_visible(hwnd, false);
                    hidden_by_fs = true;
                }
                continue;
            }
            if hidden_by_fs {
                clear_ticks = clear_ticks.saturating_add(1);
                if clear_ticks < 2 {
                    continue;
                }
            }
            fullscreen_occluded.store(false, Ordering::Relaxed);
            if hidden_by_fs && user_on {
                win_set_window_visible(hwnd, true);
                ctx.request_repaint();
            }
            hidden_by_fs = false;
            clear_ticks = 0;
        }
    });
}

/// Tray clicks are handled off the GUI thread so hide/quit stay responsive.
/// Menu text is applied after TrackPopupMenu returns (SetMenuItemInfo can deadlock).
/// `tray_tid` owns the popup menu; `paint_tid` is the eframe thread.
fn spawn_tray_poller(
    window_visible: Arc<AtomicBool>,
    main_hwnd: Arc<AtomicIsize>,
    fullscreen_occluded: Arc<AtomicBool>,
    tray_hmenu: Arc<AtomicIsize>,
    tray_tid: u32,
    paint_tid: u32,
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
                    if show && fullscreen_occluded.load(Ordering::Relaxed) {
                        // Stay hidden until the fullscreen app exits.
                    } else {
                        win_set_window_visible(h, show);
                    }
                    ctx.request_repaint();
                    win_wake_gui_thread(paint_tid, h);
                    schedule_menu_text(Arc::clone(&tray_hmenu), 0, toggle_label(show), tray_tid);
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

/// After the tray menu closes, restore HWND_TOPMOST (SetForegroundWindow on the
/// hidden tray hwnd can shuffle z-order). Drawing itself is not blocked: the
/// menu runs on the tray thread, not the eframe thread.
fn spawn_tray_menu_wake(
    window_visible: Arc<AtomicBool>,
    main_hwnd: Arc<AtomicIsize>,
    tray_tid: u32,
    paint_tid: u32,
    ctx: egui::Context,
) {
    thread::spawn(move || {
        use tray_icon::{MouseButtonState, TrayIconEvent};
        loop {
            let Ok(ev) = TrayIconEvent::receiver().recv() else {
                break;
            };
            let TrayIconEvent::Click {
                button_state: MouseButtonState::Down,
                ..
            } = ev
            else {
                continue;
            };
            let visible = Arc::clone(&window_visible);
            let hwnd = Arc::clone(&main_hwnd);
            let ctx = ctx.clone();
            thread::spawn(move || {
                wait_tray_menu_closed(tray_tid);
                win_unstick_tray_popup();
                for _ in 0..3 {
                    let h = hwnd.load(Ordering::Relaxed);
                    ctx.request_repaint();
                    win_wake_gui_thread(paint_tid, h);
                    if visible.load(Ordering::Relaxed) {
                        win_reassert_topmost(h);
                    }
                    thread::sleep(Duration::from_millis(40));
                }
            });
        }
    });
}

fn wait_tray_menu_closed(gui_tid: u32) {
    let mut seen = false;
    for _ in 0..25 {
        if win_tray_menu_open(gui_tid) {
            seen = true;
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    if !seen {
        thread::sleep(Duration::from_millis(80));
        return;
    }
    for _ in 0..400 {
        if !win_tray_menu_open(gui_tid) {
            thread::sleep(Duration::from_millis(30));
            if !win_tray_menu_open(gui_tid) {
                return;
            }
        }
        thread::sleep(Duration::from_millis(40));
    }
}

fn schedule_menu_text(hmenu: Arc<AtomicIsize>, pos: u32, text: &'static str, gui_tid: u32) {
    thread::spawn(move || {
        wait_tray_menu_closed(gui_tid);
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

fn follow_cursor_drag(hwnd: isize, grab: (i32, i32)) {
    let Some(cursor) = screen_cursor_pos() else {
        return;
    };
    let scale = display::scale_for_window(hwnd).max(0.5);
    let win_w = display::window_rect(hwnd)
        .map(|(_, _, w, _)| w)
        .unwrap_or_else(|| (W * scale).round() as i32);
    let ball_sx = cursor.x.round() as i32 - grab.0;
    let ball_sy = cursor.y.round() as i32 - grab.1;
    let x = ball_sx - win_w / 2;
    let y = ball_sy - (BALL_CENTER_Y * scale).round() as i32;
    win_set_window_pos(hwnd, x, y);
}

fn drag_grab_from_cursor(hwnd: isize) -> Option<(i32, i32)> {
    let cursor = screen_cursor_pos()?;
    let (wx, wy, ww, _) = display::window_rect(hwnd)?;
    let scale = display::scale_for_window(hwnd).max(0.5);
    let ball_sx = wx + ww / 2;
    let ball_sy = wy + (BALL_CENTER_Y * scale).round() as i32;
    Some((
        cursor.x.round() as i32 - ball_sx,
        cursor.y.round() as i32 - ball_sy,
    ))
}

fn win_primary_down() -> bool {
    #[cfg(target_os = "windows")]
    {
        const VK_LBUTTON: i32 = 0x01;
        const VK_RBUTTON: i32 = 0x02;
        const SM_SWAPBUTTON: i32 = 23;
        extern "system" {
            fn GetAsyncKeyState(vkey: i32) -> i16;
            fn GetSystemMetrics(index: i32) -> i32;
        }
        let swapped = unsafe { GetSystemMetrics(SM_SWAPBUTTON) } != 0;
        let vk = if swapped { VK_RBUTTON } else { VK_LBUTTON };
        (unsafe { GetAsyncKeyState(vk) } as u16) & 0x8000 != 0
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
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

#[cfg(target_os = "windows")]
fn win_current_thread_id() -> u32 {
    extern "system" {
        fn GetCurrentThreadId() -> u32;
    }
    unsafe { GetCurrentThreadId() }
}

#[cfg(not(target_os = "windows"))]
fn win_current_thread_id() -> u32 {
    0
}

#[cfg(target_os = "windows")]
fn win_in_popup_menu(gui_tid: u32) -> bool {
    if gui_tid == 0 {
        return false;
    }
    const GUI_INMENUMODE: u32 = 0x0000_0020;
    const GUI_POPUPMENUMODE: u32 = 0x0000_0040;
    const GUI_SYSTEMMENUMODE: u32 = 0x0000_0080;
    #[repr(C)]
    struct GuiThreadInfo {
        cb_size: u32,
        flags: u32,
        hwnd_active: *mut std::ffi::c_void,
        hwnd_focus: *mut std::ffi::c_void,
        hwnd_capture: *mut std::ffi::c_void,
        hwnd_menu_owner: *mut std::ffi::c_void,
        hwnd_move_size: *mut std::ffi::c_void,
        hwnd_caret: *mut std::ffi::c_void,
        rc_caret: [i32; 4],
    }
    extern "system" {
        fn GetGUIThreadInfo(id_thread: u32, info: *mut GuiThreadInfo) -> i32;
    }
    let mut info = GuiThreadInfo {
        cb_size: std::mem::size_of::<GuiThreadInfo>() as u32,
        flags: 0,
        hwnd_active: std::ptr::null_mut(),
        hwnd_focus: std::ptr::null_mut(),
        hwnd_capture: std::ptr::null_mut(),
        hwnd_menu_owner: std::ptr::null_mut(),
        hwnd_move_size: std::ptr::null_mut(),
        hwnd_caret: std::ptr::null_mut(),
        rc_caret: [0; 4],
    };
    unsafe {
        if GetGUIThreadInfo(gui_tid, &mut info) == 0 {
            return false;
        }
    }
    (info.flags & (GUI_INMENUMODE | GUI_POPUPMENUMODE | GUI_SYSTEMMENUMODE)) != 0
        || !info.hwnd_menu_owner.is_null()
}

#[cfg(not(target_os = "windows"))]
fn win_in_popup_menu(_gui_tid: u32) -> bool {
    false
}

fn win_tray_menu_open(gui_tid: u32) -> bool {
    win_in_popup_menu(gui_tid) || win_popup_menu_hwnd() != 0
}

#[cfg(target_os = "windows")]
fn win_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(target_os = "windows")]
fn win_find_window_class(class: &str) -> isize {
    extern "system" {
        fn FindWindowW(class: *const u16, name: *const u16) -> *mut std::ffi::c_void;
    }
    let class = win_wide(class);
    unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) as isize }
}

#[cfg(not(target_os = "windows"))]
fn win_find_window_class(_class: &str) -> isize {
    0
}

/// Popup menus use the system class `#32768` (MAKEINTATOM 32768).
#[cfg(target_os = "windows")]
fn win_popup_menu_hwnd() -> isize {
    extern "system" {
        fn FindWindowW(class: *const u16, name: *const u16) -> *mut std::ffi::c_void;
    }
    unsafe { FindWindowW(32768u16 as *const u16, std::ptr::null()) as isize }
}

#[cfg(not(target_os = "windows"))]
fn win_popup_menu_hwnd() -> isize {
    0
}

/// MSDN: after TrackPopupMenu, PostMessage(owner, WM_NULL) so the nested loop
/// actually returns. tray-icon 0.19 omits this.
#[cfg(target_os = "windows")]
fn win_unstick_tray_popup() {
    let tray = win_find_window_class("tray_icon_app");
    if tray != 0 {
        win_post_null(tray);
    }
}

#[cfg(not(target_os = "windows"))]
fn win_unstick_tray_popup() {}

#[cfg(target_os = "windows")]
fn win_post_null(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    const WM_NULL: u32 = 0;
    extern "system" {
        fn PostMessageW(
            h_wnd: *mut std::ffi::c_void,
            msg: u32,
            wparam: usize,
            lparam: isize,
        ) -> i32;
    }
    unsafe {
        PostMessageW(hwnd as *mut std::ffi::c_void, WM_NULL, 0, 0);
    }
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn win_post_null_enum(hwnd: *mut std::ffi::c_void, _lparam: isize) -> i32 {
    const WM_NULL: u32 = 0;
    extern "system" {
        fn PostMessageW(
            h_wnd: *mut std::ffi::c_void,
            msg: u32,
            wparam: usize,
            lparam: isize,
        ) -> i32;
    }
    unsafe {
        PostMessageW(hwnd, WM_NULL, 0, 0);
    }
    1
}

/// Wake winit's MsgWaitForMultipleObjectsEx by posting to every top-level
/// window on the GUI thread, plus a thread-queue WM_NULL.
#[cfg(target_os = "windows")]
fn win_wake_gui_thread(gui_tid: u32, extra_hwnd: isize) {
    const WM_NULL: u32 = 0;
    extern "system" {
        fn EnumThreadWindows(
            tid: u32,
            cb: Option<unsafe extern "system" fn(*mut std::ffi::c_void, isize) -> i32>,
            lparam: isize,
        ) -> i32;
        fn PostThreadMessageW(tid: u32, msg: u32, wparam: usize, lparam: isize) -> i32;
    }
    if gui_tid != 0 {
        unsafe {
            PostThreadMessageW(gui_tid, WM_NULL, 0, 0);
            EnumThreadWindows(gui_tid, Some(win_post_null_enum), 0);
        }
    }
    win_unstick_tray_popup();
    win_wake_event_loop(extra_hwnd);
}

#[cfg(not(target_os = "windows"))]
fn win_wake_gui_thread(_gui_tid: u32, extra_hwnd: isize) {
    win_wake_event_loop(extra_hwnd);
}

#[cfg(target_os = "windows")]
fn win_wake_event_loop(hwnd: isize) {
    win_post_null(hwnd);
}

#[cfg(not(target_os = "windows"))]
fn win_wake_event_loop(_hwnd: isize) {}

#[cfg(target_os = "windows")]
fn win_reassert_topmost(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    const HWND_TOPMOST: isize = -1;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOMOVE: u32 = 0x0002;
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
            HWND_TOPMOST as *mut std::ffi::c_void,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }
}

#[cfg(not(target_os = "windows"))]
fn win_reassert_topmost(_hwnd: isize) {}

fn ease_in_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 {
        4.0 * t * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
    }
}

/// Vue rain/snow/storm: clouds stay while only particles fade.
fn same_precip_clouds(from: Scene, to: Scene) -> bool {
    let kind = from.orb_kind();
    kind == to.orb_kind()
        && matches!(kind, OrbKind::Rain | OrbKind::Snow | OrbKind::Storm)
}

fn faded_painter(ui: &egui::Ui, opacity: f32) -> egui::Painter {
    let mut p = ui.painter().clone();
    p.multiply_opacity(opacity.clamp(0.0, 1.0));
    p
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

fn tick_toward(value: &mut f32, target: f32, dt: f32, secs: f32) {
    if (*value - target).abs() <= 0.002 {
        *value = target;
        return;
    }
    let step = dt / secs.max(0.001);
    if *value < target {
        *value = (*value + step).min(target);
    } else {
        *value = (*value - step).max(target);
    }
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
    cycle_day_night: bool,
    open_picker: bool,
    close_picker: bool,
    pick: Option<cities::CityOption>,
    use_locate: bool,
}

fn env_force_night() -> bool {
    matches!(
        std::env::var("WEATHERBALL_FORCE_NIGHT").ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE")
    )
}

fn env_force_weather_error() -> bool {
    matches!(
        std::env::var("WEATHERBALL_FORCE_ERROR").ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE")
    )
}

fn display_weather_error(err: &str) -> &'static str {
    if err.contains("解析") || err.contains("数据") {
        "天气数据异常"
    } else if err.contains("繁忙") {
        "天气服务繁忙"
    } else if err.contains("服务") {
        "天气服务异常"
    } else if err.contains("网络") || err.contains("连接") {
        "网络连接失败"
    } else {
        "天气请求失败"
    }
}

fn effective_is_day(pref: settings::DayNightPref, api_is_day: bool) -> bool {
    if env_force_night() {
        return false;
    }
    pref.apply(api_is_day)
}

#[derive(Clone, Copy)]
struct PanelTheme {
    bg: Color32,
    stroke: Color32,
    title: Color32,
    title_hover: Color32,
    body: Color32,
    muted: Color32,
    faint: Color32,
    chip_bg: Color32,
    chip_bg_hover: Color32,
    chip_fg: Color32,
    chip_fg_hover: Color32,
    row_bg: Color32,
    row_bg_hover: Color32,
    row_selected: Color32,
    accent: Color32,
    accent_fill: Color32,
    button_bg: Color32,
    button_bg_hover: Color32,
    button_stroke: Color32,
    button_fg: Color32,
    search_bg: Color32,
    search_stroke: Color32,
    search_stroke_focus: Color32,
    search_fg: Color32,
    cursor: Color32,
    pill_off: Color32,
    pill_off_hover: Color32,
    pill_on: Color32,
    pill_on_hover: Color32,
    pill_stroke_off: Color32,
    pill_stroke_on: Color32,
    knob: Color32,
    slider_track: Color32,
    slider_value: Color32,
    curve_line: Color32,
    curve_fill: Color32,
    curve_halo: Color32,
    curve_dot: Color32,
    scroll_track: Color32,
    rain: Color32,
    list_hover: Color32,
}

impl PanelTheme {
    fn from_id(id: &str) -> Self {
        match id {
            "silk" => Self::silk(),
            "changli" | "changli_a" | "changli_b" => Self::changli(),
            "cartethyia_a" | "cartethyia_b" => Self::cartethyia(),
            _ => Self::dark(),
        }
    }

    fn dark() -> Self {
        Self {
            bg: Color32::from_rgba_unmultiplied(12, 18, 32, 235),
            stroke: Color32::from_rgba_unmultiplied(255, 255, 255, 56),
            title: Color32::from_rgb(244, 247, 251),
            title_hover: Color32::WHITE,
            body: Color32::from_rgb(248, 250, 252),
            muted: Color32::from_rgba_unmultiplied(170, 185, 210, 220),
            faint: Color32::from_rgba_unmultiplied(255, 255, 255, 122),
            chip_bg: Color32::from_rgba_unmultiplied(255, 255, 255, 20),
            chip_bg_hover: Color32::from_rgba_unmultiplied(255, 255, 255, 41),
            chip_fg: Color32::from_rgba_unmultiplied(255, 255, 255, 191),
            chip_fg_hover: Color32::WHITE,
            row_bg: Color32::from_rgba_unmultiplied(255, 255, 255, 20),
            row_bg_hover: Color32::from_rgba_unmultiplied(255, 255, 255, 36),
            row_selected: Color32::from_rgba_unmultiplied(70, 120, 180, 70),
            accent: Color32::from_rgb(150, 205, 255),
            accent_fill: Color32::from_rgb(96, 164, 228),
            button_bg: Color32::from_rgba_unmultiplied(28, 40, 58, 190),
            button_bg_hover: Color32::from_rgba_unmultiplied(40, 55, 75, 210),
            button_stroke: Color32::from_rgba_unmultiplied(160, 190, 230, 90),
            button_fg: Color32::from_rgba_unmultiplied(230, 240, 255, 240),
            search_bg: Color32::from_rgba_unmultiplied(0, 0, 0, 72),
            search_stroke: Color32::from_rgba_unmultiplied(255, 255, 255, 46),
            search_stroke_focus: Color32::from_rgba_unmultiplied(255, 255, 255, 102),
            search_fg: Color32::from_rgb(248, 250, 252),
            cursor: Color32::from_rgba_unmultiplied(200, 220, 255, 220),
            pill_off: Color32::from_rgba_unmultiplied(255, 255, 255, 40),
            pill_off_hover: Color32::from_rgba_unmultiplied(255, 255, 255, 58),
            pill_on: Color32::from_rgb(96, 164, 228),
            pill_on_hover: Color32::from_rgb(130, 186, 240),
            pill_stroke_off: Color32::from_rgba_unmultiplied(255, 255, 255, 28),
            pill_stroke_on: Color32::from_rgba_unmultiplied(255, 255, 255, 50),
            knob: Color32::from_rgb(248, 251, 255),
            slider_track: Color32::from_rgba_unmultiplied(255, 255, 255, 32),
            slider_value: Color32::from_rgba_unmultiplied(210, 228, 248, 210),
            curve_line: Color32::from_rgb(127, 180, 240),
            curve_fill: Color32::from_rgba_unmultiplied(127, 180, 240, 56),
            curve_halo: Color32::from_rgba_unmultiplied(127, 180, 240, 70),
            curve_dot: Color32::from_rgba_unmultiplied(255, 255, 255, 235),
            scroll_track: Color32::from_rgba_unmultiplied(255, 255, 255, 22),
            rain: Color32::from_rgb(150, 205, 255),
            list_hover: Color32::from_rgba_unmultiplied(255, 255, 255, 31),
        }
    }

    fn changli() -> Self {
        Self {
            bg: Color32::from_rgba_unmultiplied(10, 8, 14, 242),
            stroke: Color32::from_rgba_unmultiplied(212, 168, 96, 70),
            accent: Color32::from_rgb(232, 150, 132),
            accent_fill: Color32::from_rgb(196, 86, 78),
            rain: Color32::from_rgb(236, 168, 148),
            curve_line: Color32::from_rgb(224, 140, 118),
            curve_fill: Color32::from_rgba_unmultiplied(196, 86, 78, 56),
            curve_halo: Color32::from_rgba_unmultiplied(224, 140, 118, 70),
            pill_on: Color32::from_rgb(196, 86, 78),
            pill_on_hover: Color32::from_rgb(220, 110, 96),
            ..Self::dark()
        }
    }

    fn cartethyia() -> Self {
        Self {
            bg: Color32::from_rgba_unmultiplied(8, 14, 32, 242),
            stroke: Color32::from_rgba_unmultiplied(186, 210, 240, 70),
            accent: Color32::from_rgb(168, 206, 245),
            accent_fill: Color32::from_rgb(96, 148, 214),
            rain: Color32::from_rgb(176, 214, 248),
            curve_line: Color32::from_rgb(150, 190, 240),
            curve_fill: Color32::from_rgba_unmultiplied(96, 148, 214, 56),
            curve_halo: Color32::from_rgba_unmultiplied(150, 190, 240, 70),
            pill_on: Color32::from_rgb(96, 148, 214),
            pill_on_hover: Color32::from_rgb(130, 176, 232),
            ..Self::dark()
        }
    }

    fn silk() -> Self {
        Self {
            bg: Color32::from_rgba_unmultiplied(232, 214, 228, 235),
            stroke: Color32::from_rgba_unmultiplied(255, 255, 255, 110),
            title: Color32::from_rgb(46, 40, 62),
            title_hover: Color32::from_rgb(32, 26, 48),
            body: Color32::from_rgb(46, 40, 62),
            muted: Color32::from_rgb(92, 84, 112),
            faint: Color32::from_rgb(118, 108, 136),
            chip_bg: Color32::from_rgba_unmultiplied(46, 40, 62, 24),
            chip_bg_hover: Color32::from_rgba_unmultiplied(46, 40, 62, 42),
            chip_fg: Color32::from_rgb(72, 64, 92),
            chip_fg_hover: Color32::from_rgb(46, 40, 62),
            row_bg: Color32::from_rgba_unmultiplied(255, 255, 255, 72),
            row_bg_hover: Color32::from_rgba_unmultiplied(255, 255, 255, 112),
            row_selected: Color32::from_rgba_unmultiplied(132, 112, 176, 72),
            accent: Color32::from_rgb(86, 108, 176),
            accent_fill: Color32::from_rgb(112, 132, 196),
            button_bg: Color32::from_rgba_unmultiplied(255, 255, 255, 98),
            button_bg_hover: Color32::from_rgba_unmultiplied(255, 255, 255, 142),
            button_stroke: Color32::from_rgba_unmultiplied(80, 70, 100, 46),
            button_fg: Color32::from_rgb(46, 40, 62),
            search_bg: Color32::from_rgba_unmultiplied(255, 255, 255, 148),
            search_stroke: Color32::from_rgba_unmultiplied(80, 70, 100, 40),
            search_stroke_focus: Color32::from_rgba_unmultiplied(90, 110, 180, 170),
            search_fg: Color32::from_rgb(46, 40, 62),
            cursor: Color32::from_rgb(90, 110, 180),
            pill_off: Color32::from_rgba_unmultiplied(46, 40, 62, 38),
            pill_off_hover: Color32::from_rgba_unmultiplied(46, 40, 62, 56),
            pill_on: Color32::from_rgb(112, 132, 196),
            pill_on_hover: Color32::from_rgb(132, 150, 214),
            pill_stroke_off: Color32::from_rgba_unmultiplied(46, 40, 62, 28),
            pill_stroke_on: Color32::from_rgba_unmultiplied(255, 255, 255, 80),
            knob: Color32::from_rgb(255, 252, 255),
            slider_track: Color32::from_rgba_unmultiplied(46, 40, 62, 30),
            slider_value: Color32::from_rgb(92, 84, 112),
            curve_line: Color32::from_rgb(90, 118, 188),
            curve_fill: Color32::from_rgba_unmultiplied(90, 118, 188, 72),
            curve_halo: Color32::from_rgba_unmultiplied(90, 118, 188, 80),
            curve_dot: Color32::from_rgb(255, 252, 255),
            scroll_track: Color32::from_rgba_unmultiplied(46, 40, 62, 28),
            rain: Color32::from_rgb(24, 72, 168),
            list_hover: Color32::from_rgba_unmultiplied(255, 255, 255, 96),
        }
    }
}

fn cover_uv(rect: Rect, tex_w: f32, tex_h: f32) -> Rect {
    let rw = rect.width().max(1.0);
    let rh = rect.height().max(1.0);
    let tw = tex_w.max(1.0);
    let th = tex_h.max(1.0);
    let scale = (rw / tw).max(rh / th);
    let vis_w = (rw / (tw * scale)).clamp(0.0, 1.0);
    let vis_h = (rh / (th * scale)).clamp(0.0, 1.0);
    Rect::from_min_size(
        Pos2::new((1.0 - vis_w) * 0.5, (1.0 - vis_h) * 0.5),
        Vec2::new(vis_w, vis_h),
    )
}

fn paint_detail_panel(
    ui: &mut egui::Ui,
    rect: Rect,
    data: Option<&LiveWeather>,
    loading: bool,
    opacity: f32,
    interactive: bool,
    visual_is_day: bool,
    day_night: settings::DayNightPref,
    is_manual: bool,
    debug_mode: bool,
    rain_soon: Option<PrecipSoon>,
    picker: &mut CityPicker,
    theme: PanelTheme,
    skin_tex: Option<&TextureHandle>,
) -> DetailActions {
    let mut actions = DetailActions {
        close: false,
        refresh: false,
        cycle_day_night: false,
        open_picker: false,
        close_picker: false,
        pick: None,
        use_locate: false,
    };
    if opacity < 0.02 {
        return actions;
    }

    let clip = ui.clip_rect().intersect(rect);
    let old_clip = ui.clip_rect();
    ui.set_clip_rect(clip);

    paint_panel_card(ui, rect, opacity, theme, skin_tex);

    if picker.open {
        paint_city_picker(ui, rect, loading, opacity, interactive, picker, &mut actions, theme);
        ui.set_clip_rect(old_clip);
        return actions;
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
    let city_r = Rect::from_min_max(
        Pos2::new(inner.min.x, y),
        Pos2::new(close_r.min.x - 16.0, y + 28.0),
    );
    let close = if interactive {
        ui.allocate_rect(close_r, Sense::click())
    } else {
        ui.allocate_rect(close_r, Sense::hover())
    };
    let city_hit = if interactive {
        ui.allocate_rect(city_r, Sense::click())
    } else {
        ui.allocate_rect(city_r, Sense::hover())
    };
    {
        let p = ui.painter();
        p.rect_filled(
            close_r,
            8.0,
            with_opacity(
                if close.hovered() && interactive {
                    theme.chip_bg_hover
                } else {
                    theme.chip_bg
                },
                opacity,
            ),
        );
        p.text(
            close_r.center(),
            egui::Align2::CENTER_CENTER,
            "×",
            egui::FontId::proportional(14.0),
            with_opacity(
                if close.hovered() && interactive {
                    theme.chip_fg_hover
                } else {
                    theme.chip_fg
                },
                opacity,
            ),
        );
        let city_col = if city_hit.hovered() && interactive {
            theme.title_hover
        } else {
            theme.title
        };
        let city_pos = Pos2::new(inner.min.x, y + 1.0);
        let city_font = egui::FontId::proportional(13.0);
        let galley = ui.fonts(|f| f.layout_no_wrap(city.to_string(), city_font.clone(), city_col));
        let city_w = galley.size().x;
        p.galley(city_pos, galley, city_col);
        if city_hit.hovered() && interactive {
            p.line_segment(
                [
                    Pos2::new(city_pos.x, city_pos.y + 16.0),
                    Pos2::new(city_pos.x + city_w, city_pos.y + 16.0),
                ],
                Stroke::new(1.0_f32, with_opacity(city_col, opacity)),
            );
        }
        p.text(
            Pos2::new(inner.min.x, y + 16.0),
            egui::Align2::LEFT_TOP,
            if is_manual { "自选城市" } else { "切换" },
            egui::FontId::proportional(10.0),
            with_opacity(theme.faint, opacity),
        );
        let accent = data
            .map(|d| d.scene.glow(visual_is_day))
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
    if interactive && city_hit.clicked() {
        actions.open_picker = true;
    }
    y += 28.0;

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
            with_opacity(theme.title, opacity),
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
            with_opacity(theme.body, opacity),
        );
    }
    y += 20.0;

    if let Some(soon) = rain_soon {
        let p = ui.painter();
        p.text(
            Pos2::new(inner.min.x, y),
            egui::Align2::LEFT_TOP,
            soon.hint(),
            egui::FontId::proportional(11.0),
            with_opacity(theme.rain, opacity),
        );
        y += 16.0;
    }

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
            with_opacity(theme.muted, opacity),
        );
        y += 14.0;
        p.text(
            Pos2::new(inner.min.x, y),
            egui::Align2::LEFT_TOP,
            wind,
            egui::FontId::proportional(10.0),
            with_opacity(theme.muted, opacity),
        );
    }
    y += 18.0;

    const FOOTER_H: f32 = 22.0;
    let debug_block = if debug_mode { 36.0 } else { 0.0 };
    let footer_top = inner.max.y - FOOTER_H;
    let content_bottom = (footer_top - debug_block).max(y);

    // Hourly sparkline — shrink to leftover space so it never covers the footer.
    if let Some(data) = data {
        if !data.hourly.is_empty() {
            let chart_h = (content_bottom - 6.0 - y).clamp(0.0, 96.0);
            if chart_h >= 40.0 {
                let chart = Rect::from_min_size(
                    Pos2::new(inner.min.x, y),
                    Vec2::new(inner.width(), chart_h),
                );
                paint_hourly_curve(ui, chart, &data.hourly, opacity, interactive, theme);
            }
        }
    }

    if debug_mode {
        let day_r = Rect::from_min_size(
            Pos2::new(inner.min.x, footer_top - 36.0),
            Vec2::new(inner.width(), 28.0),
        );
        let day_btn = if interactive {
            ui.allocate_rect(day_r, Sense::click())
        } else {
            ui.allocate_rect(day_r, Sense::hover())
        };
        paint_button(
            ui,
            day_r,
            &format!("外观 {}", day_night.label()),
            day_btn.hovered() && interactive,
            opacity,
            theme,
        );
        if interactive && day_btn.clicked() {
            actions.cycle_day_night = true;
        }
    }

    let updated = data
        .map(|d| format!("更新于 {}", d.updated_hm))
        .unwrap_or_else(|| "更新于 --:--".into());
    let refresh_label = if loading { "刷新中" } else { "刷新" };
    let refresh_font = egui::FontId::proportional(10.0);
    let refresh_galley = ui.fonts(|f| {
        f.layout_no_wrap(
            refresh_label.to_string(),
            refresh_font.clone(),
            theme.accent,
        )
    });
    let refresh_w = (refresh_galley.size().x + 8.0).max(36.0);
    let refresh_r = Rect::from_min_max(
        Pos2::new(inner.max.x - refresh_w, footer_top),
        Pos2::new(inner.max.x, inner.max.y),
    );
    let refresh = if interactive {
        ui.allocate_rect(refresh_r, Sense::click())
    } else {
        ui.allocate_rect(refresh_r, Sense::hover())
    };
    {
        let p = ui.painter();
        p.text(
            Pos2::new(inner.min.x, footer_top + 4.0),
            egui::Align2::LEFT_TOP,
            updated,
            egui::FontId::proportional(10.0),
            with_opacity(theme.faint, opacity),
        );
        let refresh_col = if refresh.hovered() && interactive {
            theme.title_hover
        } else {
            theme.accent
        };
        p.text(
            Pos2::new(inner.max.x, footer_top + 4.0),
            egui::Align2::RIGHT_TOP,
            refresh_label,
            refresh_font,
            with_opacity(refresh_col, opacity),
        );
    }
    if interactive && refresh.clicked() && !loading {
        actions.refresh = true;
    }

    ui.set_clip_rect(old_clip);
    actions
}

struct SettingsActions {
    close: bool,
    open_skins: bool,
    toggle_autostart: bool,
    toggle_debug: bool,
    toggle_lock: bool,
    toggle_gray_box: bool,
    ball_scale: Option<f32>,
    ball_opacity: Option<f32>,
}

fn paint_header_chip(
    ui: &mut egui::Ui,
    rect: Rect,
    label: &str,
    interactive: bool,
    opacity: f32,
    theme: PanelTheme,
) -> bool {
    let hit = if interactive {
        ui.allocate_rect(rect, Sense::click())
    } else {
        ui.allocate_rect(rect, Sense::hover())
    };
    let hovered = hit.hovered() && interactive;
    {
        let p = ui.painter();
        p.rect_filled(
            rect,
            8.0,
            with_opacity(
                if hovered {
                    theme.chip_bg_hover
                } else {
                    theme.chip_bg
                },
                opacity,
            ),
        );
        let size = if label == "×" { 14.0 } else { 11.0 };
        p.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(size),
            with_opacity(
                if hovered {
                    theme.chip_fg_hover
                } else {
                    theme.chip_fg
                },
                opacity,
            ),
        );
    }
    interactive && hit.clicked()
}

fn paint_header_skin_chip(
    ui: &mut egui::Ui,
    rect: Rect,
    interactive: bool,
    opacity: f32,
    hover_since: &mut Option<Instant>,
    theme: PanelTheme,
) -> bool {
    const TIP_DELAY: Duration = Duration::from_millis(400);
    let hit = if interactive {
        ui.allocate_rect(rect, Sense::click())
    } else {
        ui.allocate_rect(rect, Sense::hover())
    };
    let hovered = hit.hovered() && interactive;
    {
        let p = ui.painter();
        p.rect_filled(
            rect,
            8.0,
            with_opacity(
                if hovered {
                    theme.chip_bg_hover
                } else {
                    theme.chip_bg
                },
                opacity,
            ),
        );
        let c = rect.center();
        let r = 3.15_f32;
        let dots = [
            (Vec2::new(-3.1, 1.4), Color32::from_rgb(96, 164, 228)),
            (Vec2::new(3.1, 1.4), Color32::from_rgb(120, 210, 170)),
            (Vec2::new(0.0, -2.6), Color32::from_rgb(245, 196, 110)),
        ];
        for (off, col) in dots {
            p.circle_filled(c + off, r, with_opacity(col, opacity));
        }
    }
    if hovered {
        let since = hover_since.get_or_insert_with(Instant::now);
        let elapsed = since.elapsed();
        if elapsed >= TIP_DELAY {
            paint_mini_tip(ui, rect, "皮肤", opacity);
        } else {
            ui.ctx().request_repaint_after(TIP_DELAY.saturating_sub(elapsed));
        }
    } else {
        *hover_since = None;
    }
    interactive && hit.clicked()
}

fn paint_mini_tip(ui: &mut egui::Ui, anchor: Rect, text: &str, opacity: f32) {
    let font = egui::FontId::proportional(11.0);
    let color = with_opacity(Color32::from_rgb(244, 247, 251), opacity);
    let galley = ui.fonts(|f| f.layout_no_wrap(text.to_string(), font, color));
    let pad = Vec2::new(7.0, 4.0);
    let size = galley.size() + pad * 2.0;
    let mut x = anchor.center().x - size.x * 0.5;
    let panel = ui.clip_rect();
    x = x.clamp(panel.min.x + 6.0, (panel.max.x - 6.0 - size.x).max(panel.min.x + 6.0));
    let tip = Rect::from_min_size(Pos2::new(x, anchor.max.y + 4.0), size);
    let p = ui.painter();
    p.rect_filled(
        tip,
        8.0,
        with_opacity(Color32::from_rgba_unmultiplied(12, 18, 32, 242), opacity),
    );
    p.rect_stroke(
        tip,
        8.0,
        Stroke::new(
            1.0_f32,
            with_opacity(Color32::from_rgba_unmultiplied(255, 255, 255, 40), opacity),
        ),
    );
    p.galley(tip.min + pad, galley, color);
}

fn paint_panel_card(
    ui: &mut egui::Ui,
    rect: Rect,
    opacity: f32,
    theme: PanelTheme,
    skin_tex: Option<&TextureHandle>,
) {
    let p = ui.painter();
    if let Some(tex) = skin_tex {
        let tint = Color32::from_white_alpha((opacity * 255.0).round().clamp(0.0, 255.0) as u8);
        let size = tex.size();
        let uv = cover_uv(rect, size[0] as f32, size[1] as f32);
        p.add(egui::epaint::RectShape {
            rect,
            rounding: egui::Rounding::same(16.0),
            fill: tint,
            stroke: Stroke::NONE,
            blur_width: 0.0,
            fill_texture_id: tex.id(),
            uv,
        });
    } else {
        p.rect_filled(rect, 16.0, with_opacity(theme.bg, opacity));
    }
    p.rect_stroke(
        rect,
        16.0,
        Stroke::new(1.0_f32, with_opacity(theme.stroke, opacity)),
    );
}

fn paint_settings_panel(
    ui: &mut egui::Ui,
    rect: Rect,
    opacity: f32,
    interactive: bool,
    autostart_t: f32,
    debug_t: f32,
    lock_t: f32,
    gray_box_t: f32,
    ball_scale: f32,
    ball_opacity: f32,
    scroll: &mut f32,
    skin_tip_hover_since: &mut Option<Instant>,
    theme: PanelTheme,
    skin_tex: Option<&TextureHandle>,
) -> SettingsActions {
    let mut actions = SettingsActions {
        close: false,
        open_skins: false,
        toggle_autostart: false,
        toggle_debug: false,
        toggle_lock: false,
        toggle_gray_box: false,
        ball_scale: None,
        ball_opacity: None,
    };
    if opacity < 0.02 {
        return actions;
    }

    const HEADER_H: f32 = 28.0;
    const SETTING_BLOCK: f32 = 64.0;
    const SLIDER_BLOCK: f32 = 54.0;
    const CONTENT_H: f32 = SETTING_BLOCK * 4.0 + SLIDER_BLOCK * 2.0 + 8.0;

    let clip = ui.clip_rect().intersect(rect);
    let old_clip = ui.clip_rect();
    ui.set_clip_rect(clip);
    paint_panel_card(ui, rect, opacity, theme, skin_tex);

    let pad = 14.0;
    let inner = rect.shrink(pad);
    let mut y = inner.min.y;

    let close_r = Rect::from_min_size(
        Pos2::new(inner.max.x - 22.0, y),
        Vec2::new(22.0, 18.0),
    );
    let skin_r = Rect::from_min_size(
        Pos2::new(close_r.min.x - 6.0 - 22.0, y),
        Vec2::new(22.0, 18.0),
    );
    if paint_header_skin_chip(ui, skin_r, interactive, opacity, skin_tip_hover_since, theme) {
        actions.open_skins = true;
    }
    if paint_header_chip(ui, close_r, "×", interactive, opacity, theme) {
        actions.close = true;
    }
    {
        let p = ui.painter();
        p.text(
            Pos2::new(inner.min.x, y + 1.0),
            egui::Align2::LEFT_TOP,
            "设置",
            egui::FontId::proportional(13.0),
            with_opacity(theme.title, opacity),
        );
    }
    y += HEADER_H;

    let body = Rect::from_min_max(
        Pos2::new(inner.min.x, y),
        Pos2::new(inner.max.x, inner.max.y),
    );
    let view_h = body.height().max(0.0);
    let max_scroll = (CONTENT_H - view_h).max(0.0);
    if interactive && ui.rect_contains_pointer(rect) {
        let dy = ui.ctx().input_mut(|i| {
            let d = i.smooth_scroll_delta.y;
            if d.abs() > 0.01 {
                i.smooth_scroll_delta.y = 0.0;
            }
            d
        });
        if dy.abs() > 0.01 {
            *scroll = (*scroll - dy).clamp(0.0, max_scroll);
            ui.ctx().request_repaint();
        }
    }
    *scroll = scroll.clamp(0.0, max_scroll);

    let gutter = if max_scroll > 1.0 { 8.0 } else { 0.0 };
    let layout = Rect::from_min_max(
        Pos2::new(inner.min.x, inner.min.y),
        Pos2::new((inner.max.x - gutter).max(inner.min.x + 40.0), inner.min.y + 4000.0),
    );

    ui.set_clip_rect(clip.intersect(body));
    let mut cy = body.min.y - *scroll;

    if let Some(clicked) = paint_setting_row(
        ui,
        layout,
        &mut cy,
        body,
        "开机自启",
        "开机后自动打开天气球",
        autostart_t,
        interactive,
        opacity,
        theme,
    ) {
        if clicked {
            actions.toggle_autostart = true;
        }
    }
    if let Some(clicked) = paint_setting_row(
        ui,
        layout,
        &mut cy,
        body,
        "调试模式",
        "球体下方显示调试按钮",
        debug_t,
        interactive,
        opacity,
        theme,
    ) {
        if clicked {
            actions.toggle_debug = true;
        }
    }
    if let Some(clicked) = paint_setting_row(
        ui,
        layout,
        &mut cy,
        body,
        "锁定位置",
        "禁止拖动球体",
        lock_t,
        interactive,
        opacity,
        theme,
    ) {
        if clicked {
            actions.toggle_lock = true;
        }
    }
    if let Some(clicked) = paint_setting_row(
        ui,
        layout,
        &mut cy,
        body,
        "修复灰框",
        "独显出现灰底时再打开",
        gray_box_t,
        interactive,
        opacity,
        theme,
    ) {
        if clicked {
            actions.toggle_gray_box = true;
        }
    }
    if let Some(v) = paint_slider_row(
        ui,
        layout,
        &mut cy,
        body,
        "大小",
        ball_scale,
        settings::BALL_SCALE_MIN,
        settings::BALL_SCALE_MAX,
        interactive,
        opacity,
        "ball-scale",
        theme,
    ) {
        actions.ball_scale = Some(v);
    }
    if let Some(v) = paint_slider_row(
        ui,
        layout,
        &mut cy,
        body,
        "不透明度",
        ball_opacity,
        settings::BALL_OPACITY_MIN,
        settings::BALL_OPACITY_MAX,
        interactive,
        opacity,
        "ball-opacity",
        theme,
    ) {
        actions.ball_opacity = Some(v);
    }

    ui.set_clip_rect(clip);
    if max_scroll > 1.0 && body.height() > 12.0 {
        let bar = Rect::from_min_max(
            Pos2::new(inner.max.x - 4.0, body.min.y + 2.0),
            Pos2::new(inner.max.x, body.max.y - 2.0),
        );
        let thumb_h = ((view_h / CONTENT_H) * bar.height()).clamp(14.0, bar.height());
        let travel = (bar.height() - thumb_h).max(0.0);
        let thumb_t = if max_scroll > 0.0 {
            *scroll / max_scroll
        } else {
            0.0
        };
        let thumb = Rect::from_min_size(
            Pos2::new(bar.min.x, bar.min.y + travel * thumb_t),
            Vec2::new(bar.width(), thumb_h),
        );
        let bar_hit = if interactive {
            ui.allocate_rect(bar, Sense::click_and_drag())
        } else {
            ui.allocate_rect(bar, Sense::hover())
        };
        if interactive && (bar_hit.dragged() || bar_hit.clicked()) {
            if let Some(pos) = bar_hit.interact_pointer_pos() {
                let t = ((pos.y - bar.min.y - thumb_h * 0.5) / travel.max(1.0)).clamp(0.0, 1.0);
                *scroll = t * max_scroll;
                ui.ctx().request_repaint();
            }
        }
        let p = ui.painter();
        p.rect_filled(
            bar,
            2.0,
            with_opacity(theme.scroll_track, opacity),
        );
        p.rect_filled(
            thumb,
            2.0,
            with_opacity(
                if bar_hit.hovered() && interactive {
                    theme.accent
                } else {
                    theme.accent_fill
                },
                opacity,
            ),
        );
    }

    ui.set_clip_rect(old_clip);
    actions
}

struct SkinsActions {
    back: bool,
    pick: Option<&'static str>,
}

fn paint_skins_panel(
    ui: &mut egui::Ui,
    rect: Rect,
    opacity: f32,
    interactive: bool,
    current: &str,
    theme: PanelTheme,
    skin_tex: Option<&TextureHandle>,
) -> SkinsActions {
    let mut actions = SkinsActions {
        back: false,
        pick: None,
    };
    if opacity < 0.02 {
        return actions;
    }

    let clip = ui.clip_rect().intersect(rect);
    let old_clip = ui.clip_rect();
    ui.set_clip_rect(clip);
    paint_panel_card(ui, rect, opacity, theme, skin_tex);

    let pad = 14.0;
    let inner = rect.shrink(pad);
    let mut y = inner.min.y;

    let close_r = Rect::from_min_size(
        Pos2::new(inner.max.x - 22.0, y),
        Vec2::new(22.0, 18.0),
    );
    if paint_header_chip(ui, close_r, "×", interactive, opacity, theme) {
        actions.back = true;
    }
    {
        let p = ui.painter();
        p.text(
            Pos2::new(inner.min.x, y + 1.0),
            egui::Align2::LEFT_TOP,
            "皮肤",
            egui::FontId::proportional(13.0),
            with_opacity(theme.title, opacity),
        );
    }
    y += 28.0;

    let body = Rect::from_min_max(
        Pos2::new(inner.min.x, y),
        Pos2::new(inner.max.x, inner.max.y),
    );
    if body.height() > 8.0 {
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(body), |ui| {
            egui::ScrollArea::vertical()
                .id_salt("skins-list")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_min_width(body.width());
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 6.0);
                    let pairs = [
                        ("默认", "default", current == "default"),
                        ("浅色", "silk", current == "silk"),
                        (
                            "长离a",
                            "changli_a",
                            current == "changli" || current == "changli_a",
                        ),
                        ("长离b", "changli_b", current == "changli_b"),
                        ("卡提希娅a", "cartethyia_a", current == "cartethyia_a"),
                        ("卡提希娅b", "cartethyia_b", current == "cartethyia_b"),
                    ];
                    for (label, id, selected) in pairs {
                        if paint_skin_option(ui, body.width(), label, selected, interactive, opacity, theme)
                            && interactive
                        {
                            actions.pick = Some(id);
                        }
                    }
                });
        });
    }

    ui.set_clip_rect(old_clip);
    actions
}

fn paint_skin_option(
    ui: &mut egui::Ui,
    width: f32,
    label: &str,
    selected: bool,
    interactive: bool,
    opacity: f32,
    theme: PanelTheme,
) -> bool {
    let row_h = 40.0_f32;
    let (row, hit) = ui.allocate_exact_size(
        Vec2::new(width, row_h),
        if interactive {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let hovered = hit.hovered() && interactive;
    {
        let p = ui.painter();
        p.rect_filled(
            row,
            10.0,
            with_opacity(
                if selected {
                    theme.row_selected
                } else if hovered {
                    theme.row_bg_hover
                } else {
                    theme.row_bg
                },
                opacity,
            ),
        );
        p.text(
            Pos2::new(row.min.x + 10.0, row.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(12.0),
            with_opacity(theme.body, opacity),
        );
        if selected {
            p.text(
                Pos2::new(row.max.x - 10.0, row.center().y),
                egui::Align2::RIGHT_CENTER,
                "使用中",
                egui::FontId::proportional(10.0),
                with_opacity(theme.accent, opacity),
            );
        }
    }
    interactive && hit.clicked()
}

fn paint_setting_row(
    ui: &mut egui::Ui,
    inner: Rect,
    y: &mut f32,
    viewport: Rect,
    label: &str,
    hint: &str,
    pill_t: f32,
    interactive: bool,
    opacity: f32,
    theme: PanelTheme,
) -> Option<bool> {
    let pill_w = 36.0_f32;
    let pill_h = 20.0_f32;
    let row_h = 36.0_f32;
    let row = Rect::from_min_size(
        Pos2::new(inner.min.x, *y),
        Vec2::new(inner.width(), row_h),
    );
    *y = row.max.y + 4.0;
    let hint_font = egui::FontId::proportional(10.0);
    let hint_wrap = (inner.width() - 4.0).max(64.0);
    let hint_galley = ui.fonts(|f| {
        f.layout(
            hint.to_string(),
            hint_font,
            with_opacity(theme.faint, opacity),
            hint_wrap,
        )
    });
    let hint_top = *y;
    *y += hint_galley.size().y.max(12.0) + 6.0;

    if !row.intersects(viewport) && hint_top >= viewport.max.y {
        return Some(false);
    }

    let row_hit = if interactive && row.intersects(viewport) {
        ui.allocate_rect(row, Sense::click())
    } else {
        ui.allocate_rect(row, Sense::hover())
    };
    let pill = Rect::from_min_size(
        Pos2::new(row.max.x - 8.0 - pill_w, row.center().y - pill_h * 0.5),
        Vec2::new(pill_w, pill_h),
    );
    if row.intersects(viewport) {
        let p = ui.painter();
        p.rect_filled(
            row,
            10.0,
            with_opacity(
                if row_hit.hovered() && interactive {
                    theme.row_bg_hover
                } else {
                    theme.row_bg
                },
                opacity,
            ),
        );
        p.text(
            Pos2::new(row.min.x + 10.0, row.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(12.0),
            with_opacity(theme.body, opacity),
        );
        paint_pill_toggle(ui, pill, pill_t, row_hit.hovered() && interactive, opacity, theme);
    }
    if hint_top < viewport.max.y {
        ui.painter().galley(
            Pos2::new(inner.min.x + 2.0, hint_top),
            hint_galley,
            with_opacity(theme.faint, opacity),
        );
    }
    Some(interactive && row_hit.clicked())
}

fn paint_slider_row(
    ui: &mut egui::Ui,
    inner: Rect,
    y: &mut f32,
    viewport: Rect,
    label: &str,
    value: f32,
    min: f32,
    max: f32,
    interactive: bool,
    opacity: f32,
    id: &'static str,
    theme: PanelTheme,
) -> Option<f32> {
    let row_h = 48.0_f32;
    let row = Rect::from_min_size(
        Pos2::new(inner.min.x, *y),
        Vec2::new(inner.width(), row_h),
    );
    *y = row.max.y + 6.0;
    if !row.intersects(viewport) {
        return None;
    }

    let span = (max - min).max(0.0001);
    let track_h = 8.0;
    let track = Rect::from_min_max(
        Pos2::new(row.min.x + 10.0, row.min.y + 28.0),
        Pos2::new(row.max.x - 10.0, row.min.y + 28.0 + track_h),
    );
    let hit = Rect::from_min_max(
        Pos2::new(track.min.x - 2.0, track.min.y - 4.0),
        Pos2::new(track.max.x + 2.0, track.max.y + 6.0),
    );
    let sense = if interactive {
        Sense::click_and_drag()
    } else {
        Sense::hover()
    };
    let resp = ui.interact(hit, ui.id().with(id), sense);

    let mut next = None;
    if interactive && (resp.dragged() || resp.clicked()) {
        if let Some(pos) = resp.interact_pointer_pos() {
            let nt = ((pos.x - track.min.x) / track.width().max(1.0)).clamp(0.0, 1.0);
            next = Some(min + nt * span);
        }
    }

    let shown = next.unwrap_or(value);
    let shown_t = ((shown - min) / span).clamp(0.0, 1.0);
    {
        let p = ui.painter();
        p.rect_filled(
            row,
            10.0,
            with_opacity(
                if resp.hovered() && interactive {
                    theme.row_bg_hover
                } else {
                    theme.row_bg
                },
                opacity,
            ),
        );
        p.text(
            Pos2::new(row.min.x + 10.0, row.min.y + 8.0),
            egui::Align2::LEFT_TOP,
            label,
            egui::FontId::proportional(12.0),
            with_opacity(theme.body, opacity),
        );
        p.text(
            Pos2::new(row.max.x - 10.0, row.min.y + 8.0),
            egui::Align2::RIGHT_TOP,
            &format!("{}%", (shown * 100.0).round() as i32),
            egui::FontId::proportional(11.0),
            with_opacity(theme.slider_value, opacity),
        );
        p.rect_filled(
            track,
            4.0,
            with_opacity(theme.slider_track, opacity),
        );
        let fill_w = (track.width() * shown_t).max(if shown_t > 0.0 { 6.0 } else { 0.0 });
        if fill_w > 0.5 {
            p.rect_filled(
                Rect::from_min_size(track.min, Vec2::new(fill_w, track.height())),
                4.0,
                with_opacity(theme.accent_fill, opacity),
            );
        }
        let kx = track.min.x + track.width() * shown_t;
        p.circle_filled(
            Pos2::new(kx, track.center().y),
            6.5,
            with_opacity(theme.knob, opacity),
        );
    }

    next
}

fn paint_pill_toggle(
    ui: &mut egui::Ui,
    rect: Rect,
    on_t: f32,
    hovered: bool,
    opacity: f32,
    theme: PanelTheme,
) {
    let t = on_t.clamp(0.0, 1.0);
    let track_off = if hovered {
        theme.pill_off_hover
    } else {
        theme.pill_off
    };
    let track_on = if hovered {
        theme.pill_on_hover
    } else {
        theme.pill_on
    };
    let track = lerp_color(track_off, track_on, t);
    let rounding = rect.height() * 0.5;
    let p = ui.painter();
    p.rect_filled(rect, rounding, with_opacity(track, opacity));
    p.rect_stroke(
        rect,
        rounding,
        Stroke::new(
            1.0_f32,
            with_opacity(
                lerp_color(theme.pill_stroke_off, theme.pill_stroke_on, t),
                opacity,
            ),
        ),
    );
    let pad = 2.0;
    let knob_d = (rect.height() - pad * 2.0).max(8.0);
    let x0 = rect.min.x + pad;
    let x1 = rect.max.x - pad - knob_d;
    let x = x0 + (x1 - x0) * t;
    let center = Pos2::new(x + knob_d * 0.5, rect.center().y);
    p.circle_filled(center, knob_d * 0.5, with_opacity(theme.knob, opacity));
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        lerp(a.r() as f32, b.r() as f32, t).round() as u8,
        lerp(a.g() as f32, b.g() as f32, t).round() as u8,
        lerp(a.b() as f32, b.b() as f32, t).round() as u8,
        lerp(a.a() as f32, b.a() as f32, t).round() as u8,
    )
}

fn paint_city_picker(
    ui: &mut egui::Ui,
    rect: Rect,
    loading: bool,
    opacity: f32,
    interactive: bool,
    picker: &mut CityPicker,
    actions: &mut DetailActions,
    theme: PanelTheme,
) {
    let pad = 14.0;
    let inner = rect.shrink(pad);
    let mut y = inner.min.y;

    let close_r = Rect::from_min_size(
        Pos2::new(inner.max.x - 22.0, y),
        Vec2::new(22.0, 18.0),
    );
    let close = if interactive {
        ui.allocate_rect(close_r, Sense::click())
    } else {
        ui.allocate_rect(close_r, Sense::hover())
    };
    {
        let p = ui.painter();
        p.text(
            Pos2::new(inner.min.x, y + 1.0),
            egui::Align2::LEFT_TOP,
            "选择城市",
            egui::FontId::proportional(13.0),
            with_opacity(theme.title, opacity),
        );
        p.rect_filled(
            close_r,
            8.0,
            with_opacity(
                if close.hovered() && interactive {
                    theme.chip_bg_hover
                } else {
                    theme.chip_bg
                },
                opacity,
            ),
        );
        p.text(
            close_r.center(),
            egui::Align2::CENTER_CENTER,
            "×",
            egui::FontId::proportional(14.0),
            with_opacity(
                if close.hovered() && interactive {
                    theme.chip_fg_hover
                } else {
                    theme.chip_fg
                },
                opacity,
            ),
        );
    }
    if interactive && close.clicked() {
        actions.close_picker = true;
    }
    y += 26.0;

    let search_r = Rect::from_min_size(
        Pos2::new(inner.min.x, y),
        Vec2::new(inner.width(), 30.0),
    );
    let search_id = ui.id().with("city-search");
    let search_focused = ui.memory(|m| m.has_focus(search_id));
    {
        let p = ui.painter();
        p.rect_filled(
            search_r,
            10.0,
            with_opacity(theme.search_bg, opacity),
        );
        p.rect_stroke(
            search_r,
            10.0,
            Stroke::new(
                1.0_f32,
                with_opacity(
                    if search_focused {
                        theme.search_stroke_focus
                    } else {
                        theme.search_stroke
                    },
                    opacity,
                ),
            ),
        );
    }
    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(search_r), |ui| {
        ui.visuals_mut().override_text_color = Some(with_opacity(theme.search_fg, opacity));
        ui.visuals_mut().widgets.noninteractive.fg_stroke.color = with_opacity(theme.faint, opacity);
        ui.visuals_mut().text_cursor.stroke.color = theme.cursor;
        let resp = ui.add(
            egui::TextEdit::singleline(&mut picker.query)
                .id(search_id)
                .desired_width(ui.available_width())
                .hint_text("搜索城市或区县…")
                .font(egui::FontId::proportional(12.0))
                .frame(false)
                .margin(egui::Margin::symmetric(8.0, 7.0)),
        );
        if picker.focus_search {
            resp.request_focus();
            picker.focus_search = false;
        }
        if resp.changed() {
            picker.last_edit = Instant::now();
            if picker.query.trim().is_empty() {
                picker.searching = false;
                picker.results = cities::common_cities();
                picker.gen = picker.gen.wrapping_add(1);
                picker.issued.clear();
            }
        }
    });
    y += 38.0;

    let locate_h = 28.0;
    let locate_r = Rect::from_min_size(
        Pos2::new(inner.min.x, (inner.max.y - locate_h).max(y)),
        Vec2::new(inner.width(), locate_h),
    );
    let list_rect = Rect::from_min_max(
        Pos2::new(inner.min.x, y),
        Pos2::new(inner.max.x, (locate_r.min.y - 8.0).max(y)),
    );

    if list_rect.height() > 12.0 {
        let mut picked: Option<cities::CityOption> = None;
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(list_rect), |ui| {
            egui::ScrollArea::vertical()
                .id_salt("city-list")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 2.0);
                    ui.set_min_width(list_rect.width());
                    let hint_color = with_opacity(theme.faint, opacity);
                    if picker.searching {
                        ui.vertical_centered(|ui| {
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new("搜索中…")
                                    .size(11.0)
                                    .color(hint_color),
                            );
                        });
                    } else if picker.results.is_empty() {
                        ui.vertical_centered(|ui| {
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new("未找到城市或区县")
                                    .size(11.0)
                                    .color(hint_color),
                            );
                        });
                    } else {
                        for c in &picker.results {
                            let name = c.display_name();
                            let (row, resp) = ui.allocate_exact_size(
                                Vec2::new(list_rect.width(), 28.0),
                                if interactive {
                                    Sense::click()
                                } else {
                                    Sense::hover()
                                },
                            );
                            let p = ui.painter();
                            if resp.hovered() && interactive {
                                p.rect_filled(
                                    row,
                                    8.0,
                                    with_opacity(theme.list_hover, opacity),
                                );
                            }
                            p.text(
                                Pos2::new(row.min.x + 8.0, row.center().y),
                                egui::Align2::LEFT_CENTER,
                                name,
                                egui::FontId::proportional(12.0),
                                with_opacity(theme.body, opacity),
                            );
                            if interactive && resp.clicked() {
                                picked = Some(c.clone());
                            }
                        }
                    }
                });
        });
        if let Some(c) = picked {
            actions.pick = Some(c);
        }
    }

    let locate = if interactive {
        ui.allocate_rect(locate_r, Sense::click())
    } else {
        ui.allocate_rect(locate_r, Sense::hover())
    };
    {
        let p = ui.painter();
        let bg = if locate.hovered() && interactive && !loading {
            theme.button_bg_hover
        } else {
            theme.button_bg
        };
        p.rect_filled(locate_r, 10.0, with_opacity(bg, opacity));
        p.text(
            locate_r.center(),
            egui::Align2::CENTER_CENTER,
            if loading { "定位中…" } else { "使用自动定位" },
            egui::FontId::proportional(11.0),
            with_opacity(theme.button_fg, opacity),
        );
    }
    if interactive && locate.clicked() && !loading {
        actions.use_locate = true;
    }
}

fn paint_hourly_curve(
    ui: &mut egui::Ui,
    rect: Rect,
    points: &[HourlyPoint],
    opacity: f32,
    interactive: bool,
    theme: PanelTheme,
) {
    if points.len() < 2 {
        return;
    }

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
    {
        let p = ui.painter();
        p.text(
            Pos2::new(rect.min.x, y),
            egui::Align2::LEFT_TOP,
            format!("气温走势 · {}小时", points.len()),
            egui::FontId::proportional(10.0),
            with_opacity(theme.faint, opacity),
        );
        y += 14.0;
        p.text(
            Pos2::new(rect.min.x, y),
            egui::Align2::LEFT_TOP,
            format!("最低 {low}° · 最高 {high}°"),
            egui::FontId::proportional(9.0),
            with_opacity(theme.muted, opacity),
        );
        y += 14.0;
    }

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
    let smooth = smooth_open_polyline(&path, 8);

    let fill_col = with_opacity(theme.curve_fill, opacity);
    fill_under_polyline_aa(ui, &smooth, spark.max.y - 1.0, fill_col);

    let line_col = with_opacity(theme.curve_line, opacity);
    {
        let p = ui.painter();
        p.add(Shape::line(
            smooth.clone(),
            Stroke::new(3.2_f32, with_opacity(theme.curve_halo, opacity)),
        ));
        p.add(Shape::line(smooth, Stroke::new(1.8_f32, line_col)));
    }

    if interactive {
        ui.allocate_rect(spark, Sense::hover());
    }
    let hover_i = if interactive {
        ui.input(|i| i.pointer.hover_pos()).and_then(|pos| {
            let mut best_i = None;
            let mut best_d = 11.0_f32;
            for (i, pt) in path.iter().enumerate() {
                let d = (*pt - pos).length();
                if d <= best_d {
                    best_d = d;
                    best_i = Some(i);
                }
            }
            best_i
        })
    } else {
        None
    };

    for (i, pt) in path.iter().enumerate() {
        let hovered = hover_i == Some(i);
        let r = if hovered { 3.2 } else { 1.6 };
        let p = ui.painter();
        p.circle_filled(*pt, r, with_opacity(theme.curve_dot, opacity));
        p.circle_stroke(
            *pt,
            r,
            Stroke::new(
                if hovered { 1.0_f32 } else { 0.6_f32 },
                with_opacity(theme.curve_line, opacity),
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
    {
        let p = ui.painter();
        p.text(
            Pos2::new(rect.min.x, y),
            egui::Align2::LEFT_TOP,
            start_s,
            egui::FontId::proportional(9.0),
            with_opacity(theme.faint, opacity),
        );
        p.text(
            Pos2::new(rect.max.x, y),
            egui::Align2::RIGHT_TOP,
            end_s,
            egui::FontId::proportional(9.0),
            with_opacity(theme.faint, opacity),
        );
    }

    if let Some(i) = hover_i {
        if let Some(src) = points.get(i) {
            let label = format!("{}时  {}°", src.hour, src.temperature.round() as i32);
            paint_curve_hover_tip(ui, path[i], rect, &label, opacity, theme);
            ui.ctx().request_repaint();
        }
    }
}

fn paint_curve_hover_tip(
    ui: &mut egui::Ui,
    at: Pos2,
    bounds: Rect,
    text: &str,
    opacity: f32,
    theme: PanelTheme,
) {
    let font = egui::FontId::proportional(11.0);
    let color = with_opacity(theme.title, opacity);
    let galley = ui.fonts(|f| f.layout_no_wrap(text.to_string(), font, color));
    let pad = Vec2::new(7.0, 4.0);
    let size = galley.size() + pad * 2.0;
    let mut x = at.x - size.x * 0.5;
    x = x.clamp(bounds.min.x + 2.0, (bounds.max.x - 2.0 - size.x).max(bounds.min.x + 2.0));
    let mut y = at.y - size.y - 8.0;
    if y < bounds.min.y + 2.0 {
        y = at.y + 8.0;
    }
    if y + size.y > bounds.max.y - 2.0 {
        y = (bounds.max.y - 2.0 - size.y).max(bounds.min.y + 2.0);
    }
    let tip = Rect::from_min_size(Pos2::new(x, y), size);
    let old_clip = ui.clip_rect();
    ui.set_clip_rect(old_clip.union(tip.expand(2.0)));
    let p = ui.painter();
    p.rect_filled(
        tip,
        7.0,
        with_opacity(Color32::from_rgba_unmultiplied(12, 18, 32, 246), opacity),
    );
    p.rect_stroke(
        tip,
        7.0,
        Stroke::new(
            1.0_f32,
            with_opacity(Color32::from_rgba_unmultiplied(255, 255, 255, 48), opacity),
        ),
    );
    p.galley(tip.min + pad, galley, color);
    ui.set_clip_rect(old_clip);
}

/// Catmull-Rom samples so the stroke tessellates as short segments (less stair-step).
fn smooth_open_polyline(pts: &[Pos2], steps: usize) -> Vec<Pos2> {
    if pts.len() < 2 {
        return pts.to_vec();
    }
    let steps = steps.max(1);
    let mut out = Vec::with_capacity((pts.len() - 1) * steps + 1);
    out.push(pts[0]);
    for i in 0..pts.len() - 1 {
        let p0 = pts[i.saturating_sub(1)];
        let p1 = pts[i];
        let p2 = pts[i + 1];
        let p3 = pts[(i + 2).min(pts.len() - 1)];
        for s in 1..=steps {
            let t = s as f32 / steps as f32;
            out.push(catmull_rom(p0, p1, p2, p3, t));
        }
    }
    out
}

fn catmull_rom(p0: Pos2, p1: Pos2, p2: Pos2, p3: Pos2, t: f32) -> Pos2 {
    let t2 = t * t;
    let t3 = t2 * t;
    Pos2::new(
        0.5 * (2.0 * p1.x
            + (-p0.x + p2.x) * t
            + (2.0 * p0.x - 5.0 * p1.x + 4.0 * p2.x - p3.x) * t2
            + (-p0.x + 3.0 * p1.x - 3.0 * p2.x + p3.x) * t3),
        0.5 * (2.0 * p1.y
            + (-p0.y + p2.y) * t
            + (2.0 * p0.y - 5.0 * p1.y + 4.0 * p2.y - p3.y) * t2
            + (-p0.y + 3.0 * p1.y - 3.0 * p2.y + p3.y) * t3),
    )
}

/// Column coverage AA so the fill hypotenuse does not show triangle stair-steps.
fn fill_under_polyline_aa(ui: &egui::Ui, pts: &[Pos2], bottom: f32, color: Color32) {
    if pts.len() < 2 || color.a() == 0 {
        return;
    }
    let p = ui.painter();
    let ppp = ui.ctx().pixels_per_point().max(1.0);
    let dx = 1.0 / ppp;
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    for q in pts {
        min_x = min_x.min(q.x);
        max_x = max_x.max(q.x);
    }
    let mut x = min_x;
    while x < max_x {
        let x_mid = x + dx * 0.5;
        if let Some(y) = polyline_y_at(pts, x_mid) {
            if y < bottom {
                let top_pix = (y / dx).floor() * dx;
                let cover = (1.0 - (y - top_pix) / dx).clamp(0.0, 1.0);
                if cover > 0.004 {
                    let col = Color32::from_rgba_unmultiplied(
                        color.r(),
                        color.g(),
                        color.b(),
                        (color.a() as f32 * cover).round() as u8,
                    );
                    p.rect_filled(
                        Rect::from_min_max(Pos2::new(x, top_pix), Pos2::new(x + dx, top_pix + dx)),
                        0.0,
                        col,
                    );
                }
                let solid_top = top_pix + dx;
                if solid_top < bottom {
                    p.rect_filled(
                        Rect::from_min_max(Pos2::new(x, solid_top), Pos2::new(x + dx, bottom)),
                        0.0,
                        color,
                    );
                }
            }
        }
        x += dx;
    }
}

fn polyline_y_at(pts: &[Pos2], x: f32) -> Option<f32> {
    for w in pts.windows(2) {
        let a = w[0];
        let b = w[1];
        let left = a.x.min(b.x);
        let right = a.x.max(b.x);
        if x < left || x > right {
            continue;
        }
        let span = b.x - a.x;
        if span.abs() < 1e-4 {
            return Some(a.y);
        }
        let t = (x - a.x) / span;
        return Some(a.y + (b.y - a.y) * t);
    }
    None
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
    rain_soon_hint: Option<String>,
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
            display_weather_error(err).to_string(),
            Color32::from_rgba_unmultiplied(255, 180, 140, 240),
            12.0,
            LineKind::Body,
        ));
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
        if let Some(hint) = rain_soon_hint {
            lines.push((
                hint,
                Color32::from_rgba_unmultiplied(150, 205, 255, 245),
                11.0,
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
        if error.is_some() {
            lines.push((
                "刷新失败".into(),
                Color32::from_rgba_unmultiplied(255, 180, 140, 220),
                11.0,
                LineKind::Body,
            ));
        }
    }

    let margin = 10.0;
    let pad_x = 10.0;
    let pad_y = 6.0;
    let gap = 2.0;
    let max_w = (bounds.width() - margin * 2.0).clamp(88.0, 140.0);
    let wrap_w = (max_w - pad_x * 2.0).max(72.0);

    let mut row_w = 0.0_f32;
    let mut row_h = Vec::with_capacity(lines.len());
    let mut body_galleys = Vec::with_capacity(lines.len());

    for (text, color, size, kind) in &lines {
        let font = egui::FontId::proportional(*size);
        match kind {
            LineKind::Temp => {
                let num = ui.fonts(|f| f.layout_no_wrap(text.clone(), font.clone(), *color));
                let deg = ui.fonts(|f| {
                    f.layout_no_wrap("°".to_owned(), font, Color32::from_rgb(244, 247, 251))
                });
                row_w = row_w.max(num.size().x + deg.size().x * 0.55);
                row_h.push(num.size().y.max(22.0));
                body_galleys.push((Some(num), Some(deg), *color, *kind));
            }
            LineKind::Body => {
                let g = ui.fonts(|f| f.layout(text.clone(), font, *color, wrap_w));
                row_w = row_w.max(g.size().x);
                row_h.push(g.size().y.max(14.0));
                body_galleys.push((Some(g), None, *color, *kind));
            }
        }
    }

    let width = (row_w + pad_x * 2.0).min(max_w);
    let height = pad_y * 2.0 + row_h.iter().sum::<f32>() + gap * (lines.len().saturating_sub(1) as f32);

    let mut tip = Rect::from_center_size(
        Pos2::new(anchor_bottom.x, anchor_bottom.y - height * 0.5),
        Vec2::new(width, height),
    );
    if tip.min.y < bounds.min.y + margin {
        tip = tip.translate(Vec2::new(0.0, bounds.min.y + margin - tip.min.y));
    }
    if tip.max.y > bounds.max.y - margin {
        tip = tip.translate(Vec2::new(0.0, bounds.max.y - margin - tip.max.y));
    }
    if tip.max.x > bounds.max.x - margin {
        tip = tip.translate(Vec2::new(bounds.max.x - margin - tip.max.x, 0.0));
    }
    if tip.min.x < bounds.min.x + margin {
        tip = tip.translate(Vec2::new(bounds.min.x + margin - tip.min.x, 0.0));
    }
    // If the card is still wider than the hwnd, shrink in place.
    if tip.width() > bounds.width() - margin * 2.0 {
        tip.min.x = bounds.min.x + margin;
        tip.max.x = bounds.max.x - margin;
    }

    let old_clip = ui.clip_rect();
    ui.set_clip_rect(old_clip.expand(24.0).union(tip.expand(2.0)));
    let p = ui.painter();
    p.rect_filled(tip, 8.0, Color32::from_rgba_unmultiplied(12, 18, 32, 242));
    p.rect_stroke(
        tip,
        8.0,
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 50)),
    );

    let cx = tip.center().x;
    let mut y = tip.min.y + pad_y;
    for (i, (num, deg, color, kind)) in body_galleys.into_iter().enumerate() {
        let h = row_h[i];
        match kind {
            LineKind::Temp => {
                let num = num.expect("temp galley");
                let deg = deg.expect("deg galley");
                let pair_w = num.size().x + deg.size().x * 0.35;
                let num_x = cx - pair_w * 0.5;
                let num_y = y + (h - num.size().y) * 0.5;
                let deg_x = num_x + num.size().x - 1.0;
                p.galley(Pos2::new(num_x, num_y), num, color);
                p.galley(Pos2::new(deg_x, num_y - 1.0), deg, color);
            }
            LineKind::Body => {
                let g = num.expect("body galley");
                let x = if g.size().x <= wrap_w - 1.0 {
                    cx - g.size().x * 0.5
                } else {
                    tip.min.x + pad_x
                };
                let gy = y + (h - g.size().y) * 0.5;
                p.galley(Pos2::new(x, gy), g, color);
            }
        }
        y += h + gap;
    }
    ui.set_clip_rect(old_clip);
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

fn paint_button(
    ui: &mut egui::Ui,
    rect: Rect,
    label: &str,
    hovered: bool,
    opacity: f32,
    theme: PanelTheme,
) {
    let p = ui.painter();
    let bg = if hovered {
        theme.button_bg_hover
    } else {
        theme.button_bg
    };
    p.rect_filled(rect, 8.0, with_opacity(bg, opacity));
    p.rect_stroke(
        rect,
        8.0,
        Stroke::new(1.0_f32, with_opacity(theme.button_stroke, opacity)),
    );
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(12.0),
        with_opacity(theme.button_fg, opacity),
    );
}

fn inside_ball(center: Pos2, p: Pos2, margin: f32) -> bool {
    (p - center).length() <= ball_r() - margin
}

fn paint_orb(
    ui: &mut egui::Ui,
    center: Pos2,
    t: f32,
    scene: Scene,
    is_day: bool,
    temp: Option<f32>,
    drops: &[Drop],
    flakes: &[Flake],
    clouds: &CloudTextures,
    fx_alpha: f32,
    keep_clouds: bool,
    rain_soon: Option<PrecipSoon>,
) {
    // 1) Dark halo + contact shadow so the orb reads on light wallpapers.
    {
        let p = ui.painter();
        let s = ball_s();
        p.circle_filled(
            Pos2::new(center.x, center.y + 7.0 * s),
            ball_r() * 0.92,
            Color32::from_rgba_unmultiplied(12, 16, 28, 48),
        );
        p.circle_filled(
            Pos2::new(center.x, center.y + 2.0 * s),
            ball_r() * 1.08,
            Color32::from_rgba_unmultiplied(16, 22, 36, 40),
        );
        p.circle_filled(
            center,
            ball_r() * 1.04,
            Color32::from_rgba_unmultiplied(20, 28, 42, 36),
        );

        let glow = scene.glow(is_day);
        if glow.a() > 4 {
            let pulse = 0.85 + 0.15 * (t * TAU / 7.0).sin();
            for (i, (scale, a_mul)) in [(1.28, 0.28), (1.14, 0.48), (1.05, 0.7)].iter().enumerate()
            {
                let a = ((glow.a() as f32) * a_mul * pulse * (1.0 - i as f32 * 0.12)) as u8;
                p.circle_filled(
                    center,
                    ball_r() * scale,
                    Color32::from_rgba_unmultiplied(glow.r(), glow.g(), glow.b(), a),
                );
            }
        }

        if let Some(soon) = rain_soon {
            // Soft wash + thin ring. Dark hairline keeps it readable on white
            // wallpapers without a heavy colored disc.
            let pulse = 0.82 + 0.18 * (0.5 + 0.5 * (t * TAU / 2.8).sin());
            let (cr, cg, cb) = match soon.kind {
                PrecipSoonKind::Storm => (148, 128, 228),
                PrecipSoonKind::Snow => (110, 164, 214),
                PrecipSoonKind::Rain => (72, 140, 216),
            };
            p.circle_filled(
                center,
                ball_r() * 1.16,
                Color32::from_rgba_unmultiplied(cr, cg, cb, (44.0 * pulse) as u8),
            );
            p.circle_stroke(
                center,
                ball_r() * 1.06,
                Stroke::new(
                    2.2 * s,
                    Color32::from_rgba_unmultiplied(16, 28, 48, (72.0 * pulse) as u8),
                ),
            );
            p.circle_stroke(
                center,
                ball_r() * 1.06,
                Stroke::new(
                    1.4 * s,
                    Color32::from_rgba_unmultiplied(cr, cg, cb, (150.0 * pulse) as u8),
                ),
            );
        }
    }

    // 2) Interior (water under weather FX — Vue draws waves after layers but below glass)
    // Vue order: weather layers then waves. Waves sit lower so paint water first then FX on top
    // except waves should show in lower half — paint water first, then scene.
    let night_clear = !is_day && matches!(scene.orb_kind(), OrbKind::Sunny);
    if night_clear {
        paint_night_sky(ui, center, fx_alpha);
    } else {
        paint_orb_sky(ui, center, scene, is_day);
    }
    paint_water(ui, center, t, scene, is_day, temp);

    let cloud_a = if keep_clouds { 1.0 } else { fx_alpha };
    match scene.orb_kind() {
        OrbKind::Sunny => {
            if is_day {
                paint_sunny(ui, center, t, fx_alpha);
            } else {
                paint_night_clear(ui, center, t, fx_alpha);
            }
        }
        OrbKind::Cloudy => paint_clouds(ui, center, t, clouds, scene, is_day, false, cloud_a),
        OrbKind::Overcast => paint_clouds(ui, center, t, clouds, scene, is_day, true, cloud_a),
        OrbKind::Rain => {
            paint_clouds(ui, center, t, clouds, scene, is_day, true, cloud_a);
            paint_rain_drops(ui, center, drops, scene, is_day, fx_alpha);
        }
        OrbKind::Snow => {
            paint_clouds(ui, center, t, clouds, scene, is_day, false, cloud_a);
            paint_flakes(ui, center, t, flakes, scene.flake_count(), fx_alpha);
        }
        OrbKind::Storm => {
            paint_clouds(ui, center, t, clouds, scene, is_day, false, cloud_a);
            paint_rain_drops(ui, center, drops, scene, is_day, fx_alpha);
            paint_storm_fx(ui, center, t, is_day, fx_alpha);
        }
    }

    // 3) Glass highlight (sunny only) + rim
    {
        let p = ui.painter();
        let s = ball_s();
        if scene.show_highlight() && is_day {
            p.circle_filled(
                Pos2::new(center.x - ball_r() * 0.28, center.y - ball_r() * 0.38),
                ball_r() * 0.28,
                Color32::from_rgba_unmultiplied(255, 255, 255, 16),
            );
            p.circle_filled(
                Pos2::new(center.x - ball_r() * 0.34, center.y - ball_r() * 0.44),
                ball_r() * 0.10,
                Color32::from_rgba_unmultiplied(255, 255, 255, 32),
            );
            p.circle_filled(
                Pos2::new(center.x + ball_r() * 0.32, center.y + ball_r() * 0.42),
                ball_r() * 0.16,
                Color32::from_rgba_unmultiplied(255, 255, 255, 12),
            );
        }

        p.circle_stroke(
            center,
            ball_r() + 0.8 * s,
            Stroke::new(2.6 * s, Color32::from_rgba_unmultiplied(18, 24, 38, 120)),
        );
        p.circle_stroke(
            center,
            ball_r(),
            Stroke::new(
                1.6 * s,
                if is_day {
                    Color32::from_rgba_unmultiplied(255, 255, 255, 88)
                } else {
                    Color32::from_rgba_unmultiplied(200, 220, 255, 70)
                },
            ),
        );
        p.circle_stroke(
            center,
            ball_r() - 1.4 * s,
            Stroke::new(0.9 * s, Color32::from_rgba_unmultiplied(255, 255, 255, 28)),
        );
    }
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).round().clamp(0.0, 255.0) as u8
}

fn mix_rgb(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    Color32::from_rgb(
        lerp_u8(a.r(), b.r(), t),
        lerp_u8(a.g(), b.g(), t),
        lerp_u8(a.b(), b.b(), t),
    )
}

/// Shift scene water toward amber (hot) or cyan (cold). Neutral around 18°C.
fn tint_water_for_temp(base: Color32, scene: Scene, is_day: bool, temp: Option<f32>) -> Color32 {
    let Some(c) = temp else {
        return base;
    };
    let bias = ((c - 18.0) / 18.0).clamp(-1.0, 1.0);
    if bias.abs() < 0.03 {
        return base;
    }
    let (cold_s, warm_s) = match scene.orb_kind() {
        OrbKind::Sunny => (0.52, 0.70),
        OrbKind::Cloudy => (0.30, 0.36),
        OrbKind::Overcast => (0.26, 0.28),
        OrbKind::Rain => (0.20, 0.14),
        OrbKind::Snow => (0.18, 0.08),
        OrbKind::Storm => (0.16, 0.10),
    };
    let mut strength = if bias > 0.0 { warm_s } else { cold_s };
    if !is_day {
        strength *= if bias > 0.0 { 0.72 } else { 0.85 };
    }
    let warm = if is_day {
        Color32::from_rgb(0xd0, 0x82, 0x2a)
    } else {
        Color32::from_rgb(0x9a, 0x62, 0x2c)
    };
    let cold = if is_day {
        Color32::from_rgb(0x2a, 0xc8, 0xd8)
    } else {
        Color32::from_rgb(0x3a, 0x90, 0xb8)
    };
    let target = if bias > 0.0 { warm } else { cold };
    mix_rgb(base, target, bias.abs() * strength)
}

fn paint_orb_sky(ui: &mut egui::Ui, center: Pos2, scene: Scene, is_day: bool) {
    ui.painter().circle_filled(
        center,
        ball_r() - 1.4 * ball_s(),
        scene.sky(is_day),
    );
}

fn paint_water(
    ui: &mut egui::Ui,
    center: Pos2,
    t: f32,
    scene: Scene,
    is_day: bool,
    temp: Option<f32>,
) {
    let p = ui.painter();
    let back = water_polygon(center, t / 11.0 * TAU, 0.58);
    let front = water_polygon(center, -t / 8.0 * TAU + 0.8, 0.62);
    let back_a = if is_day { 228 } else { 230 };
    let front_a = if is_day { 240 } else { 242 };
    if back.len() >= 3 {
        let c = tint_water_for_temp(scene.water_a(is_day), scene, is_day, temp);
        p.add(Shape::convex_polygon(
            back,
            Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), back_a),
            Stroke::NONE,
        ));
    }
    if front.len() >= 3 {
        let c = tint_water_for_temp(scene.water_b(is_day), scene, is_day, temp);
        p.add(Shape::convex_polygon(
            front,
            Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), front_a),
            Stroke::NONE,
        ));
    }
}

fn water_polygon(center: Pos2, phase: f32, top_frac: f32) -> Vec<Pos2> {
    let mut pts = Vec::with_capacity(56);
    let y_base = center.y - ball_r() + ball_r() * 2.0 * top_frac;

    for i in 0..=28 {
        let a = i as f32 / 28.0;
        let x = center.x - ball_r() * 0.92 + a * ball_r() * 1.84;
        let wave = (a * TAU * 2.0 + phase * 2.0).sin() * (3.8 * ball_r() / ORB_R)
            + (a * TAU + phase).sin() * (2.2 * ball_r() / ORB_R);
        let pt = Pos2::new(x, y_base + wave);
        if inside_ball(center, pt, 1.5 * ball_s()) {
            pts.push(pt);
        }
    }

    for i in 0..=24 {
        let a = i as f32 / 24.0;
        let theta = 0.12 * PI + a * (PI - 0.24 * PI);
        pts.push(Pos2::new(
            center.x + ball_r() * 0.96 * theta.cos(),
            center.y + ball_r() * 0.96 * theta.sin(),
        ));
    }
    pts
}

fn paint_sunny(ui: &mut egui::Ui, center: Pos2, t: f32, opacity: f32) {
    if opacity < 0.02 {
        return;
    }
    let p = faded_painter(ui, opacity);
    let s = ball_s();
    let sun = Pos2::new(center.x, center.y - ball_r() * 0.28);
    let pulse = 0.9 + 0.1 * (t * TAU / 4.5).sin();

    let ray_rot = t / 26.0 * TAU;
    for i in 0..12 {
        let ang = ray_rot + i as f32 * (TAU / 12.0);
        let a = Pos2::new(sun.x + ang.cos() * 14.0 * s, sun.y + ang.sin() * 14.0 * s);
        let b = Pos2::new(sun.x + ang.cos() * 28.0 * s, sun.y + ang.sin() * 28.0 * s);
        if inside_ball(center, a, 4.0 * s) && inside_ball(center, b, 4.0 * s) {
            p.line_segment(
                [a, b],
                Stroke::new(2.2 * s, Color32::from_rgba_unmultiplied(255, 205, 90, 90)),
            );
        }
    }

    p.circle_filled(
        sun,
        22.0 * pulse * s,
        Color32::from_rgba_unmultiplied(255, 190, 80, 55),
    );
    p.circle_filled(sun, 16.0 * s, Color32::from_rgba_unmultiplied(255, 170, 60, 70));
    p.circle_filled(sun, 14.0 * s, Color32::from_rgb(0xff, 0xab, 0x3d));
    p.circle_filled(sun, 10.0 * s, Color32::from_rgb(0xff, 0xd8, 0x73));
    p.circle_filled(
        Pos2::new(sun.x - 2.0 * s, sun.y - 2.0 * s),
        5.5 * s,
        Color32::from_rgb(0xff, 0xf7, 0xd6),
    );

    for i in 0..5 {
        let phase = t * (0.4 + i as f32 * 0.07) + i as f32;
        let mx = center.x - ball_r() * 0.35 + (i as f32 * 17.0 * s) % (ball_r() * 0.7);
        let my = center.y + ball_r() * 0.05 + phase.sin() * 10.0 * s;
        let m = Pos2::new(mx, my);
        if inside_ball(center, m, 8.0 * s) {
            p.circle_filled(
                m,
                (1.8 + (i % 3) as f32 * 0.6) * s,
                Color32::from_rgba_unmultiplied(255, 225, 140, 180),
            );
        }
    }
}

fn night_sky() -> Color32 {
    Color32::from_rgba_unmultiplied(14, 20, 38, 240)
}

fn paint_night_sky(ui: &mut egui::Ui, center: Pos2, opacity: f32) {
    if opacity < 0.02 {
        return;
    }
    let p = faded_painter(ui, opacity);
    p.circle_filled(center, ball_r() - 1.6 * ball_s(), night_sky());
}

fn paint_night_clear(ui: &mut egui::Ui, center: Pos2, t: f32, opacity: f32) {
    if opacity < 0.02 {
        return;
    }
    let p = faded_painter(ui, opacity);
    let s = ball_s();
    const STARS: [(f32, f32, f32); 8] = [
        (-0.42, -0.38, 1.15),
        (0.38, -0.22, 0.9),
        (-0.18, -0.55, 1.35),
        (0.22, -0.48, 0.8),
        (-0.52, -0.08, 0.7),
        (0.48, -0.42, 1.05),
        (0.08, -0.18, 0.75),
        (-0.32, -0.22, 0.95),
    ];
    for (i, &(rx, ry, r)) in STARS.iter().enumerate() {
        let twinkle =
            0.40 + 0.60 * (0.5 + 0.5 * (t * (1.15 + i as f32 * 0.37) + i as f32 * 1.7).sin());
        let pos = Pos2::new(center.x + rx * ball_r(), center.y + ry * ball_r());
        if inside_ball(center, pos, 10.0 * s) {
            let a = (210.0 * twinkle) as u8;
            p.circle_filled(pos, r * s, Color32::from_rgba_unmultiplied(230, 238, 255, a));
        }
    }

    // Crescent moon in the upper half — no large halo.
    let moon = Pos2::new(center.x + ball_r() * 0.10, center.y - ball_r() * 0.34);
    let r = 12.5 * s;
    p.circle_filled(
        moon,
        r * 1.12,
        Color32::from_rgba_unmultiplied(220, 230, 255, 20),
    );
    p.circle_filled(moon, r, Color32::from_rgb(0xed, 0xf1, 0xf8));
    p.circle_filled(moon, r * 0.86, Color32::from_rgb(0xd4, 0xdc, 0xea));
    p.circle_filled(
        Pos2::new(moon.x - 3.2 * s, moon.y - 3.4 * s),
        3.0 * s,
        Color32::from_rgb(0xf7, 0xf9, 0xff),
    );
    p.circle_filled(
        Pos2::new(moon.x + 5.6 * s, moon.y - 2.0 * s),
        r * 0.94,
        night_sky(),
    );
}

fn load_skin_texture(ctx: &egui::Context, name: &str, bytes: &'static [u8]) -> TextureHandle {
    ctx.load_texture(name, load_png_rgba(bytes), TextureOptions::LINEAR)
}

fn load_png_rgba(bytes: &'static [u8]) -> ColorImage {
    match image::load_from_memory(bytes) {
        Ok(img) => {
            let img = img.into_rgba8();
            let (w, h) = (img.width() as usize, img.height() as usize);
            ColorImage::from_rgba_unmultiplied([w, h], img.as_raw())
        }
        Err(_) => ColorImage::from_rgba_unmultiplied([1, 1], &[0, 0, 0, 0]),
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
    is_day: bool,
    with_mist: bool,
    opacity: f32,
) {
    if opacity < 0.02 {
        return;
    }
    let p = faded_painter(ui, opacity);
    let s = ball_s();
    let clip = Rect::from_center_size(center, Vec2::splat(ball_r() * 2.0 - 4.0 * s));
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
            let drift = (t / period * TAU).sin() * (amp * s);
            let mist_rect = Rect::from_center_size(
                Pos2::new(center.x + drift, center.y + ry * ball_r()),
                Vec2::new(ball_r() * w_scale, ball_r() * 0.42),
            );
            p.image(
                clouds.mist.id(),
                mist_rect,
                uv,
                scene.cloud_tint(is_day, mist_alpha),
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
        let drift = (t / period * TAU).sin() * (amp * s);
        let c = Pos2::new(center.x + rx * ball_r() + drift, center.y + ry * ball_r());
        if !inside_ball(center, c, 8.0 * s) {
            continue;
        }
        let rect = Rect::from_center_size(c, Vec2::new(w * s, h * s));
        p.image(tex.id(), rect, uv, scene.cloud_tint(is_day, alpha));
    }
}

fn paint_rain_drops(
    ui: &mut egui::Ui,
    center: Pos2,
    drops: &[Drop],
    scene: Scene,
    is_day: bool,
    opacity: f32,
) {
    if opacity < 0.02 {
        return;
    }
    let p = faded_painter(ui, opacity);
    let s = ball_s();
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
            center.x + d.x * ball_r() * 0.78,
            center.y + d.y * ball_r() * 0.78,
        );
        let end = pos + slant * (d.len * s);
        if inside_ball(center, pos, 7.0 * s) && inside_ball(center, end, 5.0 * s) {
            let col = if storm {
                if is_day {
                    Color32::from_rgba_unmultiplied(185, 200, 255, d.alpha)
                } else {
                    Color32::from_rgba_unmultiplied(200, 210, 255, d.alpha)
                }
            } else if is_day {
                Color32::from_rgba_unmultiplied(200, 225, 255, d.alpha)
            } else {
                Color32::from_rgba_unmultiplied(180, 210, 255, d.alpha)
            };
            p.line_segment([pos, end], Stroke::new(d.width * s, col));
        }
    }
}

fn paint_flakes(
    ui: &mut egui::Ui,
    center: Pos2,
    t: f32,
    flakes: &[Flake],
    count: usize,
    opacity: f32,
) {
    if opacity < 0.02 {
        return;
    }
    let p = faded_painter(ui, opacity);
    let s = ball_s();
    for f in flakes.iter().take(count) {
        let sway = (t * 1.1 + f.phase).sin() * (f.sway / ORB_R);
        let pos = Pos2::new(
            center.x + (f.x + sway) * ball_r() * 0.78,
            center.y + f.y * ball_r() * 0.78,
        );
        if inside_ball(center, pos, 8.0 * s) {
            p.circle_filled(
                pos,
                f.r * s,
                Color32::from_rgba_unmultiplied(240, 248, 255, f.alpha),
            );
            p.circle_filled(
                pos,
                f.r * 0.45 * s,
                Color32::from_rgba_unmultiplied(255, 255, 255, 220),
            );
        }
    }
}

fn paint_storm_fx(ui: &mut egui::Ui, center: Pos2, t: f32, is_day: bool, opacity: f32) {
    if opacity < 0.02 {
        return;
    }
    let p = faded_painter(ui, opacity);
    let s = ball_s();
    // Periodic flash + bolt (Vue .flash / .bolt)
    let cycle = (t * 0.55).fract();
    let flash_on = cycle < 0.08 || (0.14..0.18).contains(&cycle);
    if flash_on {
        let a = if cycle < 0.08 {
            if is_day { 55 } else { 70 }
        } else if is_day {
            35
        } else {
            48
        };
        p.circle_filled(
            center,
            ball_r() * 0.92,
            Color32::from_rgba_unmultiplied(200, 190, 255, a),
        );
    }

    let bolt_on = (0.02..0.11).contains(&cycle) || (0.15..0.19).contains(&cycle);
    if bolt_on {
        let ox = center.x + 6.0 * s;
        let oy = center.y - ball_r() * 0.35;
        let pts = [
            Pos2::new(ox, oy),
            Pos2::new(ox + 6.0 * s, oy + 14.0 * s),
            Pos2::new(ox - 2.0 * s, oy + 14.0 * s),
            Pos2::new(ox + 8.0 * s, oy + 32.0 * s),
            Pos2::new(ox - 1.0 * s, oy + 22.0 * s),
            Pos2::new(ox + 4.0 * s, oy + 22.0 * s),
        ];
        // Zigzag as connected segments approximating a bolt
        let segs = [
            (pts[0], pts[1]),
            (pts[1], pts[2]),
            (pts[2], pts[3]),
        ];
        for (a, b) in segs {
            if inside_ball(center, a, 4.0 * s) && inside_ball(center, b, 4.0 * s) {
                p.line_segment(
                    [a, b],
                    Stroke::new(2.4 * s, Color32::from_rgba_unmultiplied(230, 220, 255, 230)),
                );
                p.line_segment(
                    [a, b],
                    Stroke::new(1.0 * s, Color32::from_rgba_unmultiplied(255, 255, 255, 255)),
                );
            }
        }
        let _ = (pts[4], pts[5]);
    }
}

fn build_tray_menu(
    toggle_text: &str,
) -> Result<tray_icon::menu::Menu, Box<dyn std::error::Error>> {
    use tray_icon::menu::{Menu, MenuItem, PredefinedMenuItem};

    let menu = Menu::new();
    let toggle = MenuItem::with_id("toggle", toggle_text, true, None);
    let quit_item = MenuItem::with_id("quit", "退出", true, None);
    menu.append(&toggle)?;
    menu.append(&PredefinedMenuItem::separator())?;
    menu.append(&quit_item)?;
    Ok(menu)
}

fn make_tray_icon() -> Result<tray_icon::Icon, Box<dyn std::error::Error>> {
    use tray_icon::Icon;
    const N: u32 = 32;
    let img = image::load_from_memory(include_bytes!("../assets/tray_icon.png"))?.to_rgba8();
    let resized = image::imageops::resize(&img, N, N, image::imageops::FilterType::Lanczos3);
    Ok(Icon::from_rgba(resized.into_raw(), N, N)?)
}

fn create_tray(
    window_visible: Arc<AtomicBool>,
    main_hwnd: Arc<AtomicIsize>,
    fullscreen_occluded: Arc<AtomicBool>,
    ctx: egui::Context,
) -> Result<TrayUi, Box<dyn std::error::Error>> {
    #[cfg(target_os = "windows")]
    {
        let paint_tid = win_current_thread_id();
        thread::Builder::new()
            .name("wb-tray".into())
            .spawn(move || {
                run_tray_thread(
                    paint_tid,
                    window_visible,
                    main_hwnd,
                    fullscreen_occluded,
                    ctx,
                );
            })?;
        return Ok(TrayUi { _icon: None });
    }
    #[cfg(not(target_os = "windows"))]
    {
        let tray_hmenu = Arc::new(AtomicIsize::new(0));
        let icon = build_native_tray_icon(&tray_hmenu)?;
        spawn_tray_poller(
            Arc::clone(&window_visible),
            Arc::clone(&main_hwnd),
            fullscreen_occluded,
            tray_hmenu,
            0,
            0,
            ctx.clone(),
        );
        spawn_tray_menu_wake(window_visible, main_hwnd, 0, 0, ctx);
        Ok(TrayUi { _icon: Some(icon) })
    }
}

/// Owns the tray window + its Win32 loop. `TrackPopupMenu` blocks this thread only.
#[cfg(target_os = "windows")]
fn run_tray_thread(
    paint_tid: u32,
    window_visible: Arc<AtomicBool>,
    main_hwnd: Arc<AtomicIsize>,
    fullscreen_occluded: Arc<AtomicBool>,
    ctx: egui::Context,
) {
    let tray_tid = win_current_thread_id();
    let tray_hmenu = Arc::new(AtomicIsize::new(0));
    spawn_tray_poller(
        Arc::clone(&window_visible),
        Arc::clone(&main_hwnd),
        fullscreen_occluded,
        Arc::clone(&tray_hmenu),
        tray_tid,
        paint_tid,
        ctx.clone(),
    );
    spawn_tray_menu_wake(
        window_visible,
        main_hwnd,
        tray_tid,
        paint_tid,
        ctx,
    );
    let _icon = loop {
        match build_native_tray_icon(&tray_hmenu) {
            Ok(icon) => break icon,
            Err(_) => thread::sleep(Duration::from_millis(250)),
        }
    };
    win_run_message_loop();
}

fn build_native_tray_icon(
    tray_hmenu: &AtomicIsize,
) -> Result<tray_icon::TrayIcon, Box<dyn std::error::Error>> {
    use tray_icon::menu::ContextMenu;
    use tray_icon::TrayIconBuilder;

    let icon = make_tray_icon()?;
    let menu = build_tray_menu(toggle_label(true))?;
    tray_hmenu.store(menu.hpopupmenu(), Ordering::Relaxed);

    Ok(TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("天气球")
        .with_title("天气球")
        .with_icon(icon)
        .build()?)
}

#[cfg(target_os = "windows")]
fn win_run_message_loop() {
    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }
    #[repr(C)]
    struct Msg {
        hwnd: *mut std::ffi::c_void,
        message: u32,
        wparam: usize,
        lparam: isize,
        time: u32,
        pt: Point,
    }
    extern "system" {
        fn GetMessageW(
            msg: *mut Msg,
            hwnd: *mut std::ffi::c_void,
            min: u32,
            max: u32,
        ) -> i32;
        fn TranslateMessage(msg: *const Msg) -> i32;
        fn DispatchMessageW(msg: *const Msg) -> isize;
    }
    unsafe {
        let mut msg: Msg = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// winit/DWM can still report HTCAPTION on this borderless hwnd (NVIDIA in
/// particular). A first right-click then opens the system window menu on the
/// GL thread and the process dies; a prior left-click activates the window so
/// later right-clicks arrive as client WM_RBUTTON and settings open normally.
#[cfg(target_os = "windows")]
static ORB_WNDPROC_ORIG: AtomicIsize = AtomicIsize::new(0);

#[cfg(target_os = "windows")]
fn ensure_orb_click_guard(hwnd_val: isize) {
    if hwnd_val == 0 {
        return;
    }
    const GWLP_WNDPROC: i32 = -4;
    extern "system" {
        fn GetWindowLongPtrW(h_wnd: *mut std::ffi::c_void, n_index: i32) -> isize;
        fn SetWindowLongPtrW(h_wnd: *mut std::ffi::c_void, n_index: i32, dw_new_long: isize) -> isize;
    }
    let hwnd = hwnd_val as *mut std::ffi::c_void;
    let ours = orb_wndproc as *const () as isize;
    let current = unsafe { GetWindowLongPtrW(hwnd, GWLP_WNDPROC) };
    if current == ours {
        return;
    }
    let orig = unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, ours) };
    if orig == ours {
        return;
    }
    ORB_WNDPROC_ORIG.store(orig, Ordering::Relaxed);
}

#[cfg(target_os = "windows")]
unsafe extern "system" fn orb_wndproc(
    hwnd: *mut std::ffi::c_void,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    const WM_NCHITTEST: u32 = 0x0084;
    const WM_NCRBUTTONDOWN: u32 = 0x00A4;
    const WM_NCRBUTTONUP: u32 = 0x00A5;
    const WM_NCRBUTTONDBLCLK: u32 = 0x00A6;
    const WM_CONTEXTMENU: u32 = 0x007B;
    const WM_SYSCOMMAND: u32 = 0x0112;
    const SC_KEYMENU: usize = 0xF100;
    const SC_MOUSEMENU: usize = 0xF090;
    const HTCLIENT: isize = 1;
    const HTTRANSPARENT: isize = -1;

    extern "system" {
        fn CallWindowProcW(
            prev: isize,
            h_wnd: *mut std::ffi::c_void,
            msg: u32,
            wparam: usize,
            lparam: isize,
        ) -> isize;
        fn DefWindowProcW(
            h_wnd: *mut std::ffi::c_void,
            msg: u32,
            wparam: usize,
            lparam: isize,
        ) -> isize;
    }

    let orig = ORB_WNDPROC_ORIG.load(Ordering::Relaxed);
    let call_prev = |msg: u32, wparam: usize, lparam: isize| -> isize {
        if orig == 0 {
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        } else {
            unsafe { CallWindowProcW(orig, hwnd, msg, wparam, lparam) }
        }
    };

    match msg {
        WM_NCHITTEST => {
            let hit = call_prev(msg, wparam, lparam);
            if hit == HTTRANSPARENT {
                hit
            } else if hit != HTCLIENT {
                HTCLIENT
            } else {
                hit
            }
        }
        WM_NCRBUTTONDOWN | WM_NCRBUTTONUP | WM_NCRBUTTONDBLCLK | WM_CONTEXTMENU => 0,
        WM_SYSCOMMAND if (wparam & 0xFFF0) == SC_KEYMENU || (wparam & 0xFFF0) == SC_MOUSEMENU => 0,
        _ => call_prev(msg, wparam, lparam),
    }
}

/// Keep the hwnd borderless. winit's frameless path leaves WS_CAPTION set and
/// uses WM_NCCALCSIZE to hide the non-client area; some NVIDIA DWM paths still
/// paint the caption string in the client (often the font style name "Normal").
///
/// Empty titles are worse than "WeatherBall": DWM may substitute the caption
/// font's subfamily ("Normal" / "常规"). Use an NBSP instead.
#[cfg(target_os = "windows")]
fn hide_hwnd_from_taskbar(hwnd_val: isize) -> bool {
    use std::ffi::c_void;

    if hwnd_val == 0 {
        return false;
    }

    type Hwnd = *mut c_void;
    const GWL_EXSTYLE: i32 = -20;
    const GWL_STYLE: i32 = -16;
    const WS_POPUP: isize = 0x8000_0000_u32 as isize;
    const WS_CAPTION: isize = 0x00C0_0000;
    const WS_BORDER: isize = 0x0080_0000;
    const WS_DLGFRAME: isize = 0x0040_0000;
    const WS_THICKFRAME: isize = 0x0004_0000;
    const WS_SYSMENU: isize = 0x0008_0000;
    const WS_MINIMIZEBOX: isize = 0x0002_0000;
    const WS_MAXIMIZEBOX: isize = 0x0001_0000;
    const WS_EX_APPWINDOW: isize = 0x0004_0000;
    const WS_EX_TOOLWINDOW: isize = 0x0000_0080;
    const WS_EX_DLGMODALFRAME: isize = 0x0000_0001;
    const WS_EX_WINDOWEDGE: isize = 0x0000_0100;
    const WS_EX_CLIENTEDGE: isize = 0x0000_0200;
    const WS_EX_STATICEDGE: isize = 0x0002_0000;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_FRAMECHANGED: u32 = 0x0020;
    const DWMWA_ALLOW_NCPAINT: u32 = 4;
    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    const DWMWA_BORDER_COLOR: u32 = 34;
    const DWMWA_CAPTION_COLOR: u32 = 35;
    const DWMWA_SYSTEMBACKDROP_TYPE: u32 = 38;
    /// Hide DWM caption/border (Win11). Do not use this for DWMWA_TEXT_COLOR —
    /// that resets caption text to the system default instead of hiding it.
    const DWMWA_COLOR_NONE: u32 = 0xFFFF_FFFE;
    const DWMWCP_DONOTROUND: u32 = 1;
    const DWMSBT_NONE: u32 = 1;
    const DWM_BB_ENABLE: u32 = 0x1;
    const DWM_BB_BLURREGION: u32 = 0x2;

    #[repr(C)]
    struct DwmBlurBehind {
        dw_flags: u32,
        f_enable: i32,
        h_rgn_blur: *mut c_void,
        f_transition_on_maximized: i32,
    }

    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmSetWindowAttribute(
            h_wnd: Hwnd,
            attr: u32,
            value: *const u32,
            size: u32,
        ) -> i32;
        fn DwmEnableBlurBehindWindow(h_wnd: Hwnd, p_bb: *const DwmBlurBehind) -> i32;
    }
    #[link(name = "uxtheme")]
    extern "system" {
        fn SetWindowTheme(
            h_wnd: Hwnd,
            psz_sub_app_name: *const u16,
            psz_sub_id_list: *const u16,
        ) -> i32;
    }
    #[link(name = "gdi32")]
    extern "system" {
        fn CreateRectRgn(x1: i32, y1: i32, x2: i32, y2: i32) -> *mut c_void;
        fn DeleteObject(h: *mut c_void) -> i32;
    }
    extern "system" {
        fn IsWindow(h_wnd: Hwnd) -> i32;
        fn GetWindowLongPtrW(h_wnd: Hwnd, n_index: i32) -> isize;
        fn SetWindowLongPtrW(h_wnd: Hwnd, n_index: i32, dw_new_long: isize) -> isize;
        fn GetWindowTextW(h_wnd: Hwnd, lp_string: *mut u16, n_max: i32) -> i32;
        fn SetWindowTextW(h_wnd: Hwnd, lp_string: *const u16) -> i32;
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
        let caption_bits = WS_CAPTION
            | WS_BORDER
            | WS_DLGFRAME
            | WS_THICKFRAME
            | WS_SYSMENU
            | WS_MINIMIZEBOX
            | WS_MAXIMIZEBOX;
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        let next_style = (style | WS_POPUP) & !caption_bits;
        let mut changed = false;
        if next_style != style {
            SetWindowLongPtrW(hwnd, GWL_STYLE, next_style);
            changed = true;
        }
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let next_ex = (ex | WS_EX_TOOLWINDOW)
            & !(WS_EX_APPWINDOW
                | WS_EX_DLGMODALFRAME
                | WS_EX_WINDOWEDGE
                | WS_EX_CLIENTEDGE
                | WS_EX_STATICEDGE);
        if next_ex != ex {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next_ex);
            changed = true;
        }
        let mut buf = [0u16; 32];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        let silent = n == 1 && buf[0] == 0x00A0;
        if !silent {
            let nbsp: [u16; 2] = [0x00A0, 0];
            SetWindowTextW(hwnd, nbsp.as_ptr());
            changed = true;
        }

        let none = DWMWA_COLOR_NONE;
        let no_nc: u32 = 0;
        let backdrop = DWMSBT_NONE;
        let corner = DWMWCP_DONOTROUND;
        DwmSetWindowAttribute(hwnd, DWMWA_ALLOW_NCPAINT, &no_nc, 4);
        DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &none, 4);
        DwmSetWindowAttribute(hwnd, DWMWA_CAPTION_COLOR, &none, 4);
        DwmSetWindowAttribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, &backdrop, 4);
        DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &corner, 4);

        if changed {
            let empty: [u16; 1] = [0];
            SetWindowTheme(hwnd, empty.as_ptr(), empty.as_ptr());
            // Same empty-region blur winit uses for per-pixel alpha — not the
            // full-window blur that paints a glass rectangle on Intel.
            let region = CreateRectRgn(0, 0, -1, -1);
            let bb = DwmBlurBehind {
                dw_flags: DWM_BB_ENABLE | DWM_BB_BLURREGION,
                f_enable: 1,
                h_rgn_blur: region,
                f_transition_on_maximized: 0,
            };
            DwmEnableBlurBehindWindow(hwnd, &bb);
            if !region.is_null() {
                DeleteObject(region);
            }
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

/// True when the GL context is actually running on NVIDIA (not merely installed).
#[cfg(target_os = "windows")]
fn gl_vendor_is_nvidia(frame: &eframe::Frame) -> Option<bool> {
    use glow::HasContext;
    let gl = frame.gl()?;
    let vendor = unsafe { gl.get_parameter_string(glow::VENDOR) };
    Some(vendor.to_ascii_uppercase().contains("NVIDIA"))
}

/// NVIDIA's default "layered on DXGI swapchain" present strips framebuffer alpha,
/// so the 160×520 hwnd is an opaque gray rectangle around the orb.
///
/// Fix used by SDL/snowglobe: WS_EX_LAYERED + DwmExtendFrameIntoClientArea(-1).
/// Do **not** call DwmEnableBlurBehindWindow — that paints a glass rectangle on
/// machines where GL alpha already works (Intel).
#[cfg(target_os = "windows")]
fn enable_nvidia_per_pixel_alpha(hwnd_val: isize) {
    use std::ffi::c_void;

    if hwnd_val == 0 {
        return;
    }

    type Hwnd = *mut c_void;
    const GWL_EXSTYLE: i32 = -20;
    const WS_EX_LAYERED: isize = 0x0008_0000;
    const LWA_ALPHA: u32 = 0x02;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_FRAMECHANGED: u32 = 0x0020;

    #[repr(C)]
    struct Margins {
        cx_left_width: i32,
        cx_right_width: i32,
        cy_top_height: i32,
        cy_bottom_height: i32,
    }

    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmExtendFrameIntoClientArea(h_wnd: Hwnd, p_mar_inset: *const Margins) -> i32;
    }
    extern "system" {
        fn IsWindow(h_wnd: Hwnd) -> i32;
        fn GetWindowLongPtrW(h_wnd: Hwnd, n_index: i32) -> isize;
        fn SetWindowLongPtrW(h_wnd: Hwnd, n_index: i32, dw_new_long: isize) -> isize;
        fn SetLayeredWindowAttributes(
            h_wnd: Hwnd,
            cr_key: u32,
            b_alpha: u8,
            dw_flags: u32,
        ) -> i32;
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
            return;
        }
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if ex & WS_EX_LAYERED == 0 {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | WS_EX_LAYERED);
        }
        // Negative margins = sheet of glass: DWM honors GL framebuffer alpha.
        let margins = Margins {
            cx_left_width: -1,
            cx_right_width: -1,
            cy_top_height: -1,
            cy_bottom_height: -1,
        };
        DwmExtendFrameIntoClientArea(hwnd, &margins);
        // Activate layered composition without reducing window-level opacity.
        SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);
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
}

#[cfg(target_os = "windows")]
fn disable_nvidia_per_pixel_alpha(hwnd_val: isize) {
    use std::ffi::c_void;

    if hwnd_val == 0 {
        return;
    }

    type Hwnd = *mut c_void;
    const GWL_EXSTYLE: i32 = -20;
    const WS_EX_LAYERED: isize = 0x0008_0000;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOZORDER: u32 = 0x0004;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_FRAMECHANGED: u32 = 0x0020;

    #[repr(C)]
    struct Margins {
        cx_left_width: i32,
        cx_right_width: i32,
        cy_top_height: i32,
        cy_bottom_height: i32,
    }

    #[link(name = "dwmapi")]
    extern "system" {
        fn DwmExtendFrameIntoClientArea(h_wnd: Hwnd, p_mar_inset: *const Margins) -> i32;
    }
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
            return;
        }
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if ex & WS_EX_LAYERED != 0 {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex & !WS_EX_LAYERED);
        }
        let margins = Margins {
            cx_left_width: 0,
            cx_right_width: 0,
            cy_top_height: 0,
            cy_bottom_height: 0,
        };
        DwmExtendFrameIntoClientArea(hwnd, &margins);
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
}

#[cfg(not(target_os = "windows"))]
fn hide_hwnd_from_taskbar(_hwnd: isize) -> bool {
    true
}

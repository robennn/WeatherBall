//! Open-Meteo + IP locate — mirrors `src/services/weather.ts` / geolocation.

use serde::Deserialize;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::{Intensity, Scene};

#[derive(Debug, Clone)]
pub struct HourlyPoint {
    pub hour: String,
    pub temperature: f32,
}

#[derive(Debug, Clone)]
pub struct LiveWeather {
    pub temperature: f32,
    pub feels_like: Option<f32>,
    pub humidity: Option<u8>,
    pub wind_speed: Option<f32>,
    pub description: String,
    pub city: String,
    pub scene: Scene,
    pub hourly: Vec<HourlyPoint>,
    pub fetched_at_ms: u64,
    /// Local `HH:MM` captured when this snapshot was fetched.
    pub updated_hm: String,
}

#[derive(Debug, Clone, Default)]
pub struct WeatherState {
    pub loading: bool,
    pub error: Option<String>,
    pub data: Option<LiveWeather>,
}

#[derive(Deserialize)]
struct IpApi {
    latitude: Option<f64>,
    longitude: Option<f64>,
    city: Option<String>,
    region: Option<String>,
    error: Option<bool>,
}

#[derive(Deserialize)]
struct OpenMeteo {
    current: OpenMeteoCurrent,
    hourly: Option<OpenMeteoHourly>,
}

#[derive(Deserialize)]
struct OpenMeteoCurrent {
    temperature_2m: f32,
    apparent_temperature: Option<f32>,
    relative_humidity_2m: Option<f32>,
    wind_speed_10m: Option<f32>,
    weather_code: i32,
    precipitation: Option<f32>,
}

#[derive(Deserialize)]
struct OpenMeteoHourly {
    time: Vec<String>,
    temperature_2m: Vec<f32>,
}

struct CachedLoc {
    lat: f64,
    lon: f64,
    city: String,
    at: Instant,
}

static LAST_LOC: Mutex<Option<CachedLoc>> = Mutex::new(None);
const LOC_TTL: Duration = Duration::from_secs(6 * 60 * 60);

fn lock_loc() -> std::sync::MutexGuard<'static, Option<CachedLoc>> {
    LAST_LOC.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub fn refresh_blocking() -> Result<LiveWeather, String> {
    let (lat, lon, city) = resolve_location();
    fetch_weather(lat, lon, city)
}

fn resolve_location() -> (f64, f64, String) {
    {
        let g = lock_loc();
        if let Some(c) = g.as_ref() {
            if c.at.elapsed() < LOC_TTL {
                return (c.lat, c.lon, c.city.clone());
            }
        }
    }

    match locate_from_ip() {
        Some((lat, lon, rough)) => {
            let city = reverse_geocode(lat, lon).unwrap_or(rough);
            *lock_loc() = Some(CachedLoc {
                lat,
                lon,
                city: city.clone(),
                at: Instant::now(),
            });
            (lat, lon, city)
        }
        None => {
            if let Some(c) = lock_loc().as_ref() {
                return (c.lat, c.lon, c.city.clone());
            }
            shanghai()
        }
    }
}

fn locate_from_ip() -> Option<(f64, f64, String)> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(8))
        .build();

    let resp = agent.get("https://ipapi.co/json/").call().ok()?;
    let data: IpApi = resp.into_json().ok()?;
    if data.error == Some(true) || data.latitude.is_none() || data.longitude.is_none() {
        return None;
    }
    let rough = data
        .city
        .filter(|s| !s.is_empty())
        .or(data.region)
        .unwrap_or_else(|| "当前位置".into());
    Some((data.latitude.unwrap(), data.longitude.unwrap(), rough))
}

fn shanghai() -> (f64, f64, String) {
    (31.2304, 121.4737, "上海".into())
}

/// BigDataCloud reverse geocode — prefer district/county (区县), mirrors `src/services/geocode.ts`.
fn reverse_geocode(lat: f64, lon: f64) -> Option<String> {
    #[derive(Deserialize)]
    struct Admin {
        name: Option<String>,
        #[serde(rename = "adminLevel")]
        admin_level: Option<i32>,
    }
    #[derive(Deserialize)]
    struct LocalityInfo {
        administrative: Option<Vec<Admin>>,
    }
    #[derive(Deserialize)]
    struct GeoCode {
        locality: Option<String>,
        city: Option<String>,
        #[serde(rename = "principalSubdivision")]
        principal_subdivision: Option<String>,
        #[serde(rename = "localityInfo")]
        locality_info: Option<LocalityInfo>,
    }

    let url = format!(
        "https://api.bigdatacloud.net/data/reverse-geocode-client\
         ?latitude={lat}&longitude={lon}&localityLanguage=zh-Hans"
    );

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(8))
        .build();

    let resp = agent.get(&url).call().ok()?;
    if !(200..300).contains(&resp.status()) {
        return None;
    }
    let data: GeoCode = resp.into_json().ok()?;

    let mut admins = data
        .locality_info
        .and_then(|li| li.administrative)
        .unwrap_or_default();
    admins.retain(|a| {
        a.name.as_ref().is_some_and(|n| !n.is_empty())
            && a.admin_level.is_some_and(|lv| lv >= 5)
    });
    admins.sort_by_key(|a| std::cmp::Reverse(a.admin_level.unwrap_or(0)));

    let district = admins
        .into_iter()
        .next()
        .and_then(|a| a.name)
        .filter(|s| !s.is_empty());

    let name = district
        .or(data.locality.filter(|s| !s.is_empty()))
        .or(data.city.filter(|s| !s.is_empty()))
        .or(data.principal_subdivision.filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "未知位置".into());

    let cleaned = name.replace('\u{00a0}', " ").trim().to_string();
    if cleaned.is_empty() {
        None
    } else {
        Some(cleaned)
    }
}

fn fetch_weather(lat: f64, lon: f64, city: String) -> Result<LiveWeather, String> {
    let url = format!(
        "https://api.open-meteo.com/v1/forecast?latitude={lat}&longitude={lon}\
         &current=temperature_2m,apparent_temperature,weather_code,is_day,precipitation,relative_humidity_2m,wind_speed_10m\
         &hourly=temperature_2m&forecast_hours=14&timezone=auto"
    );

    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(10))
        .build();

    let resp = agent
        .get(&url)
        .call()
        .map_err(|e| format!("天气请求失败: {e}"))?;

    if !(200..300).contains(&resp.status()) {
        return Err(format!("天气服务异常 ({})", resp.status()));
    }

    let data: OpenMeteo = resp
        .into_json()
        .map_err(|e| format!("天气解析失败: {e}"))?;

    let mapped = map_weather_code(data.current.weather_code);
    let precip = data.current.precipitation.unwrap_or(0.0);
    let intensity = refine_intensity(mapped.kind, mapped.intensity, precip);
    let scene = kind_to_scene(mapped.kind, intensity);
    let hourly = parse_hourly(data.hourly.as_ref());

    let fetched_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    Ok(LiveWeather {
        temperature: data.current.temperature_2m,
        feels_like: data.current.apparent_temperature,
        humidity: data
            .current
            .relative_humidity_2m
            .map(|h| h.round().clamp(0.0, 100.0) as u8),
        wind_speed: data.current.wind_speed_10m,
        description: mapped.description.to_string(),
        city,
        scene,
        hourly,
        fetched_at_ms,
        updated_hm: local_hm_now(),
    })
}

fn local_hm_now() -> String {
    #[cfg(target_os = "windows")]
    {
        use std::mem::MaybeUninit;
        #[repr(C)]
        struct SystemTime {
            w_year: u16,
            w_month: u16,
            w_day_of_week: u16,
            w_day: u16,
            w_hour: u16,
            w_minute: u16,
            w_second: u16,
            w_milliseconds: u16,
        }
        extern "system" {
            fn GetLocalTime(time: *mut SystemTime);
        }
        let mut st = MaybeUninit::<SystemTime>::uninit();
        unsafe {
            GetLocalTime(st.as_mut_ptr());
            let st = st.assume_init();
            format!("{:02}:{:02}", st.w_hour, st.w_minute)
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        "--:--".into()
    }
}

fn parse_hourly(hourly: Option<&OpenMeteoHourly>) -> Vec<HourlyPoint> {
    let Some(h) = hourly else {
        return Vec::new();
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    let mut start = 0usize;
    for (i, t) in h.time.iter().enumerate() {
        if let Ok(dt) = chrono_ish_parse_ms(t) {
            if dt >= now - 30 * 60 * 1000 {
                start = i;
                break;
            }
        }
    }

    let mut out = Vec::new();
    for i in start..h.time.len() {
        if out.len() >= 12 {
            break;
        }
        let Some(&temp) = h.temperature_2m.get(i) else {
            continue;
        };
        if !temp.is_finite() {
            continue;
        }
        let hour = hour_label(&h.time[i]);
        out.push(HourlyPoint {
            hour,
            temperature: temp,
        });
    }
    out
}

/// Minimal RFC3339 / Open-Meteo local time parse → unix ms (best-effort).
fn chrono_ish_parse_ms(s: &str) -> Result<i64, ()> {
    // "2024-01-01T15:00" or with offset
    let cleaned = s.replace('T', " ");
    let parts: Vec<_> = cleaned.split(|c| c == ' ' || c == '-' || c == ':' || c == '+').collect();
    if parts.len() < 5 {
        return Err(());
    }
    let y: i32 = parts[0].parse().map_err(|_| ())?;
    let mo: u32 = parts[1].parse().map_err(|_| ())?;
    let d: u32 = parts[2].parse().map_err(|_| ())?;
    let h: u32 = parts[3].parse().map_err(|_| ())?;
    let mi: u32 = parts[4].parse().map_err(|_| ())?;
    // Approximate as UTC civil time — good enough to find "now" index
    let days = days_from_civil(y, mo, d);
    Ok(((days * 86400) + (h as i64) * 3600 + (mi as i64) * 60) * 1000)
}

fn days_from_civil(y: i32, m: u32, d: u32) -> i64 {
    // Howard Hinnant algorithms
    let y = if m <= 2 { y - 1 } else { y } as i64;
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400) as u64;
    let mp = if m > 2 { m - 3 } else { m + 9 } as u64;
    let doy = (153 * mp + 2) / 5 + d as u64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    (era * 146097 + doe as i64) - 719468
}

fn hour_label(s: &str) -> String {
    // ...T15:00 or ...T15:00:00
    if let Some(t) = s.split('T').nth(1) {
        let hh = t.split(':').next().unwrap_or("--");
        return hh.chars().take(2).collect();
    }
    "--".into()
}

#[derive(Clone, Copy)]
enum Kind {
    Clear,
    Cloudy,
    Drizzle,
    Rain,
    Snow,
    Storm,
    Fog,
}

struct Mapped {
    kind: Kind,
    description: &'static str,
    intensity: Option<Intensity>,
}

fn map_weather_code(code: i32) -> Mapped {
    match code {
        0 => Mapped {
            kind: Kind::Clear,
            description: "晴朗",
            intensity: None,
        },
        1 => Mapped {
            kind: Kind::Clear,
            description: "大致晴朗",
            intensity: None,
        },
        2 => Mapped {
            kind: Kind::Cloudy,
            description: "局部多云",
            intensity: None,
        },
        3 => Mapped {
            kind: Kind::Cloudy,
            description: "阴天",
            intensity: None,
        },
        45 | 48 => Mapped {
            kind: Kind::Fog,
            description: "雾",
            intensity: None,
        },
        51 => Mapped {
            kind: Kind::Drizzle,
            description: "小毛毛雨",
            intensity: Some(Intensity::Light),
        },
        53 => Mapped {
            kind: Kind::Drizzle,
            description: "毛毛雨",
            intensity: Some(Intensity::Moderate),
        },
        55 => Mapped {
            kind: Kind::Drizzle,
            description: "浓毛毛雨",
            intensity: Some(Intensity::Heavy),
        },
        56 => Mapped {
            kind: Kind::Drizzle,
            description: "轻冻毛毛雨",
            intensity: Some(Intensity::Light),
        },
        57 => Mapped {
            kind: Kind::Drizzle,
            description: "冻毛毛雨",
            intensity: Some(Intensity::Moderate),
        },
        61 => Mapped {
            kind: Kind::Rain,
            description: "小雨",
            intensity: Some(Intensity::Light),
        },
        63 => Mapped {
            kind: Kind::Rain,
            description: "中雨",
            intensity: Some(Intensity::Moderate),
        },
        65 => Mapped {
            kind: Kind::Rain,
            description: "大雨",
            intensity: Some(Intensity::Heavy),
        },
        66 => Mapped {
            kind: Kind::Rain,
            description: "轻冻雨",
            intensity: Some(Intensity::Light),
        },
        67 => Mapped {
            kind: Kind::Rain,
            description: "冻雨",
            intensity: Some(Intensity::Heavy),
        },
        71 => Mapped {
            kind: Kind::Snow,
            description: "小雪",
            intensity: Some(Intensity::Light),
        },
        73 => Mapped {
            kind: Kind::Snow,
            description: "中雪",
            intensity: Some(Intensity::Moderate),
        },
        75 => Mapped {
            kind: Kind::Snow,
            description: "大雪",
            intensity: Some(Intensity::Heavy),
        },
        77 => Mapped {
            kind: Kind::Snow,
            description: "米雪",
            intensity: Some(Intensity::Light),
        },
        80 => Mapped {
            kind: Kind::Rain,
            description: "小阵雨",
            intensity: Some(Intensity::Light),
        },
        81 => Mapped {
            kind: Kind::Rain,
            description: "阵雨",
            intensity: Some(Intensity::Moderate),
        },
        82 => Mapped {
            kind: Kind::Rain,
            description: "强阵雨",
            intensity: Some(Intensity::Heavy),
        },
        85 => Mapped {
            kind: Kind::Snow,
            description: "小阵雪",
            intensity: Some(Intensity::Light),
        },
        86 => Mapped {
            kind: Kind::Snow,
            description: "强阵雪",
            intensity: Some(Intensity::Heavy),
        },
        95 => Mapped {
            kind: Kind::Storm,
            description: "雷阵雨",
            intensity: Some(Intensity::Moderate),
        },
        96 => Mapped {
            kind: Kind::Storm,
            description: "雷暴伴冰雹",
            intensity: Some(Intensity::Heavy),
        },
        99 => Mapped {
            kind: Kind::Storm,
            description: "强雷暴伴冰雹",
            intensity: Some(Intensity::Heavy),
        },
        _ => Mapped {
            kind: Kind::Cloudy,
            description: "多云",
            intensity: None,
        },
    }
}

fn refine_intensity(kind: Kind, intensity: Option<Intensity>, precip_mm: f32) -> Option<Intensity> {
    match kind {
        Kind::Rain | Kind::Drizzle | Kind::Snow | Kind::Storm => {
            let mut level = intensity.unwrap_or(Intensity::Moderate);
            if precip_mm >= 8.0 {
                level = Intensity::Heavy;
            } else if precip_mm >= 2.5 && level == Intensity::Light {
                level = Intensity::Moderate;
            }
            Some(level)
        }
        _ => None,
    }
}

fn kind_to_scene(kind: Kind, intensity: Option<Intensity>) -> Scene {
    let i = intensity.unwrap_or(Intensity::Moderate);
    match kind {
        Kind::Clear => Scene::Sunny,
        Kind::Cloudy => Scene::Cloudy,
        Kind::Fog => Scene::Overcast,
        Kind::Drizzle => Scene::Drizzle,
        Kind::Rain => Scene::Rain(i),
        Kind::Snow => Scene::Snow(i),
        Kind::Storm => Scene::Storm(i),
    }
}

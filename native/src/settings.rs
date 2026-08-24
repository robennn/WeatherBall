//! Persist window position and login autostart (HKCU Run).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const RUN_VALUE_NAME: &str = "WeatherBall";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppSettings {
    pub x: Option<i32>,
    pub y: Option<i32>,
    #[serde(default)]
    pub open_at_login: bool,
}

impl AppSettings {
    pub fn load() -> Self {
        let mut s = Self::load_file().unwrap_or_default();
        s.open_at_login = is_open_at_login();
        s
    }

    fn load_file() -> Option<Self> {
        let path = settings_path()?;
        let raw = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&raw).ok()
    }

    pub fn save(&self) {
        let Some(path) = settings_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn position(&self) -> Option<(i32, i32)> {
        match (self.x, self.y) {
            (Some(x), Some(y)) => Some((x, y)),
            _ => None,
        }
    }
}

pub fn save_position(x: i32, y: i32) {
    let (x, y) = crate::display::clamp_saved_pos(x, y);
    let mut s = AppSettings::load_file().unwrap_or_default();
    s.x = Some(x);
    s.y = Some(y);
    s.save();
}

pub fn set_open_at_login(enable: bool) -> bool {
    if enable {
        let _ = enable_open_at_login();
    } else {
        disable_open_at_login();
    }
    let enabled = is_open_at_login();
    let mut s = AppSettings::load_file().unwrap_or_default();
    s.open_at_login = enabled;
    s.save();
    enabled
}

fn settings_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join("WeatherBall").join("settings.json"))
}

pub fn is_open_at_login() -> bool {
    #[cfg(target_os = "windows")]
    {
        win_run_value().is_some()
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

fn enable_open_at_login() -> bool {
    #[cfg(target_os = "windows")]
    {
        let Some(exe) = current_exe_path() else {
            return false;
        };
        if !exe.is_file() {
            return false;
        }
        let value = format!("\"{}\"", exe.display());
        if !win_set_run_value(&value) {
            return false;
        }
        win_set_startup_approved(true);
        win_run_value().is_some()
    }
    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

fn disable_open_at_login() {
    #[cfg(target_os = "windows")]
    {
        win_delete_run_value();
        win_set_startup_approved(false);
        if let Some(lnk) = startup_lnk_path() {
            let _ = std::fs::remove_file(lnk);
        }
    }
}

fn current_exe_path() -> Option<PathBuf> {
    let raw = std::env::current_exe().ok()?;
    let s = raw.to_string_lossy();
    let stripped = s
        .strip_prefix(r"\\?\")
        .or_else(|| s.strip_prefix("//?/"))
        .unwrap_or(s.as_ref());
    let path = PathBuf::from(stripped);
    if path.is_file() {
        Some(path)
    } else if raw.is_file() {
        Some(raw)
    } else {
        None
    }
}

#[cfg(target_os = "windows")]
fn startup_lnk_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(
        PathBuf::from(appdata)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Startup")
            .join("WeatherBall.lnk"),
    )
}

#[cfg(target_os = "windows")]
mod winreg_run {
    use super::RUN_VALUE_NAME;
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;

    type Hkey = *mut std::ffi::c_void;
    const HKEY_CURRENT_USER: Hkey = 0x8000_0001u32 as i32 as isize as Hkey;
    const KEY_READ: u32 = 0x20019;
    const KEY_WRITE: u32 = 0x20006;
    const REG_SZ: u32 = 1;
    const REG_BINARY: u32 = 3;
    const ERROR_SUCCESS: i32 = 0;

    extern "system" {
        fn RegOpenKeyExW(
            h_key: Hkey,
            sub_key: *const u16,
            options: u32,
            sam: u32,
            result: *mut Hkey,
        ) -> i32;
        fn RegSetValueExW(
            h_key: Hkey,
            value_name: *const u16,
            reserved: u32,
            ty: u32,
            data: *const u8,
            cb_data: u32,
        ) -> i32;
        fn RegQueryValueExW(
            h_key: Hkey,
            value_name: *const u16,
            reserved: *mut u32,
            ty: *mut u32,
            data: *mut u8,
            cb_data: *mut u32,
        ) -> i32;
        fn RegDeleteValueW(h_key: Hkey, value_name: *const u16) -> i32;
        fn RegCloseKey(h_key: Hkey) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(Some(0)).collect()
    }

    const SUB_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

    fn open(sam: u32) -> Option<Hkey> {
        let sub = wide(SUB_KEY);
        let mut h = ptr::null_mut();
        let rc = unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, sub.as_ptr(), 0, sam, &mut h) };
        if rc == ERROR_SUCCESS && !h.is_null() {
            Some(h)
        } else {
            None
        }
    }

    const ACCESS: u32 = KEY_READ | KEY_WRITE;

    pub fn read() -> Option<String> {
        let h = open(KEY_READ)?;
        let name = wide(RUN_VALUE_NAME);
        let mut ty = 0u32;
        let mut cb = 0u32;
        unsafe {
            let _ = RegQueryValueExW(h, name.as_ptr(), ptr::null_mut(), &mut ty, ptr::null_mut(), &mut cb);
        }
        if cb == 0 {
            unsafe { RegCloseKey(h) };
            return None;
        }
        let mut buf = vec![0u8; cb as usize];
        let rc = unsafe {
            RegQueryValueExW(
                h,
                name.as_ptr(),
                ptr::null_mut(),
                &mut ty,
                buf.as_mut_ptr(),
                &mut cb,
            )
        };
        unsafe { RegCloseKey(h) };
        if rc != ERROR_SUCCESS || ty != REG_SZ {
            return None;
        }
        let u16s: Vec<u16> = buf
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&c| c != 0)
            .collect();
        let s = String::from_utf16_lossy(&u16s);
        if s.trim().is_empty() {
            None
        } else {
            Some(s)
        }
    }

    pub fn write(value: &str) -> bool {
        let Some(h) = open(ACCESS) else {
            return false;
        };
        let name = wide(RUN_VALUE_NAME);
        let data = wide(value);
        let bytes = data.len() * 2;
        let rc = unsafe {
            RegSetValueExW(
                h,
                name.as_ptr(),
                0,
                REG_SZ,
                data.as_ptr() as *const u8,
                bytes as u32,
            )
        };
        unsafe { RegCloseKey(h) };
        rc == ERROR_SUCCESS
    }

    pub fn delete() {
        let Some(h) = open(ACCESS) else {
            return;
        };
        let name = wide(RUN_VALUE_NAME);
        unsafe {
            let _ = RegDeleteValueW(h, name.as_ptr());
            RegCloseKey(h);
        }
    }

    const APPROVED_SUB: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

    fn open_approved() -> Option<Hkey> {
        extern "system" {
            fn RegCreateKeyExW(
                h_key: Hkey,
                sub_key: *const u16,
                reserved: u32,
                class: *mut u16,
                options: u32,
                sam: u32,
                security: *mut std::ffi::c_void,
                result: *mut Hkey,
                disposition: *mut u32,
            ) -> i32;
        }
        let sub = wide(APPROVED_SUB);
        let mut h = ptr::null_mut();
        let rc = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                sub.as_ptr(),
                0,
                ptr::null_mut(),
                0,
                ACCESS,
                ptr::null_mut(),
                &mut h,
                ptr::null_mut(),
            )
        };
        if rc == ERROR_SUCCESS && !h.is_null() {
            Some(h)
        } else {
            None
        }
    }

    pub fn set_approved(enabled: bool) {
        let Some(h) = open_approved() else {
            return;
        };
        let name = wide(RUN_VALUE_NAME);
        if enabled {
            let data: [u8; 12] = [2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
            unsafe {
                let _ = RegSetValueExW(h, name.as_ptr(), 0, REG_BINARY, data.as_ptr(), data.len() as u32);
                RegCloseKey(h);
            }
        } else {
            unsafe {
                let _ = RegDeleteValueW(h, name.as_ptr());
                RegCloseKey(h);
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn win_run_value() -> Option<String> {
    winreg_run::read()
}

#[cfg(target_os = "windows")]
fn win_set_run_value(value: &str) -> bool {
    winreg_run::write(value)
}

#[cfg(target_os = "windows")]
fn win_delete_run_value() {
    winreg_run::delete();
}

#[cfg(target_os = "windows")]
fn win_set_startup_approved(enabled: bool) {
    winreg_run::set_approved(enabled);
}

#![allow(dead_code)]
// Windows-only DD 驱动加载与封装
// 仅在 target_os = "windows" 时编译此文件
#[cfg(target_os = "windows")]

use std::ffi::c_void;
use std::mem;
use std::ffi::CString;
use winapi::ctypes::c_int;
use winapi::um::libloaderapi::{FreeLibrary, LoadLibraryA, GetProcAddress};
use winapi::um::winnt::LPCSTR;

// 导入日志宏
#[allow(unused_imports)]
use log::{debug, error, info, warn, trace};

// 重新导出 enigo::Key 以便其它文件使用（Windows 下）
pub use enigo::Key;

#[derive(Debug)]
pub struct DdDriver {
    hmodule: *mut c_void,
    dd_btn: Option<extern "C" fn(c_int) -> c_int>,
    dd_key: Option<extern "C" fn(c_int, c_int) -> c_int>,
    dd_mov: Option<extern "C" fn(c_int, c_int) -> c_int>,
    dd_whl: Option<extern "C" fn(c_int) -> c_int>,
    initialized: bool,
}

impl DdDriver {
    pub fn new() -> Option<Self> {
        unsafe {
            let dll_name = "dd32695.x64.dll";
            let current_exe = std::env::current_exe()
                .ok()
                .and_then(|exe_path| exe_path.parent().map(|p| p.to_path_buf()));

            let dll_path = if let Some(mut path) = current_exe {
                path.push(dll_name);
                path
            } else {
                std::path::PathBuf::from(dll_name)
            };

            log::debug!("尝试加载DD驱动: {:?}", dll_path);

            let c_path = match dll_path.to_str().and_then(|s| CString::new(s).ok()) {
                Some(cstr) => cstr,
                None => {
                    log::error!("无法转换DLL路径为C字符串");
                    return None;
                }
            };

            let hmodule = LoadLibraryA(c_path.as_ptr() as LPCSTR);

            if hmodule.is_null() {
                let error_code = winapi::um::errhandlingapi::GetLastError();
                log::error!(
                    "无法加载DD驱动DLL (错误代码: {})，请确保 dd32695.x64.dll 在可访问路径",
                    error_code
                );
                return None;
            }

            let dd_btn_name = CString::new("DD_btn").unwrap();
            let dd_key_name = CString::new("DD_key").unwrap();
            let dd_mov_name = CString::new("DD_mov").unwrap();
            let dd_whl_name = CString::new("DD_whl").unwrap();

            let dd_btn = GetProcAddress(hmodule as _, dd_btn_name.as_ptr() as LPCSTR);
            let dd_key = GetProcAddress(hmodule as _, dd_key_name.as_ptr() as LPCSTR);
            let dd_mov = GetProcAddress(hmodule as _, dd_mov_name.as_ptr() as LPCSTR);
            let dd_whl = GetProcAddress(hmodule as _, dd_whl_name.as_ptr() as LPCSTR);

            let dd_btn_func = if !dd_btn.is_null() {
                Some(mem::transmute(dd_btn))
            } else {
                log::error!("无法获取 DD_btn 函数地址");
                FreeLibrary(hmodule as _);
                return None;
            };

            let dd_key_func = if !dd_key.is_null() {
                Some(mem::transmute(dd_key))
            } else {
                log::error!("无法获取 DD_key 函数地址");
                FreeLibrary(hmodule as _);
                return None;
            };

            let dd_mov_func = if !dd_mov.is_null() {
                Some(mem::transmute(dd_mov))
            } else {
                log::error!("无法获取 DD_mov 函数地址");
                FreeLibrary(hmodule as _);
                return None;
            };

            let dd_whl_func = if !dd_whl.is_null() {
                Some(mem::transmute(dd_whl))
            } else {
                log::error!("无法获取 DD_whl 函数地址");
                FreeLibrary(hmodule as _);
                return None;
            };

            // 简单测试调用（可选）
            if let Some(func) = dd_btn_func {
                let _ = func(0);
            }

            Some(DdDriver {
                hmodule,
                dd_btn: dd_btn_func,
                dd_key: dd_key_func,
                dd_mov: dd_mov_func,
                dd_whl: dd_whl_func,
                initialized: true,
            })
        }
    }

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    pub fn mouse_down(&self, button: MouseButton) -> bool {
        if !self.initialized {
            return false;
        }
        let code = match button {
            MouseButton::Left => 1,
            MouseButton::Right => 4,
            MouseButton::Middle => 16,
            MouseButton::Back => 64,
            MouseButton::Forward => 128,
        };
        unsafe {
            if let Some(func) = self.dd_btn {
                func(code) == 1
            } else {
                false
            }
        }
    }

    pub fn mouse_up(&self, button: MouseButton) -> bool {
        if !self.initialized {
            return false;
        }
        let code = match button {
            MouseButton::Left => 2,
            MouseButton::Right => 8,
            MouseButton::Middle => 32,
            MouseButton::Back => 0,
            MouseButton::Forward => 0,
        };
        if code == 0 {
            return false;
        }
        unsafe {
            if let Some(func) = self.dd_btn {
                func(code) == 1
            } else {
                false
            }
        }
    }

    pub fn mouse_move_absolute(&self, x: i32, y: i32) -> bool {
        if !self.initialized {
            return false;
        }
        unsafe {
            if let Some(func) = self.dd_mov {
                func(x, y) == 1
            } else {
                false
            }
        }
    }

    pub fn mouse_scroll(&self, delta: i32) -> bool {
        if !self.initialized {
            return false;
        }
        unsafe {
            if let Some(func) = self.dd_whl {
                if delta > 0 { func(1) == 1 } else { func(2) == 1 }
            } else {
                false
            }
        }
    }

    pub fn key_down(&self, key: enigo::Key) -> bool {
        if !self.initialized {
            return false;
        }
        let dd_code = Self::key_to_dd_code(key);
        if dd_code == 0 {
            return false;
        }
        unsafe {
            if let Some(func) = self.dd_key {
                func(dd_code as c_int, 1) == 1
            } else {
                false
            }
        }
    }

    pub fn key_up(&self, key: enigo::Key) -> bool {
        if !self.initialized {
            return false;
        }
        let dd_code = Self::key_to_dd_code(key);
        if dd_code == 0 {
            return false;
        }
        unsafe {
            if let Some(func) = self.dd_key {
                func(dd_code as c_int, 2) == 1
            } else {
                false
            }
        }
    }

    pub fn key_click(&self, key: enigo::Key) -> bool {
        self.key_down(key) && self.key_up(key)
    }

    pub fn get_key_state(&self, _key: enigo::Key) -> bool {
        // DD 驱动没有提供获取按键状态的 API，这里返回 false（可在外部记录按键状态以补足）
        false
    }

    fn key_to_dd_code(key: enigo::Key) -> u32 {
        use enigo::Key::*;
        match key {
            Backspace => 214,
            Tab => 300,
            Return => 815,
            Escape => 100,
            Space => 603,
            Home => 704,
            End => 707,
            PageUp => 705,
            PageDown => 708,
            LeftArrow => 710,
            UpArrow => 709,
            RightArrow => 712,
            DownArrow => 711,
            Insert => 703,
            Delete => 706,
            CapsLock => 400,
            NumLock => 810,
            ScrollLock => 701,
            PrintScreen => 700,
            Pause => 702,
            Shift => 500,
            RightShift => 511,
            Control => 600,
            RightControl => 607,
            Alt => 604,
            RightAlt => 604,
            Meta => 601,
            RWin => 608,
            Apps => 609,
            F1 => 101,
            F2 => 102,
            F3 => 103,
            F4 => 104,
            F5 => 105,
            F6 => 106,
            F7 => 107,
            F8 => 108,
            F9 => 109,
            F10 => 110,
            F11 => 111,
            F12 => 112,
            Layout(c) => {
                match c {
                    '0' => 210,
                    '1' => 201,
                    '2' => 202,
                    '3' => 203,
                    '4' => 204,
                    '5' => 205,
                    '6' => 206,
                    '7' => 207,
                    '8' => 208,
                    '9' => 209,
                    'a' | 'A' => 401,
                    'b' | 'B' => 505,
                    'c' | 'C' => 503,
                    'd' | 'D' => 403,
                    'e' | 'E' => 303,
                    'f' | 'F' => 404,
                    'g' | 'G' => 405,
                    'h' | 'H' => 406,
                    'i' | 'I' => 308,
                    'j' | 'J' => 407,
                    'k' | 'K' => 408,
                    'l' | 'L' => 409,
                    'm' | 'M' => 507,
                    'n' | 'N' => 506,
                    'o' | 'O' => 309,
                    'p' | 'P' => 310,
                    'q' | 'Q' => 301,
                    'r' | 'R' => 304,
                    's' | 'S' => 402,
                    't' | 'T' => 305,
                    'u' | 'U' => 307,
                    'v' | 'V' => 504,
                    'w' | 'W' => 302,
                    'x' | 'X' => 502,
                    'y' | 'Y' => 306,
                    'z' | 'Z' => 501,
                    ' ' => 603,
                    '-' => 211,
                    '=' => 212,
                    '[' => 311,
                    ']' => 312,
                    ';' => 410,
                    '\'' => 411,
                    '`' => 200,
                    '\\' => 313,
                    ',' => 508,
                    '.' => 509,
                    '/' => 510,
                    _ => 0,
                }
            }
            Numpad0 => 810,
            Numpad1 => 811,
            Numpad2 => 812,
            Numpad3 => 813,
            Numpad4 => 814,
            Numpad5 => 815,
            Numpad6 => 816,
            Numpad7 => 817,
            Numpad8 => 818,
            Numpad9 => 819,
            NumpadMultiply => 820,
            NumpadAdd => 821,
            NumpadSubtract => 822,
            NumpadDecimal => 823,
            NumpadDivide => 824,
            NumpadEnter => 815,
            Multiply => 820,
            Add => 821,
            Subtract => 822,
            Decimal => 823,
            Divide => 824,
            _ => 0,
        }
    }
}

impl Drop for DdDriver {
    fn drop(&mut self) {
        unsafe {
            if !self.hmodule.is_null() {
                FreeLibrary(self.hmodule as _);
                log::info!("DD 驱动已卸载");
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

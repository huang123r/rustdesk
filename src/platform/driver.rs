// src/platform/windows/driver.rs
use std::ffi::c_void;
use std::mem;
use std::path::Path;
use std::ptr;
use winapi::ctypes::c_int;
use winapi::um::libloaderapi::{FreeLibrary, LoadLibraryA, GetProcAddress};
use winapi::um::winnt::LPCSTR;
use std::ffi::CString;

// 导入日志宏
#[allow(unused_imports)]
use log::{debug, error, info, warn, trace};

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
            // 从当前目录加载DD驱动DLL
            let dll_name = "dd32695.x64.dll";
            
            // 尝试在当前目录查找DLL
            let current_exe = std::env::current_exe()
                .ok()
                .and_then(|exe_path| exe_path.parent().map(|p| p.to_path_buf()));
            
            let dll_path = if let Some(mut path) = current_exe {
                path.push(dll_name);
                path
            } else {
                // 如果无法获取当前目录，使用相对路径
                std::path::PathBuf::from(dll_name)
            };
            
            log::debug!("尝试加载DD驱动: {:?}", dll_path);
            
            // 转换为C字符串
            let c_path = match dll_path.to_str().and_then(|s| CString::new(s).ok()) {
                Some(cstr) => cstr,
                None => {
                    log::error!("无法转换DLL路径为C字符串");
                    return None;
                }
            };
            
            // 加载DLL
            let hmodule = LoadLibraryA(c_path.as_ptr() as LPCSTR);
            
            if hmodule.is_null() {
                // 获取Windows错误信息
                let error_code = winapi::um::errhandlingapi::GetLastError();
                log::error!("无法加载DD驱动DLL (错误代码: {})，请确保dd32695.x64.dll在当前目录", error_code);
                return None;
            }
            
            // 获取函数地址
            let dd_btn_name = CString::new("DD_btn").unwrap();
            let dd_key_name = CString::new("DD_key").unwrap();
            let dd_mov_name = CString::new("DD_mov").unwrap();
            let dd_whl_name = CString::new("DD_whl").unwrap();
            
            let dd_btn = GetProcAddress(hmodule as _, dd_btn_name.as_ptr() as LPCSTR);
            let dd_key = GetProcAddress(hmodule as _, dd_key_name.as_ptr() as LPCSTR);
            let dd_mov = GetProcAddress(hmodule as _, dd_mov_name.as_ptr() as LPCSTR);
            let dd_whl = GetProcAddress(hmodule as _, dd_whl_name.as_ptr() as LPCSTR);
            
            // 转换为函数指针
            let dd_btn_func = if !dd_btn.is_null() {
                Some(mem::transmute(dd_btn))
            } else {
                log::error!("无法获取DD_btn函数地址");
                FreeLibrary(hmodule as _);
                return None;
            };
            
            let dd_key_func = if !dd_key.is_null() {
                Some(mem::transmute(dd_key))
            } else {
                log::error!("无法获取DD_key函数地址");
                FreeLibrary(hmodule as _);
                return None;
            };
            
            let dd_mov_func = if !dd_mov.is_null() {
                Some(mem::transmute(dd_mov))
            } else {
                log::error!("无法获取DD_mov函数地址");
                FreeLibrary(hmodule as _);
                return None;
            };
            
            let dd_whl_func = if !dd_whl.is_null() {
                Some(mem::transmute(dd_whl))
            } else {
                log::error!("无法获取DD_whl函数地址");
                FreeLibrary(hmodule as _);
                return None;
            };
            
            // 测试驱动
            if let Some(func) = dd_btn_func {
                let result = func(0);
                if result == 1 {
                    log::info!("DD驱动初始化成功 (从当前目录加载)");
                } else {
                    log::warn!("DD驱动测试返回异常: {}", result);
                }
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
            MouseButton::Left => 1,     // 左键按下
            MouseButton::Right => 4,    // 右键按下
            MouseButton::Middle => 16,  // 中键按下
            MouseButton::Back => 64,    // 后退键按下
            MouseButton::Forward => 128,// 前进键按下
        };
        
        unsafe {
            if let Some(func) = self.dd_btn {
                let result = func(code);
                if result != 1 {
                    log::debug!("DD驱动: 鼠标按下失败，返回码: {}", result);
                }
                result == 1
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
            MouseButton::Left => 2,     // 左键释放
            MouseButton::Right => 8,    // 右键释放
            MouseButton::Middle => 32,  // 中键释放
            MouseButton::Back => 0,     // 不支持
            MouseButton::Forward => 0,  // 不支持
        };
        
        if code == 0 {
            return false;
        }
        
        unsafe {
            if let Some(func) = self.dd_btn {
                let result = func(code);
                if result != 1 {
                    log::debug!("DD驱动: 鼠标释放失败，返回码: {}", result);
                }
                result == 1
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
                let result = func(x, y);
                if result != 1 {
                    log::debug!("DD驱动: 鼠标移动失败，坐标({}, {})，返回码: {}", x, y, result);
                }
                result == 1
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
                let result = if delta > 0 {
                    func(1)  // 向上滚动
                } else {
                    func(2)  // 向下滚动
                };
                if result != 1 {
                    log::debug!("DD驱动: 鼠标滚轮失败，delta: {}，返回码: {}", delta, result);
                }
                result == 1
            } else {
                false
            }
        }
    }
    
    pub fn key_down(&self, key: Key) -> bool {
        if !self.initialized {
            return false;
        }
        
        let dd_code = Self::key_to_dd_code(key);
        if dd_code == 0 {
            log::debug!("DD驱动: 未知的按键: {:?}", key);
            return false;
        }
        
        unsafe {
            if let Some(func) = self.dd_key {
                let result = func(dd_code as c_int, 1);  // 1表示按下
                if result != 1 {
                    log::debug!("DD驱动: 按键按下失败，键码: {}，返回码: {}", dd_code, result);
                }
                result == 1
            } else {
                false
            }
        }
    }
    
    pub fn key_up(&self, key: Key) -> bool {
        if !self.initialized {
            return false;
        }
        
        let dd_code = Self::key_to_dd_code(key);
        if dd_code == 0 {
            log::debug!("DD驱动: 未知的按键: {:?}", key);
            return false;
        }
        
        unsafe {
            if let Some(func) = self.dd_key {
                let result = func(dd_code as c_int, 2);  // 2表示释放
                if result != 1 {
                    log::debug!("DD驱动: 按键释放失败，键码: {}，返回码: {}", dd_code, result);
                }
                result == 1
            } else {
                false
            }
        }
    }
    
    pub fn key_click(&self, key: Key) -> bool {
        self.key_down(key) && self.key_up(key)
    }
    
    pub fn get_key_state(&self, key: Key) -> bool {
        // DD驱动没有提供获取按键状态的API
        // 这是一个限制，我们需要记录自己的状态
        false
    }
    
    fn key_to_dd_code(key: Key) -> u32 {
        match key {
            Key::Backspace => 214,
            Key::Tab => 300,
            Key::Return => 815,
            Key::Escape => 100,
            Key::Space => 603,
            Key::Home => 704,
            Key::End => 707,
            Key::PageUp => 705,
            Key::PageDown => 708,
            Key::LeftArrow => 710,
            Key::UpArrow => 709,
            Key::RightArrow => 712,
            Key::DownArrow => 711,
            Key::Insert => 703,
            Key::Delete => 706,
            Key::CapsLock => 400,
            Key::NumLock => 810,
            Key::ScrollLock => 701,
            Key::PrintScreen => 700,
            Key::Pause => 702,
            Key::Shift => 500,
            Key::RightShift => 511,
            Key::Control => 600,
            Key::RightControl => 607,
            Key::Alt => 604,
            Key::RightAlt => 604,
            Key::Meta => 601,      // Windows键
            Key::RWin => 608,
            Key::Apps => 609,      // 应用程序键
            Key::F1 => 101,
            Key::F2 => 102,
            Key::F3 => 103,
            Key::F4 => 104,
            Key::F5 => 105,
            Key::F6 => 106,
            Key::F7 => 107,
            Key::F8 => 108,
            Key::F9 => 109,
            Key::F10 => 110,
            Key::F11 => 111,
            Key::F12 => 112,
            Key::Layout(c) => {
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
                    ' ' => 603,  // 空格
                    '-' => 211,  // 减号
                    '=' => 212,  // 等号
                    '[' => 311,  // 左方括号
                    ']' => 312,  // 右方括号
                    ';' => 410,  // 分号
                    '\'' => 411, // 单引号
                    '`' => 200,  // 反引号
                    '\\' => 313, // 反斜杠
                    ',' => 508,  // 逗号
                    '.' => 509,  // 句点
                    '/' => 510,  // 斜杠
                    _ => 0,
                }
            }
            Key::Numpad0 => 810,
            Key::Numpad1 => 811,
            Key::Numpad2 => 812,
            Key::Numpad3 => 813,
            Key::Numpad4 => 814,
            Key::Numpad5 => 815,
            Key::Numpad6 => 816,
            Key::Numpad7 => 817,
            Key::Numpad8 => 818,
            Key::Numpad9 => 819,
            Key::NumpadMultiply => 820,
            Key::NumpadAdd => 821,
            Key::NumpadSubtract => 822,
            Key::NumpadDecimal => 823,
            Key::NumpadDivide => 824,
            Key::NumpadEnter => 815,
            Key::Multiply => 820,
            Key::Add => 821,
            Key::Subtract => 822,
            Key::Decimal => 823,
            Key::Divide => 824,
            _ => 0,
        }
    }
}

impl Drop for DdDriver {
    fn drop(&mut self) {
        unsafe {
            if !self.hmodule.is_null() {
                FreeLibrary(self.hmodule as _);
                log::info!("DD驱动已卸载");
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

// 重新导出enigo::Key以便使用
pub use enigo::Key;

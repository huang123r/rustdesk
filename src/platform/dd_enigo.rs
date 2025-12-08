// src/platform/windows/dd_enigo.rs
use super::driver::{DdDriver, Key, MouseButton};
use enigo::{KeyboardControllable, MouseControllable, Settings};
#[allow(unused_imports)]
use log::{debug, warn, error, info};

pub struct DdEnigo {
    driver: DdDriver,
    mouse_x: i32,
    mouse_y: i32,
}

impl DdEnigo {
    pub fn new() -> Option<Self> {
        DdDriver::new().map(|driver| {
            DdEnigo {
                driver,
                mouse_x: 0,
                mouse_y: 0,
            }
        })
    }
    
    pub fn is_available(&self) -> bool {
        self.driver.is_initialized()
    }
    
    // 添加 reset_flag 和 add_flag 方法以兼容 enigo 接口
    pub fn reset_flag(&mut self) {
        // DD驱动没有标志位概念，这个方法只是为了兼容接口
    }
    
    pub fn add_flag(&mut self, _key: &Key) {
        // DD驱动没有标志位概念，这个方法只是为了兼容接口
    }
    
    pub fn set_ignore_flags(&mut self, _ignore: bool) {
        // DD驱动没有标志位概念，这个方法只是为了兼容接口
    }
    
    pub fn tfc_clear_remapped(&mut self) {
        // DD驱动不支持键盘重映射清除
    }
    
    pub fn set_custom_keyboard(&mut self, _keyboard: Box<dyn KeyboardControllable>) {
        // DD驱动已经使用了自定义键盘，这个方法不需要实现
    }
    
    pub fn set_custom_mouse(&mut self, _mouse: Box<dyn MouseControllable>) {
        // DD驱动已经使用了自定义鼠标，这个方法不需要实现
    }
    
    pub fn get_custom_mouse(&mut self) -> Option<&mut dyn MouseControllable> {
        None
    }
}

impl KeyboardControllable for DdEnigo {
    fn key_down(&mut self, key: Key) -> Result<(), enigo::ErrorKind> {
        if self.driver.key_down(key) {
            Ok(())
        } else {
            Err(enigo::ErrorKind::InvalidInput)
        }
    }
    
    fn key_up(&mut self, key: Key) -> Result<(), enigo::ErrorKind> {
        if self.driver.key_up(key) {
            Ok(())
        } else {
            Err(enigo::ErrorKind::InvalidInput)
        }
    }
    
    fn key_click(&mut self, key: Key) -> Result<(), enigo::ErrorKind> {
        if self.driver.key_click(key) {
            Ok(())
        } else {
            Err(enigo::ErrorKind::InvalidInput)
        }
    }
    
    fn key_sequence(&mut self, sequence: &str) -> Result<(), enigo::ErrorKind> {
        // DD驱动不支持直接输入字符串，需要逐个字符输入
        for ch in sequence.chars() {
            let key = Key::Layout(ch);
            if !self.driver.key_click(key) {
                return Err(enigo::ErrorKind::InvalidInput);
            }
        }
        Ok(())
    }
    
    fn get_key_state(&mut self, key: Key) -> bool {
        self.driver.get_key_state(key)
    }
    
    // 实现 enigo 的其他方法
    fn raw_keycode(&mut self, keycode: u16, down: bool) -> Result<(), enigo::ErrorKind> {
        // DD驱动不支持原始键码
        Err(enigo::ErrorKind::InvalidInput)
    }
}

impl MouseControllable for DdEnigo {
    fn mouse_move_to(&mut self, x: i32, y: i32) -> Result<(), enigo::ErrorKind> {
        if self.driver.mouse_move_absolute(x, y) {
            self.mouse_x = x;
            self.mouse_y = y;
            Ok(())
        } else {
            Err(enigo::ErrorKind::InvalidInput)
        }
    }
    
    fn mouse_move_relative(&mut self, x: i32, y: i32) -> Result<(), enigo::ErrorKind> {
        let new_x = self.mouse_x + x;
        let new_y = self.mouse_y + y;
        if self.driver.mouse_move_absolute(new_x, new_y) {
            self.mouse_x = new_x;
            self.mouse_y = new_y;
            Ok(())
        } else {
            Err(enigo::ErrorKind::InvalidInput)
        }
    }
    
    fn mouse_down(&mut self, button: enigo::MouseButton) -> Result<(), enigo::ErrorKind> {
        let dd_button = match button {
            enigo::MouseButton::Left => MouseButton::Left,
            enigo::MouseButton::Right => MouseButton::Right,
            enigo::MouseButton::Middle => MouseButton::Middle,
            enigo::MouseButton::Back => MouseButton::Back,
            enigo::MouseButton::Forward => MouseButton::Forward,
        };
        
        if self.driver.mouse_down(dd_button) {
            Ok(())
        } else {
            Err(enigo::ErrorKind::InvalidInput)
        }
    }
    
    fn mouse_up(&mut self, button: enigo::MouseButton) -> Result<(), enigo::ErrorKind> {
        let dd_button = match button {
            enigo::MouseButton::Left => MouseButton::Left,
            enigo::MouseButton::Right => MouseButton::Right,
            enigo::MouseButton::Middle => MouseButton::Middle,
            enigo::MouseButton::Back => MouseButton::Back,
            enigo::MouseButton::Forward => MouseButton::Forward,
        };
        
        if self.driver.mouse_up(dd_button) {
            Ok(())
        } else {
            Err(enigo::ErrorKind::InvalidInput)
        }
    }
    
    fn mouse_scroll_x(&mut self, length: i32) -> Result<(), enigo::ErrorKind> {
        // DD驱动可能不支持水平滚动，这里暂时不实现
        warn!("DD驱动不支持水平滚轮滚动");
        Err(enigo::ErrorKind::InvalidInput)
    }
    
    fn mouse_scroll_y(&mut self, length: i32) -> Result<(), enigo::ErrorKind> {
        if self.driver.mouse_scroll(length) {
            Ok(())
        } else {
            Err(enigo::ErrorKind::InvalidInput)
        }
    }
    
    fn mouse_click(&mut self, button: enigo::MouseButton) -> Result<(), enigo::ErrorKind> {
        let dd_button = match button {
            enigo::MouseButton::Left => MouseButton::Left,
            enigo::MouseButton::Right => MouseButton::Right,
            enigo::MouseButton::Middle => MouseButton::Middle,
            enigo::MouseButton::Back => MouseButton::Back,
            enigo::MouseButton::Forward => MouseButton::Forward,
        };
        
        if self.driver.mouse_down(dd_button) && self.driver.mouse_up(dd_button) {
            Ok(())
        } else {
            Err(enigo::ErrorKind::InvalidInput)
        }
    }
    
    // 需要实现 main_display_size 方法
    fn main_display_size(&self) -> Result<(i32, i32), enigo::ErrorKind> {
        // 获取主显示器大小
        use winapi::um::winuser::GetSystemMetrics;
        use winapi::um::winuser::SM_CXSCREEN;
        use winapi::um::winuser::SM_CYSCREEN;
        
        unsafe {
            let width = GetSystemMetrics(SM_CXSCREEN);
            let height = GetSystemMetrics(SM_CYSCREEN);
            Ok((width, height))
        }
    }
    
    // 实现 macOS 特定的 mouse_scroll_x 和 mouse_scroll_y 方法
    #[cfg(target_os = "macos")]
    fn mouse_scroll_x(&mut self, length: i32, _is_track_pad: bool) -> Result<(), enigo::ErrorKind> {
        // DD驱动不支持水平滚动
        Err(enigo::ErrorKind::InvalidInput)
    }
    
    #[cfg(target_os = "macos")]
    fn mouse_scroll_y(&mut self, length: i32, _is_track_pad: bool) -> Result<(), enigo::ErrorKind> {
        if self.driver.mouse_scroll(length) {
            Ok(())
        } else {
            Err(enigo::ErrorKind::InvalidInput)
        }
    }
}

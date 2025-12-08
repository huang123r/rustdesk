#![allow(dead_code)]
// Windows-only: 将 DdDriver 适配为 enigo 的接口（KeyboardControllable / MouseControllable）
#[cfg(target_os = "windows")]

use super::driver::{DdDriver, MouseButton};
use enigo::{KeyboardControllable, MouseControllable};
#[allow(unused_imports)]
use log::{debug, warn, error, info};

pub struct DdEnigo {
    driver: DdDriver,
    mouse_x: i32,
    mouse_y: i32,
}

impl DdEnigo {
    pub fn new() -> Option<Self> {
        DdDriver::new().map(|driver| DdEnigo {
            driver,
            mouse_x: 0,
            mouse_y: 0,
        })
    }

    pub fn is_available(&self) -> bool {
        self.driver.is_initialized()
    }

    // 兼容 Enigo 的接口（占位，DD 驱动无相应语义）
    pub fn reset_flag(&mut self) {}
    pub fn add_flag(&mut self, _key: &enigo::Key) {}
    pub fn set_ignore_flags(&mut self, _ignore: bool) {}
    pub fn tfc_clear_remapped(&mut self) {}
    pub fn set_custom_keyboard(&mut self, _keyboard: Box<dyn KeyboardControllable>) {}
    pub fn set_custom_mouse(&mut self, _mouse: Box<dyn MouseControllable>) {}
    pub fn get_custom_mouse(&mut self) -> Option<&mut dyn MouseControllable> { None }
}

impl KeyboardControllable for DdEnigo {
    fn key_down(&mut self, key: enigo::Key) -> Result<(), enigo::ErrorKind> {
        if self.driver.key_down(key) {
            Ok(())
        } else {
            Err(enigo::ErrorKind::InvalidInput)
        }
    }

    fn key_up(&mut self, key: enigo::Key) -> Result<(), enigo::ErrorKind> {
        if self.driver.key_up(key) {
            Ok(())
        } else {
            Err(enigo::ErrorKind::InvalidInput)
        }
    }

    fn key_click(&mut self, key: enigo::Key) -> Result<(), enigo::ErrorKind> {
        if self.driver.key_click(key) {
            Ok(())
        } else {
            Err(enigo::ErrorKind::InvalidInput)
        }
    }

    fn key_sequence(&mut self, sequence: &str) -> Result<(), enigo::ErrorKind> {
        for ch in sequence.chars() {
            let key = enigo::Key::Layout(ch);
            if !self.driver.key_click(key) {
                return Err(enigo::ErrorKind::InvalidInput);
            }
        }
        Ok(())
    }

    fn get_key_state(&mut self, key: enigo::Key) -> bool {
        self.driver.get_key_state(key)
    }

    fn raw_keycode(&mut self, _keycode: u16, _down: bool) -> Result<(), enigo::ErrorKind> {
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

    fn mouse_scroll_x(&mut self, _length: i32) -> Result<(), enigo::ErrorKind> {
        warn!("DD 驱动不支持水平滚动");
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

    fn main_display_size(&self) -> Result<(i32, i32), enigo::ErrorKind> {
        use winapi::um::winuser::GetSystemMetrics;
        use winapi::um::winuser::{SM_CXSCREEN, SM_CYSCREEN};
        unsafe {
            let w = GetSystemMetrics(SM_CXSCREEN);
            let h = GetSystemMetrics(SM_CYSCREEN);
            Ok((w, h))
        }
    }
}

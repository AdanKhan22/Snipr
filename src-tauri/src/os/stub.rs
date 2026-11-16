//! Placeholder backend for platforms that are not implemented yet.

use super::KeyEvent;
use std::sync::mpsc::Sender;

pub const SUPPORTED: bool = false;

pub fn start_listener(_tx: Sender<KeyEvent>) {}
pub fn foreground_handle() -> isize {
    0
}
pub fn focus_window(_h: isize) {}
pub fn app_name_for(_h: isize) -> Option<String> {
    None
}
pub fn wait_modifiers_released() {}
pub fn send_backspaces(_n: usize) {}
pub fn send_text(_s: &str) {}
pub fn send_combo(_spec: &str) {}
pub fn send_paste() {}
pub fn send_left(_n: usize) {}

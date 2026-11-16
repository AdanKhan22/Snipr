//! Thin OS layer: global key listening, keystroke injection, focus and active-app queries.
//! Only Windows is implemented; `stub` keeps the crate compiling elsewhere so the
//! macOS/Linux backends can be added behind the same function set.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEvent {
    /// A printable character was typed.
    Char(char),
    Backspace,
    /// Anything that breaks the typing flow: navigation keys, shortcuts, mouse clicks.
    Reset,
}

#[cfg(windows)]
mod win;
#[cfg(windows)]
pub use win::*;

#[cfg(not(windows))]
mod stub;
#[cfg(not(windows))]
pub use stub::*;

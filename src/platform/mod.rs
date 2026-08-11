#[cfg(windows)]
mod win32;
#[cfg(windows)]
pub use win32::{beep, set_clipboard, Adapter};

#[cfg(not(windows))]
mod stub;
#[cfg(not(windows))]
pub use stub::{beep, set_clipboard, Adapter};

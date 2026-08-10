#[cfg(windows)]
mod win32;
#[cfg(windows)]
pub use win32::{beep, Adapter};

#[cfg(not(windows))]
mod stub;
#[cfg(not(windows))]
pub use stub::{beep, Adapter};

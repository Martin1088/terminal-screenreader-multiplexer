use accesskit::{ActionHandler, TreeUpdate};
use accesskit_windows::{Adapter as PlatformAdapter, HWND};
use windows::Win32::System::Console::GetConsoleWindow;

/// Wraps `accesskit_windows::Adapter` on the console window's `HWND`.
///
/// KNOWN GAP: `GetConsoleWindow` returns a handle owned by conhost.exe (or
/// Windows Terminal), a different process from this one. Windows only ever
/// delivers `WM_GETOBJECT` to the window procedure that owning process
/// registered — subclassing another process's window via
/// `SetWindowLongPtr`/`SetWindowSubclass` is documented as same-process
/// only. So nothing in this process's control flow ever calls
/// `PlatformAdapter::handle_wm_getobject` for this `hwnd`, and no screen
/// reader will see this tree yet. Making that reachable means owning a real
/// `HWND` (a window this process creates, with its own `WndProc`) instead of
/// piggybacking on the console window — a bigger design decision to make
/// deliberately, not papered over here.
pub struct Adapter {
    inner: PlatformAdapter,
}

impl Adapter {
    pub fn new(action_handler: impl ActionHandler + Send + 'static) -> Self {
        let hwnd: HWND = unsafe { GetConsoleWindow() };
        Self {
            inner: PlatformAdapter::new(hwnd, true, action_handler),
        }
    }

    pub fn update_if_active(&mut self, update_factory: impl FnOnce() -> TreeUpdate) {
        if let Some(events) = self.inner.update_if_active(update_factory) {
            events.raise();
        }
    }
}

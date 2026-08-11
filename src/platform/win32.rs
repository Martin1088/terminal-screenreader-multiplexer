use crate::{AppEvent, Key};
use accesskit::{ActionHandler, ActivationHandler, Node, NodeId, Role, Tree, TreeId, TreeUpdate};
use accesskit_windows::Adapter as PlatformAdapter;
use std::cell::RefCell;
use std::mem::size_of;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use windows::core::w;
use windows::Win32::Foundation::{HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Console::GetConsoleWindow;
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Diagnostics::Debug::Beep;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VIRTUAL_KEY, VK_DOWN, VK_ESCAPE, VK_F1, VK_F2, VK_M, VK_N, VK_P, VK_RETURN, VK_SPACE, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowLongPtrW, PostMessageW, PostQuitMessage, RegisterClassExW, SetForegroundWindow,
    SetWindowLongPtrW, ShowWindow, TranslateMessage, CW_USEDEFAULT, GWLP_USERDATA, MSG, SW_SHOW,
    WINDOW_EX_STYLE, WM_APP, WM_CLOSE, WM_GETOBJECT, WM_KEYDOWN, WM_KILLFOCUS, WM_SETFOCUS,
    WNDCLASSEXW, WS_POPUP,
};

const WM_APP_UPDATE: u32 = WM_APP + 1;
const WM_APP_SHUTDOWN: u32 = WM_APP + 2;

fn placeholder_tree() -> TreeUpdate {
    let root = NodeId(0);
    TreeUpdate {
        nodes: vec![(root, Node::new(Role::Window))],
        tree: Some(Tree::new(root)),
        tree_id: TreeId::ROOT,
        focus: root,
    }
}

/// Hands AccessKit the most recently pushed tree the first time a screen
/// reader asks for one (`WM_GETOBJECT`), regardless of which
/// `update_if_active` call happened to produce it.
struct LastTree(Arc<Mutex<TreeUpdate>>);

impl ActivationHandler for LastTree {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        Some(self.0.lock().unwrap().clone())
    }
}

struct WindowState {
    adapter: RefCell<PlatformAdapter>,
    activation: RefCell<LastTree>,
    update_rx: Receiver<Box<dyn FnOnce() -> TreeUpdate + Send>>,
    event_tx: Sender<AppEvent>,
}

fn window_state(hwnd: HWND) -> Option<&'static WindowState> {
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const WindowState;
    unsafe { ptr.as_ref() }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        // Converting the result to an LRESULT calls into UIA, which can
        // synchronously send a nested WM_GETOBJECT back into this WndProc
        // (JAWS does this on ShowWindow). Same for QueuedEvents::raise below.
        // No RefCell borrow may be live across either call, or the nested
        // entry panics with "RefCell already borrowed" and aborts.
        WM_GETOBJECT => {
            if let Some(state) = window_state(hwnd) {
                let result = {
                    let mut adapter = state.adapter.borrow_mut();
                    let mut activation = state.activation.borrow_mut();
                    adapter.handle_wm_getobject(wparam, lparam, &mut *activation)
                };
                if let Some(result) = result {
                    return result.into();
                }
            }
        }
        WM_KEYDOWN => {
            if let Some(state) = window_state(hwnd) {
                let key = match VIRTUAL_KEY(wparam.0 as u16) {
                    VK_UP => Some(Key::Up),
                    VK_DOWN => Some(Key::Down),
                    VK_ESCAPE | VK_F2 => Some(Key::Exit),
                    VK_F1 => Some(Key::Prefix),
                    VK_SPACE => Some(Key::StartSelection),
                    VK_RETURN => Some(Key::Copy),
                    VK_M => Some(Key::ToggleBookmark),
                    VK_N => Some(Key::NextBookmark),
                    VK_P => Some(Key::PrevBookmark),
                    _ => None,
                };
                if let Some(key) = key {
                    let _ = state.event_tx.send(AppEvent::Key(key));
                    return LRESULT(0);
                }
            }
        }
        WM_SETFOCUS | WM_KILLFOCUS => {
            if let Some(state) = window_state(hwnd) {
                let events = state
                    .adapter
                    .borrow_mut()
                    .update_window_focus_state(msg == WM_SETFOCUS);
                if let Some(events) = events {
                    events.raise();
                }
                return LRESULT(0);
            }
        }
        WM_CLOSE => {
            // Closing the bridge window means leaving copy-mode; the app
            // drives the actual teardown so state stays consistent.
            if let Some(state) = window_state(hwnd) {
                let _ = state.event_tx.send(AppEvent::Key(Key::Exit));
            }
            return LRESULT(0);
        }
        WM_APP_UPDATE => {
            if let Some(state) = window_state(hwnd) {
                while let Ok(factory) = state.update_rx.try_recv() {
                    let update = factory();
                    *state.activation.borrow().0.lock().unwrap() = update.clone();
                    let events = state.adapter.borrow_mut().update_if_active(|| update);
                    if let Some(events) = events {
                        events.raise();
                    }
                }
            }
            return LRESULT(0);
        }
        WM_APP_SHUTDOWN => {
            unsafe { PostQuitMessage(0) };
            return LRESULT(0);
        }
        _ => {}
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

fn create_window() -> HWND {
    let instance: HINSTANCE = unsafe { GetModuleHandleW(None) }
        .expect("GetModuleHandleW failed")
        .into();
    let class_name = w!("TerminalScreenReaderMultiplexerA11yWindow");

    let wc = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(wndproc),
        hInstance: instance,
        lpszClassName: class_name,
        ..Default::default()
    };
    let atom = unsafe { RegisterClassExW(&wc) };
    assert_ne!(atom, 0, "RegisterClassExW failed");

    // Borderless and 2x2 px: the window only exists to hold keyboard focus
    // and answer UIA queries, so it should not visually cover the terminal.
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class_name,
            w!("Copy-Mode"),
            WS_POPUP,
            0,
            0,
            2,
            2,
            None,
            None,
            Some(instance),
            None,
        )
    }
    .expect("CreateWindowExW failed")
}

/// Owns a small top-level window created — and message-pumped — by *this*
/// process, so `accesskit_windows` has a real `WndProc` of ours to intercept
/// `WM_GETOBJECT` on. (`GetConsoleWindow`'s `HWND` belongs to conhost.exe /
/// Windows Terminal; Windows never delivers `WM_GETOBJECT` for it to us, and
/// UIA calls against it deadlocked this process before it ran a message pump.)
///
/// The window is shown and takes the foreground while copy-mode is active:
/// that's what makes screen readers query *our* AccessKit tree (with its text
/// selection and routing-key handling) instead of reading the console
/// natively. Copy-mode keys pressed while it has focus (`Up`/`Down`/`Esc`/
/// `F2`, plus the `F1` prefix and its command keys `m`/`n`/`p`) are forwarded
/// to the app as `AppEvent::Key`; everything else is ignored. On drop,
/// foreground goes back to the console window.
pub struct Adapter {
    hwnd: HWND,
    update_tx: Sender<Box<dyn FnOnce() -> TreeUpdate + Send>>,
    thread: Option<JoinHandle<()>>,
}

impl Adapter {
    pub fn new(
        action_handler: impl ActionHandler + Send + 'static,
        event_tx: Sender<AppEvent>,
    ) -> Self {
        let (update_tx, update_rx) = channel();
        let (ready_tx, ready_rx) = channel();

        let thread = std::thread::spawn(move || {
            let hwnd = create_window();

            let state = Box::new(WindowState {
                adapter: RefCell::new(PlatformAdapter::new(hwnd, false, action_handler)),
                activation: RefCell::new(LastTree(Arc::new(Mutex::new(placeholder_tree())))),
                update_rx,
                event_tx,
            });
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
            }

            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = SetForegroundWindow(hwnd);
            }

            // HWND wraps a raw pointer and so isn't Send; the handle value
            // itself is process-global, so it crosses as an integer.
            ready_tx
                .send(hwnd.0 as isize)
                .expect("adapter constructor dropped its ready channel");

            let mut msg = MSG::default();
            loop {
                let ret = unsafe { GetMessageW(&mut msg, None, 0, 0) };
                if ret.0 <= 0 {
                    break; // WM_QUIT (0) or an error (-1)
                }
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }

            let state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut WindowState;
            drop(unsafe { Box::from_raw(state_ptr) });
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
        });

        let hwnd = HWND(
            ready_rx
                .recv()
                .expect("accessibility window thread failed to start") as *mut core::ffi::c_void,
        );

        Self {
            hwnd,
            update_tx,
            thread: Some(thread),
        }
    }

    pub fn update_if_active(&mut self, update_factory: impl FnOnce() -> TreeUpdate + Send + 'static) {
        if self.update_tx.send(Box::new(update_factory)).is_ok() {
            unsafe {
                let _ = PostMessageW(Some(self.hwnd), WM_APP_UPDATE, WPARAM(0), LPARAM(0));
            }
        }
    }
}

/// Blocking beep (up to `duration_ms`); only ever call from a dedicated
/// audio thread (see `crate::tones::Tones`), never from the render loop or
/// the bridge window's message pump.
pub fn beep(freq_hz: u32, duration_ms: u32) {
    unsafe {
        let _ = Beep(freq_hz, duration_ms);
    }
}

/// Puts `text` on the Windows clipboard as CF_UNICODETEXT. Returns false if
/// the clipboard is unavailable (some other app holds it open).
pub fn set_clipboard(text: &str) -> bool {
    let utf16: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        if OpenClipboard(None).is_err() {
            return false;
        }
        let ok = (|| {
            EmptyClipboard().ok()?;
            let hmem = GlobalAlloc(GMEM_MOVEABLE, utf16.len() * 2).ok()?;
            let ptr = GlobalLock(hmem) as *mut u16;
            if ptr.is_null() {
                return None;
            }
            std::ptr::copy_nonoverlapping(utf16.as_ptr(), ptr, utf16.len());
            let _ = GlobalUnlock(hmem);
            // Nach Erfolg gehört der Speicher dem System, nicht mehr uns.
            SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(hmem.0))).ok()?;
            Some(())
        })()
        .is_some();
        let _ = CloseClipboard();
        ok
    }
}

impl Drop for Adapter {
    fn drop(&mut self) {
        unsafe {
            let console = GetConsoleWindow();
            if !console.is_invalid() {
                let _ = SetForegroundWindow(console);
            }
            let _ = PostMessageW(Some(self.hwnd), WM_APP_SHUTDOWN, WPARAM(0), LPARAM(0));
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

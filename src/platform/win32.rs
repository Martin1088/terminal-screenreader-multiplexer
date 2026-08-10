use accesskit::{ActionHandler, ActivationHandler, Node, NodeId, Role, Tree, TreeId, TreeUpdate};
use accesskit_windows::Adapter as PlatformAdapter;
use std::cell::RefCell;
use std::mem::size_of;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use windows::core::w;
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowLongPtrW, PostMessageW, PostQuitMessage, RegisterClassExW, SetWindowLongPtrW,
    TranslateMessage, CW_USEDEFAULT, GWLP_USERDATA, MSG, WINDOW_EX_STYLE, WM_APP, WM_GETOBJECT,
    WNDCLASSEXW, WS_OVERLAPPEDWINDOW,
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
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_GETOBJECT => {
            let state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const WindowState;
            if let Some(state) = unsafe { state_ptr.as_ref() } {
                let mut adapter = state.adapter.borrow_mut();
                let mut activation = state.activation.borrow_mut();
                if let Some(result) = adapter.handle_wm_getobject(wparam, lparam, &mut *activation) {
                    return result.into();
                }
            }
        }
        WM_APP_UPDATE => {
            let state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const WindowState;
            if let Some(state) = unsafe { state_ptr.as_ref() } {
                while let Ok(factory) = state.update_rx.try_recv() {
                    let update = factory();
                    *state.activation.borrow().0.lock().unwrap() = update.clone();
                    if let Some(events) = state.adapter.borrow_mut().update_if_active(|| update) {
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

    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class_name,
            w!("Accessibility Bridge"),
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        )
    }
    .expect("CreateWindowExW failed")
}

/// Owns a hidden top-level window created — and message-pumped — by *this*
/// process, purely so `accesskit_windows` has a real `WndProc` of ours to
/// intercept `WM_GETOBJECT` on.
///
/// `GetConsoleWindow` (the previous approach) returns an `HWND` owned by
/// conhost.exe/Windows Terminal — a different process. Windows only ever
/// delivers `WM_GETOBJECT` to the window procedure the *owning* process
/// registered, and Win32/UIA calls that expect this thread to service that
/// foreign window's message queue can deadlock it outright (which is what
/// froze the app: the adapter blocked waiting on a message pump that never
/// ran, since the render loop never called `GetMessage`/`DispatchMessage`).
/// Owning our own window, with our own dedicated `GetMessageW` loop on a
/// separate thread, avoids both problems.
///
/// KNOWN GAP: this window is never shown or focused, so nothing yet makes
/// a screen reader actually query it instead of the console window it's
/// already reading natively. That's a separate, still-open problem —
/// wiring up real AT discovery means either taking over focus/foreground
/// state explicitly or rendering our own visible surface instead of
/// relying on conhost, neither of which this change attempts.
pub struct Adapter {
    hwnd: HWND,
    update_tx: Sender<Box<dyn FnOnce() -> TreeUpdate + Send>>,
    thread: Option<JoinHandle<()>>,
}

impl Adapter {
    pub fn new(action_handler: impl ActionHandler + Send + 'static) -> Self {
        let (update_tx, update_rx) = channel();
        let (ready_tx, ready_rx) = channel();

        let thread = std::thread::spawn(move || {
            let hwnd = create_window();

            let state = Box::new(WindowState {
                adapter: RefCell::new(PlatformAdapter::new(hwnd, false, action_handler)),
                activation: RefCell::new(LastTree(Arc::new(Mutex::new(placeholder_tree())))),
                update_rx,
            });
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(state) as isize);
            }

            ready_tx
                .send(hwnd)
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

        let hwnd = ready_rx
            .recv()
            .expect("accessibility window thread failed to start");

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

impl Drop for Adapter {
    fn drop(&mut self) {
        unsafe {
            let _ = PostMessageW(Some(self.hwnd), WM_APP_SHUTDOWN, WPARAM(0), LPARAM(0));
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

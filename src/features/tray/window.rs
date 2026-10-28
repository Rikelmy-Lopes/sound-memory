//! Hidden Win32 window that owns the notification icon and its message loop.

use std::cell::RefCell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;

use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{Shell_NotifyIconW, NIM_DELETE, NOTIFYICONDATAW};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowLongPtrW, KillTimer, LoadIconW, PostMessageW, PostQuitMessage, RegisterClassW,
    RegisterWindowMessageW, SetTimer, SetWindowLongPtrW, TranslateMessage, CREATESTRUCTW,
    CS_DBLCLKS, GWLP_USERDATA, IDI_APPLICATION, MSG, WINDOW_EX_STYLE, WM_CLOSE, WM_DESTROY,
    WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_NCCREATE, WM_RBUTTONUP, WM_TIMER, WNDCLASSW, WS_OVERLAPPED,
};

use super::icon::{self, TrayNotifier};
use super::menu;
use crate::commands::ids;
use crate::core::command::{CommandContext, CommandOutcome, CommandRegistry};
use crate::core::state::Shared;
use crate::core::util;
use crate::features::audio::AudioRequest;
use crate::features::ui;

const TIMER_ID: usize = 1;

static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);
/// Handle of the hidden tray window, so other threads can ask it to close.
static TRAY_HWND: AtomicIsize = AtomicIsize::new(0);

/// Posts `WM_CLOSE` to the tray window, ending its message loop.
pub fn request_exit() {
    let raw = TRAY_HWND.load(Ordering::SeqCst);
    if raw != 0 {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(raw as *mut c_void)),
                WM_CLOSE,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}

struct Ctx {
    shared: Arc<Shared>,
    audio: Sender<AudioRequest>,
    commands: Arc<CommandRegistry>,
    hwnd: HWND,
    nid: NOTIFYICONDATAW,
    last_tip: RefCell<String>,
}

pub fn run(
    shared: Arc<Shared>,
    audio: Sender<AudioRequest>,
    commands: Arc<CommandRegistry>,
) -> windows::core::Result<()> {
    unsafe {
        let module = GetModuleHandleW(None)?;
        let instance = windows::Win32::Foundation::HINSTANCE(module.0);
        let icon = LoadIconW(None, IDI_APPLICATION)?;
        let class_name = w!("BtVolumeTrayWindow");

        let window_class = WNDCLASSW {
            style: CS_DBLCLKS,
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            hIcon: icon,
            lpszClassName: class_name,
            ..Default::default()
        };

        if RegisterClassW(&window_class) == 0 {
            return Err(windows::core::Error::from_thread());
        }

        TASKBAR_CREATED.store(
            RegisterWindowMessageW(w!("TaskbarCreated")),
            Ordering::Relaxed,
        );

        let mut ctx = Box::new(Ctx {
            shared,
            audio,
            commands,
            hwnd: HWND::default(),
            nid: NOTIFYICONDATAW::default(),
            last_tip: RefCell::new(String::new()),
        });

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class_name,
            w!("sound-memory"),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            Some(&*ctx as *const Ctx as *const c_void),
        )?;

        ctx.hwnd = hwnd;
        ctx.nid = icon::make_icon_data(hwnd, icon);
        TRAY_HWND.store(hwnd.0 as isize, Ordering::SeqCst);

        if !icon::add(&ctx.nid) {
            TRAY_HWND.store(0, Ordering::SeqCst);
            let _ = DestroyWindow(hwnd);
            return Err(windows::core::Error::from_thread());
        }

        if SetTimer(Some(hwnd), TIMER_ID, 1000, None) == 0 {
            util::log("warning: could not start tray refresh timer");
        }

        let ctx_ptr = Box::into_raw(ctx);
        util::log("tray icon created");

        let mut message = MSG::default();
        loop {
            let result = GetMessageW(&mut message, None, 0, 0);
            if result.0 == 0 || result.0 == -1 {
                break;
            }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }

        let ctx = Box::from_raw(ctx_ptr);
        let _ = Shell_NotifyIconW(NIM_DELETE, &ctx.nid);
        let _ = KillTimer(Some(hwnd), TIMER_ID);
        // Make sure the window no longer points at the context before it is freed.
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        let _ = DestroyWindow(hwnd);
        TRAY_HWND.store(0, Ordering::SeqCst);
        let _ = ctx.audio.send(AudioRequest::Shutdown);
        ui::request_exit();
        util::log("tray icon removed");
    }

    Ok(())
}

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let context = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Ctx;

    match message {
        WM_NCCREATE => {
            let create = lparam.0 as *const CREATESTRUCTW;
            if !create.is_null() {
                let context = (*create).lpCreateParams as *mut Ctx;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, context as isize);
            }
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        icon::CALLBACK_MESSAGE if !context.is_null() => {
            (*context).on_tray((lparam.0 as u32) & 0xFFFF);
            LRESULT(0)
        }
        WM_TIMER if !context.is_null() => {
            (*context).on_timer();
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ if !context.is_null()
            && TASKBAR_CREATED.load(Ordering::Relaxed) != 0
            && message == TASKBAR_CREATED.load(Ordering::Relaxed) =>
        {
            (*context).restore_icon();
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

impl Ctx {
    fn restore_icon(&self) {
        if icon::add(&self.nid) {
            self.last_tip.borrow_mut().clear();
            self.update_tip();
            util::log("tray icon re-created after Explorer restart");
        }
    }

    fn on_tray(&self, event: u32) {
        match event {
            WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
                if ui::is_available() {
                    self.execute(ids::SHOW_WINDOW);
                } else {
                    self.execute(ids::SHOW_STATUS);
                }
            }
            WM_RBUTTONUP => self.show_menu(),
            _ => {}
        }
    }

    fn on_timer(&self) {
        let note = self
            .shared
            .notes
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pop_front();

        if let Some(note) = note {
            icon::balloon(&self.nid, &note, false);
        }

        self.update_tip();
    }

    fn update_tip(&self) {
        let devices = self
            .shared
            .devices
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let tip = match devices.as_slice() {
            [] => "sound-memory · waiting for Bluetooth audio".to_string(),
            [one] => format!(
                "sound-memory · {} · {}%",
                util::truncate(&one.name, 60),
                one.level_pct
            ),
            many => format!(
                "sound-memory · {} devices · {}%",
                many.len(),
                many[0].level_pct
            ),
        };
        drop(devices);

        if tip == *self.last_tip.borrow() {
            return;
        }

        icon::update_tip(&self.nid, &tip);
        *self.last_tip.borrow_mut() = tip;
    }

    fn show_menu(&self) {
        if let Some(id) = menu::show(self.hwnd, &self.commands) {
            self.execute(id);
        }
    }

    fn execute(&self, id: &'static str) {
        let ctx = CommandContext::new(Arc::clone(&self.shared), self.audio.clone())
            .with_notifier(Box::new(TrayNotifier::new(self.nid)));

        if matches!(
            self.commands.run(id, &ctx),
            Some(CommandOutcome::Exit)
        ) {
            unsafe {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

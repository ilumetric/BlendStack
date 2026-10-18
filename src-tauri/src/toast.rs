//! Windows notifications through the notification-area balloon API.
//!
//! Toast APIs only work for apps registered with the shell (Start menu
//! shortcut or registry entry). Balloons need neither, so the self-contained
//! executable can notify without writing anything outside its folder. Windows
//! renders them as regular notifications.

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Mutex, OnceLock};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_LARGE_ICON,
    NIIF_RESPECT_QUIET_TIME, NIIF_USER, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIM_SETVERSION,
    NIN_BALLOONUSERCLICK, NOTIFYICONDATAW, NOTIFYICON_VERSION_4,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateIconFromResourceEx, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW,
    PostMessageW, RegisterClassW, TranslateMessage, HICON, LR_DEFAULTCOLOR, MSG, WM_APP,
    WM_LBUTTONUP, WNDCLASSW,
};

const WM_ICON: u32 = WM_APP + 1;
const WM_SHOW: u32 = WM_APP + 2;
const WM_REMOVE: u32 = WM_APP + 3;
const ICON_ID: u32 = 1;

static HWND_VALUE: AtomicIsize = AtomicIsize::new(0);
static ADDED: AtomicBool = AtomicBool::new(false);
static PENDING: Mutex<Option<(String, String)>> = Mutex::new(None);
static ON_CLICK: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

/// Starts the hidden window that owns the notification icon.
pub fn init(on_click: impl Fn() + Send + Sync + 'static) {
    if ON_CLICK.set(Box::new(on_click)).is_err() {
        return;
    }
    std::thread::Builder::new()
        .name("notifications".into())
        .spawn(|| unsafe { run_window() })
        .ok();
}

pub fn show(title: &str, body: &str) {
    *PENDING.lock().unwrap() = Some((title.to_string(), body.to_string()));
    post(WM_SHOW);
}

/// Removes the notification icon. The icon stays in the notification area
/// until the user has seen the news (removing it earlier would also withdraw
/// the notification from the Action Center).
pub fn clear() {
    post(WM_REMOVE);
}

fn post(msg: u32) {
    let hwnd = HWND_VALUE.load(Ordering::Acquire);
    if hwnd != 0 {
        unsafe {
            PostMessageW(hwnd as HWND, msg, 0, 0);
        }
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn copy_into(dst: &mut [u16], s: &str) {
    let src: Vec<u16> = s.encode_utf16().take(dst.len() - 1).collect();
    dst[..src.len()].copy_from_slice(&src);
    dst[src.len()] = 0;
}

unsafe fn icon(png: &[u8], size: i32) -> HICON {
    CreateIconFromResourceEx(
        png.as_ptr(),
        png.len() as u32,
        1,
        0x0003_0000,
        size,
        size,
        LR_DEFAULTCOLOR,
    )
}

unsafe fn run_window() {
    let instance = GetModuleHandleW(std::ptr::null());
    let class = wide("BlendStackNotifications");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        lpszClassName: class.as_ptr(),
        ..std::mem::zeroed()
    };
    RegisterClassW(&wc);
    // A hidden top-level window: the shell posts icon events to it.
    let hwnd = CreateWindowExW(
        0,
        class.as_ptr(),
        class.as_ptr(),
        0,
        0,
        0,
        0,
        0,
        std::ptr::null_mut(),
        std::ptr::null_mut(),
        instance,
        std::ptr::null(),
    );
    if hwnd.is_null() {
        log::warn!("could not create the notification window");
        return;
    }
    HWND_VALUE.store(hwnd as isize, Ordering::Release);
    if PENDING.lock().unwrap().is_some() {
        PostMessageW(hwnd, WM_SHOW, 0, 0);
    }
    let mut msg: MSG = std::mem::zeroed();
    while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
}

unsafe fn base_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = ICON_ID;
    nid
}

unsafe fn remove_icon(hwnd: HWND) {
    if ADDED.swap(false, Ordering::AcqRel) {
        let nid = base_data(hwnd);
        Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

unsafe fn show_pending(hwnd: HWND) {
    let Some((title, body)) = PENDING.lock().unwrap().take() else {
        return;
    };
    static SMALL: &[u8] = include_bytes!("../icons/32x32.png");
    static LARGE: &[u8] = include_bytes!("../icons/64x64.png");

    let mut nid = base_data(hwnd);
    nid.uFlags = NIF_ICON | NIF_TIP | NIF_MESSAGE | NIF_INFO;
    nid.uCallbackMessage = WM_ICON;
    nid.hIcon = icon(SMALL, 32);
    nid.hBalloonIcon = icon(LARGE, 64);
    nid.dwInfoFlags = NIIF_USER | NIIF_LARGE_ICON | NIIF_RESPECT_QUIET_TIME;
    copy_into(&mut nid.szTip, &format!("Blend Stack — {title}"));
    copy_into(&mut nid.szInfoTitle, &title);
    copy_into(&mut nid.szInfo, &body);

    if ADDED.load(Ordering::Acquire) {
        Shell_NotifyIconW(NIM_MODIFY, &nid);
    } else if Shell_NotifyIconW(NIM_ADD, &nid) != 0 {
        ADDED.store(true, Ordering::Release);
        nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        Shell_NotifyIconW(NIM_SETVERSION, &nid);
    } else {
        log::warn!("could not show a notification");
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_SHOW => {
            show_pending(hwnd);
            0
        }
        WM_REMOVE => {
            remove_icon(hwnd);
            0
        }
        WM_ICON => {
            // NOTIFYICON_VERSION_4: the event is in the low word of lParam.
            let event = (lparam as u32) & 0xFFFF;
            match event {
                NIN_BALLOONUSERCLICK | WM_LBUTTONUP => {
                    remove_icon(hwnd);
                    if let Some(f) = ON_CLICK.get() {
                        f();
                    }
                }
                _ => {}
            }
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

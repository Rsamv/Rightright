// 自定义快捷键捕获窗：聚焦后按下组合键即记录，保存后回调主程序注册。
use crate::glass;
use native_windows_gui as nwg;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use winapi::shared::minwindef::{FALSE, LPARAM, LRESULT, TRUE, UINT, WPARAM};
use winapi::shared::windef::{HWND, RECT};
use winapi::um::winuser::{
    BeginPaint, EndPaint, FillRect, GetClientRect, GetKeyState, GetSystemMetrics, InvalidateRect,
    LoadCursorW, ReleaseCapture, ScreenToClient, SetCapture, SetCursor, SetForegroundWindow,
    SetWindowPos, ShowWindow, TrackMouseEvent, IDC_HAND, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN,
    PAINTSTRUCT, SWP_NOZORDER, SW_HIDE, TRACKMOUSEEVENT, TME_LEAVE, VK_CONTROL, VK_ESCAPE,
    VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MENU, VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN,
    VK_SHIFT, VK_F1, VK_F12, WM_CLOSE, WM_KEYDOWN, WM_SYSKEYDOWN,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum KHit {
    Close,
    Cancel,
    Save,
}

struct KState {
    mods: u32,
    vk: u32,
    has: bool,
    hover: Option<KHit>,
    press: Option<KHit>,
}

pub struct HkCap {
    window: nwg::Window,
    hwnd: HWND,
    state: Rc<RefCell<KState>>,
}

/// (mods, vk) -> 配置串，如 ctrl+shift+t
pub fn hotkey_string(mods: u32, vk: u32) -> String {
    let mut s = String::new();
    if mods & MOD_CONTROL as u32 != 0 {
        s.push_str("ctrl+");
    }
    if mods & MOD_SHIFT as u32 != 0 {
        s.push_str("shift+");
    }
    if mods & MOD_ALT as u32 != 0 {
        s.push_str("alt+");
    }
    if mods & MOD_WIN as u32 != 0 {
        s.push_str("win+");
    }
    let key = match vk {
        v if (0x41..=0x5A).contains(&v) => ((v as u8) as char).to_ascii_lowercase().to_string(),
        v if (0x30..=0x39).contains(&v) => ((v as u8) as char).to_string(),
        v if (VK_F1 as u32..=VK_F12 as u32).contains(&v) => format!("f{}", v - VK_F1 as u32 + 1),
        v if v == winapi::um::winuser::VK_SPACE as u32 => "space".into(),
        v if v == winapi::um::winuser::VK_TAB as u32 => "tab".into(),
        v if v == winapi::um::winuser::VK_RETURN as u32 => "enter".into(),
        _ => format!("vk{vk}"),
    };
    s.push_str(&key);
    s
}

fn play(w: i32, scale: f32) -> (RECT, RECT, RECT, RECT, RECT) {
    let s = |v: i32| glass::px(v, scale);
    let p = s(16);
    let bw = s(96);
    let gap = s(8);
    (
        glass::rect(p, s(12), w - s(40), s(36)),
        glass::rect(w - s(40), s(12), w - p, s(36)),
        glass::rect(p, s(44), w - p, s(80)),
        glass::rect(w - p - bw - gap - bw, s(96), w - p - bw - gap, s(128)),
        glass::rect(w - p - bw, s(96), w - p, s(128)),
    )
}

fn khit(w: i32, scale: f32, x: i32, y: i32) -> Option<KHit> {
    let (_, close, _, cancel, save) = play(w, scale);
    if glass::in_rect(&close, x, y) {
        Some(KHit::Close)
    } else if glass::in_rect(&save, x, y) {
        Some(KHit::Save)
    } else if glass::in_rect(&cancel, x, y) {
        Some(KHit::Cancel)
    } else {
        None
    }
}

fn is_modifier(vk: u32) -> bool {
    [
        VK_CONTROL, VK_SHIFT, VK_MENU, VK_LWIN, VK_RWIN, VK_LCONTROL, VK_RCONTROL, VK_LSHIFT,
        VK_RSHIFT, VK_LMENU, VK_RMENU,
    ]
    .iter()
    .any(|m| *m as u32 == vk)
}

impl HkCap {
    pub fn new(scale: f32, icon: &nwg::Icon, on_save: Rc<dyn Fn(u32, u32)>) -> HkCap {
        let mut window = nwg::Window::default();
        nwg::Window::builder()
            .flags(nwg::WindowFlags::WINDOW)
            .size((glass::px(440, scale), glass::px(144, scale)))
            .position((600, 400))
            .title("设置快捷键")
            .icon(Some(icon))
            .build(&mut window)
            .unwrap();
        let hwnd = window.handle.hwnd().unwrap();
        unsafe {
            winapi::um::winuser::SetWindowLongW(
                hwnd,
                winapi::um::winuser::GWL_STYLE,
                (winapi::um::winuser::WS_POPUP | winapi::um::winuser::WS_CLIPCHILDREN) as i32,
            );
            SetWindowPos(hwnd, std::ptr::null_mut(), 0, 0, 0, 0, winapi::um::winuser::SWP_FRAMECHANGED | SWP_NOZORDER | 0x0002 | 0x0001);
            glass::force_window_icons(hwnd);
        }
        glass::apply_round_corners(hwnd);

        let state = Rc::new(RefCell::new(KState { mods: 0, vk: 0, has: false, hover: None, press: None }));
        let fonts = glass::Fonts::new(scale);
        let bg = glass::bg_brush();
        let orig = Rc::new(Cell::new(0isize));
        let hook_orig = orig.clone();
        let hs = state.clone();
        let save_cb = on_save.clone();

        let hook = Box::new(move |h: HWND, msg: UINT, w: WPARAM, l: LPARAM| -> LRESULT {
            let st = &hs;
            let mut cr: RECT = unsafe { std::mem::zeroed() };
            unsafe { GetClientRect(h, &mut cr) };
            let (title, close, card, cancel, save) = play(cr.right, scale);
            let mut cx = (l as i32) & 0xFFFF;
            let mut cy = ((l as i32) >> 16) & 0xFFFF;
            if cx > 32767 { cx -= 65536; }
            if cy > 32767 { cy -= 65536; }
            match msg {
                glass::WM_ERASEBKGND => 1,
                WM_CLOSE => { unsafe { ShowWindow(h, SW_HIDE); } 0 }
                WM_KEYDOWN | WM_SYSKEYDOWN => {
                    let vk = w as u32;
                    if vk == VK_ESCAPE as u32 {
                        unsafe { ShowWindow(h, SW_HIDE); }
                        return 0;
                    }
                    if !is_modifier(vk) {
                        let mut mods = 0u32;
                        unsafe {
                            if GetKeyState(VK_CONTROL) < 0 { mods |= MOD_CONTROL as u32; }
                            if GetKeyState(VK_SHIFT) < 0 { mods |= MOD_SHIFT as u32; }
                            if GetKeyState(VK_MENU) < 0 { mods |= MOD_ALT as u32; }
                            if GetKeyState(VK_LWIN) < 0 || GetKeyState(VK_RWIN) < 0 { mods |= MOD_WIN as u32; }
                        }
                        let mut s = st.borrow_mut();
                        s.mods = mods;
                        s.vk = vk;
                        s.has = true;
                        drop(s);
                        unsafe { InvalidateRect(h, std::ptr::null(), FALSE) };
                    }
                    0
                }
                glass::WM_NCHITTEST => {
                    let mut pt = winapi::shared::windef::POINT { x: cx, y: cy };
                    unsafe { ScreenToClient(h, &mut pt) };
                    if glass::in_rect(&title, pt.x, pt.y) && !glass::in_rect(&close, pt.x, pt.y) {
                        glass::HTCAPTION as LRESULT
                    } else {
                        glass::HTCLIENT as LRESULT
                    }
                }
                glass::WM_MOUSEMOVE => {
                    let hit = khit(cr.right, scale, cx, cy);
                    if st.borrow().hover != hit {
                        st.borrow_mut().hover = hit;
                        unsafe { InvalidateRect(h, std::ptr::null(), FALSE) };
                    }
                    let mut tme = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: h,
                        dwHoverTime: 0,
                    };
                    unsafe { TrackMouseEvent(&mut tme) };
                    0
                }
                glass::WM_MOUSELEAVE => {
                    st.borrow_mut().hover = None;
                    unsafe { InvalidateRect(h, std::ptr::null(), FALSE) };
                    0
                }
                glass::WM_SETCURSOR => {
                    if st.borrow().hover.is_some() {
                        unsafe { SetCursor(LoadCursorW(std::ptr::null_mut(), IDC_HAND)) };
                        1
                    } else {
                        unsafe { glass::call_orig(hook_orig.get(), h, msg, w, l) }
                    }
                }
                glass::WM_LBUTTONDOWN => {
                    st.borrow_mut().press = khit(cr.right, scale, cx, cy);
                    unsafe { SetCapture(h) };
                    unsafe { InvalidateRect(h, std::ptr::null(), FALSE) };
                    0
                }
                glass::WM_LBUTTONUP => {
                    unsafe { ReleaseCapture() };
                    let hit = khit(cr.right, scale, cx, cy);
                    let pressed = st.borrow().press;
                    st.borrow_mut().press = None;
                    if pressed == hit {
                        match hit {
                            Some(KHit::Save) => {
                                let (m, v, has) = { let s = st.borrow(); (s.mods, s.vk, s.has) };
                                if has {
                                    unsafe { ShowWindow(h, SW_HIDE); }
                                    save_cb(m, v);
                                }
                            }
                            Some(KHit::Close) | Some(KHit::Cancel) => { unsafe { ShowWindow(h, SW_HIDE); } }
                            None => {}
                        }
                    }
                    unsafe { InvalidateRect(h, std::ptr::null(), FALSE) };
                    0
                }
                glass::WM_PAINT => {
                    let mut ps: PAINTSTRUCT = unsafe { std::mem::zeroed() };
                    let hdc = unsafe { BeginPaint(h, &mut ps) };
                    unsafe {
                        glass::double_buffer(hdc, cr.right, cr.bottom, |mdc| {
                            FillRect(mdc, &cr, bg);
                            let s = |v: i32| glass::px(v, scale);
                            glass::draw_text(mdc, "设置快捷键", &title, glass::TEXT, fonts.title, false);
                            if st.borrow().hover == Some(KHit::Close) {
                                glass::round_panel(mdc, &close, s(8), glass::HOVER, None);
                            }
                            glass::draw_text(mdc, "×", &close, glass::DIM, fonts.body, true);
                            glass::round_panel(mdc, &card, s(8), glass::CARD, Some(glass::CARD_BORDER));
                            let inner = glass::rect(card.left + s(12), card.top, card.right - s(12), card.bottom);
                            let (txt, color) = {
                                let b = st.borrow();
                                if b.has {
                                    (hotkey_string(b.mods, b.vk), glass::TEXT)
                                } else {
                                    ("按下新的快捷键…".to_string(), glass::DIM)
                                }
                            };
                            glass::draw_text(mdc, &txt, &inner, color, fonts.body, false);
                            let cfill = if st.borrow().hover == Some(KHit::Cancel) { glass::HOVER } else { glass::BTN2 };
                            glass::round_panel(mdc, &cancel, s(8), cfill, Some(glass::BTN2_BORDER));
                            glass::draw_text(mdc, "取消", &cancel, glass::BTN2_TEXT, fonts.body, true);
                            let has = st.borrow().has;
                            let sfill = if !has { glass::HOVER } else if st.borrow().press == Some(KHit::Save) { glass::rgb(29, 78, 214) } else { glass::ACCENT };
                            glass::round_panel(mdc, &save, s(8), sfill, None);
                            glass::draw_text(mdc, "保存", &save, glass::ACCENT_TEXT, fonts.bold, true);
                        });
                        EndPaint(h, &mut ps);
                    }
                    0
                }
                _ => unsafe { glass::call_orig(hook_orig.get(), h, msg, w, l) },
            }
        });
        let o = unsafe { glass::install_hook(hwnd, hook) };
        orig.set(o);
        HkCap { window, hwnd, state }
    }

    pub fn show(&self) {
        {
            let mut s = self.state.borrow_mut();
            s.has = false;
            s.mods = 0;
            s.vk = 0;
            s.hover = None;
            s.press = None;
        }
        let (cx, cy) = nwg::GlobalCursor::position();
        let sw = unsafe { GetSystemMetrics(0) };
        let sh = unsafe { GetSystemMetrics(1) };
        let mut cr: RECT = unsafe { std::mem::zeroed() };
        unsafe { winapi::um::winuser::GetWindowRect(self.hwnd, &mut cr) };
        let w = cr.right - cr.left;
        let h = cr.bottom - cr.top;
        let x = (cx + 12).min(sw - w - 8).max(8);
        let y = (cy + 12).min(sh - h - 8).max(8);
        unsafe {
            winapi::um::winuser::MoveWindow(self.hwnd, x, y, w, h, TRUE);
            InvalidateRect(self.hwnd, std::ptr::null(), TRUE);
        }
        self.window.set_visible(true);
        unsafe { SetForegroundWindow(self.hwnd) };
    }
}
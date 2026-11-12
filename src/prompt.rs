// 纯链接剪贴板时弹出的小确认窗：是否用默认浏览器打开。
use crate::glass;
use native_windows_gui as nwg;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use winapi::shared::minwindef::{FALSE, LPARAM, LRESULT, TRUE, UINT, WPARAM};
use winapi::shared::windef::{HWND, RECT};
use winapi::um::winuser::{
    BeginPaint, EndPaint, FillRect, GetClientRect, GetSystemMetrics, InvalidateRect, LoadCursorW,
    ReleaseCapture, ScreenToClient, SetCapture, SetCursor, SetForegroundWindow, SetWindowPos,
    ShowWindow, TrackMouseEvent, IDC_HAND, PAINTSTRUCT, SWP_NOZORDER, SW_HIDE, TRACKMOUSEEVENT,
    TME_LEAVE, WM_CLOSE,
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum PHit {
    Close,
    Open,
    Cancel,
}

struct PState {
    url: String,
    hover: Option<PHit>,
    press: Option<PHit>,
}

pub struct Prompt {
    window: nwg::Window,
    hwnd: HWND,
    state: Rc<RefCell<PState>>,
}

fn play(w: i32, scale: f32) -> (RECT, RECT, RECT, RECT, RECT) {
    let s = |v: i32| glass::px(v, scale);
    let p = s(16);
    let bw = s(96);
    let gap = s(8);
    (
        glass::rect(p, s(12), w - s(40), s(36)),          // title
        glass::rect(w - s(40), s(12), w - p, s(36)),      // close
        glass::rect(p, s(44), w - p, s(76)),              // url card
        glass::rect(w - p - bw - gap - bw, s(92), w - p - bw - gap, s(124)), // cancel
        glass::rect(w - p - bw, s(92), w - p, s(124)),    // open
    )
}

fn phit(w: i32, scale: f32, x: i32, y: i32) -> Option<PHit> {
    let (_, close, _, cancel, open) = play(w, scale);
    if glass::in_rect(&close, x, y) {
        Some(PHit::Close)
    } else if glass::in_rect(&open, x, y) {
        Some(PHit::Open)
    } else if glass::in_rect(&cancel, x, y) {
        Some(PHit::Cancel)
    } else {
        None
    }
}

impl Prompt {
    pub fn new(scale: f32, icon: &nwg::Icon) -> Prompt {
        let mut window = nwg::Window::default();
        nwg::Window::builder()
            .flags(nwg::WindowFlags::WINDOW)
            .size((glass::px(440, scale), glass::px(140, scale)))
            .position((600, 400))
            .title("打开链接")
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
        }
        glass::apply_round_corners(hwnd);
        unsafe { glass::force_window_icons(hwnd) };

        let state = Rc::new(RefCell::new(PState { url: String::new(), hover: None, press: None }));
        let fonts = glass::Fonts::new(scale);
        let bg = glass::bg_brush();
        let orig = Rc::new(Cell::new(0isize));
        let hook_orig = orig.clone();
        let hs = state.clone();

        let hook = Box::new(move |h: HWND, msg: UINT, w: WPARAM, l: LPARAM| -> LRESULT {
            let st = &hs;
            let mut cr: RECT = unsafe { std::mem::zeroed() };
            unsafe { GetClientRect(h, &mut cr) };
            let (title, close, urlcard, cancel, open) = play(cr.right, scale);
            let mut cx = (l as i32) & 0xFFFF;
            let mut cy = ((l as i32) >> 16) & 0xFFFF;
            if cx > 32767 { cx -= 65536; }
            if cy > 32767 { cy -= 65536; }
            match msg {
                glass::WM_ERASEBKGND => 1,
                WM_CLOSE => { unsafe { ShowWindow(h, SW_HIDE); } 0 }
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
                    let hit = phit(cr.right, scale, cx, cy);
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
                    st.borrow_mut().press = phit(cr.right, scale, cx, cy);
                    unsafe { SetCapture(h) };
                    unsafe { InvalidateRect(h, std::ptr::null(), FALSE) };
                    0
                }
                glass::WM_LBUTTONUP => {
                    unsafe { ReleaseCapture() };
                    let hit = phit(cr.right, scale, cx, cy);
                    let pressed = st.borrow().press;
                    st.borrow_mut().press = None;
                    if pressed == hit {
                        match hit {
                            Some(PHit::Open) => {
                                let url = st.borrow().url.clone();
                                unsafe { ShowWindow(h, SW_HIDE); }
                                if !url.is_empty() { let _ = open::that(&url); }
                            }
                            Some(PHit::Close) | Some(PHit::Cancel) => { unsafe { ShowWindow(h, SW_HIDE); } }
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
                            glass::draw_text(mdc, "打开链接？", &title, glass::TEXT, fonts.title, false);
                            if st.borrow().hover == Some(PHit::Close) {
                                glass::round_panel(mdc, &close, s(8), glass::HOVER, None);
                            }
                            glass::draw_text(mdc, "×", &close, glass::DIM, fonts.body, true);
                            glass::round_panel(mdc, &urlcard, s(8), glass::CARD, Some(glass::CARD_BORDER));
                            let inner = glass::rect(urlcard.left + s(10), urlcard.top, urlcard.right - s(10), urlcard.bottom);
                            glass::draw_text(mdc, &st.borrow().url, &inner, glass::DIM, fonts.small, false);
                            let cfill = if st.borrow().hover == Some(PHit::Cancel) { glass::HOVER } else { glass::BTN2 };
                            glass::round_panel(mdc, &cancel, s(8), cfill, Some(glass::BTN2_BORDER));
                            glass::draw_text(mdc, "取消", &cancel, glass::BTN2_TEXT, fonts.body, true);
                            let ofill = if st.borrow().press == Some(PHit::Open) { glass::rgb(29, 78, 214) } else { glass::ACCENT };
                            glass::round_panel(mdc, &open, s(8), ofill, None);
                            glass::draw_text(mdc, "打开", &open, glass::ACCENT_TEXT, fonts.bold, true);
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
        Prompt { window, hwnd, state }
    }

    pub fn hide(&self) {
        self.window.set_visible(false);
    }

    pub fn show(&self, url: &str) {
        self.state.borrow_mut().url = url.to_string();
        self.state.borrow_mut().hover = None;
        self.state.borrow_mut().press = None;
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
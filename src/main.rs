// 右键翻译 —— 全局快捷键划词翻译 + 链接跳转，浅色 Fluent 原生 Win32 界面，托盘常驻。
#![windows_subsystem = "windows"]

use rightright::{config, glass, hkcap, prompt, translate};

use config::{Config, LANGS};
use glass::{Hit, Layout};
use native_windows_gui as nwg;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use winapi::shared::minwindef::{FALSE, LPARAM, LRESULT, TRUE, UINT, WPARAM};
use winapi::shared::windef::{HDC, HICON, HWND, RECT};
use winapi::um::libloaderapi::GetModuleHandleW;
use winapi::um::uxtheme::SetWindowTheme;
use winapi::um::wingdi::{CreateRectRgn, SetBkColor, SetTextColor};
use winapi::um::winuser::{
    BeginPaint, DrawIconEx, EndPaint, FillRect, GetClientRect, GetDpiForSystem, GetWindowLongW,
    GetWindowTextLengthW, GetWindowTextW, InvalidateRect, LoadCursorW, LoadIconW, MoveWindow,
    RegisterHotKey, ReleaseCapture, ScreenToClient, SendMessageW, SetCapture, SetCursor,
    SetForegroundWindow, SetProcessDPIAware, SetWindowLongW, SetWindowPos, SetWindowRgn, ShowWindow,
    TrackMouseEvent, UnregisterHotKey, GWL_EXSTYLE, GWL_STYLE, IDC_HAND, MOD_ALT, MOD_CONTROL,
    MOD_SHIFT, MOD_WIN, PAINTSTRUCT, SWP_FRAMECHANGED, SWP_NOZORDER, SW_HIDE, SW_SHOW,
    TRACKMOUSEEVENT, TME_LEAVE, VK_ESCAPE, VK_F1, VK_RETURN, VK_SPACE, VK_TAB, WM_HOTKEY,
    WM_SETFONT, WM_SETTEXT, WS_BORDER, WS_CLIPCHILDREN, WS_EX_CLIENTEDGE, WS_POPUP,
};

const DI_NORMAL: u32 = 0x0003;
const HOTKEY_ID: i32 = 1;
const DEFAULT_HOTKEY: &str = "ctrl+shift+t";

struct State {
    cfg: Config,
    latest_id: u64,
    hover: Option<Hit>,
    press: Option<Hit>,
}

struct Shared {
    payload: Mutex<Option<(u64, Result<String, String>)>>,
    next_id: AtomicU64,
}

fn detect_url(t: &str) -> Option<String> {
    // 纯链接才弹确认窗：前后有空格或杂字符一律不算
    let s = t;
    if s.is_empty() || s.contains(char::is_whitespace) {
        return None;
    }
    if s.starts_with("http://") || s.starts_with("https://") {
        Some(s.to_string())
    } else if s.starts_with("www.") && s.contains('.') {
        Some(format!("https://{s}"))
    } else {
        None
    }
}

fn lang_name(code: &str) -> &'static str {
    LANGS.iter().find(|(_, c)| *c == code).map(|(n, _)| *n).unwrap_or("简体中文")
}

fn parse_hotkey(s: &str) -> Option<(u32, u32)> {
    let mut mods = 0u32;
    let mut vk = 0u32;
    for part in s.split('+') {
        let p = part.trim().to_ascii_lowercase();
        match p.as_str() {
            "ctrl" | "control" => mods |= MOD_CONTROL as u32,
            "shift" => mods |= MOD_SHIFT as u32,
            "alt" => mods |= MOD_ALT as u32,
            "win" | "meta" | "super" => mods |= MOD_WIN as u32,
            "" => {}
            _ => {
                if vk != 0 {
                    return None;
                }
                vk = match p.as_str() {
                    "space" => VK_SPACE as u32,
                    "tab" => VK_TAB as u32,
                    "enter" | "return" => VK_RETURN as u32,
                    "esc" | "escape" => VK_ESCAPE as u32,
                    _ => {
                        if p.len() == 1 {
                            let c = p.chars().next().unwrap();
                            if c.is_ascii_alphabetic() {
                                c.to_ascii_uppercase() as u32
                            } else if c.is_ascii_digit() {
                                c as u32
                            } else {
                                return None;
                            }
                        } else if p.starts_with('f') {
                            let n: u32 = p[1..].parse().ok()?;
                            if (1..=12).contains(&n) {
                                VK_F1 as u32 + n - 1
                            } else {
                                return None;
                            }
                        } else {
                            return None;
                        }
                    }
                };
            }
        }
    }
    if vk == 0 {
        None
    } else {
        Some((mods, vk))
    }
}

// 编辑框自带客户区边线，用内缩 2px 的窗口区域裁掉，露出卡片白底
fn trim_edit_border(h: HWND, w: i32, hgt: i32) {
    unsafe {
        let rgn = CreateRectRgn(2, 2, w - 2, hgt - 2);
        SetWindowRgn(h, rgn, TRUE);
    }
}

fn window_text(h: HWND) -> String {
    unsafe {
        let n = GetWindowTextLengthW(h);
        if n <= 0 {
            return String::new();
        }
        let mut buf: Vec<u16> = vec![0; (n + 1) as usize];
        let got = GetWindowTextW(h, buf.as_mut_ptr(), n + 1);
        String::from_utf16_lossy(&buf[..got as usize])
    }
}

fn set_edit_text(h: HWND, s: &str) {
    let w: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe { SendMessageW(h, WM_SETTEXT, 0, w.as_ptr() as LPARAM) };
}

fn paint(hdc: HDC, st: &State, l: &Layout, f: &glass::Fonts, hicon: HICON, scale: f32) {
    let s = |v: i32| glass::px(v, scale);
    unsafe { DrawIconEx(hdc, s(16), s(14), hicon, s(18), s(18), 0, std::ptr::null_mut(), DI_NORMAL) };
    let title_r = glass::rect(s(42), s(12), l.close.left - s(4), s(40));
    glass::draw_text(hdc, "右键翻译", &title_r, glass::TEXT, f.title, false);
    if st.hover == Some(Hit::Close) {
        glass::round_panel(hdc, &l.close, s(8), glass::HOVER, None);
    }
    glass::draw_text(hdc, "×", &l.close, glass::DIM, f.body, true);

    let lbl_r = glass::rect(s(16), s(46), s(86), s(74));
    glass::draw_text(hdc, "目标语言", &lbl_r, glass::DIM, f.small, false);
    let pill_fill = if st.hover == Some(Hit::Pill) { glass::HOVER } else { glass::CARD };
    glass::round_panel(hdc, &l.pill, s(14), pill_fill, Some(glass::CARD_BORDER));
    let pill_text = format!("{}  ▾", lang_name(&st.cfg.target_lang));
    let pill_inner = glass::rect(l.pill.left + s(12), l.pill.top, l.pill.right - s(12), l.pill.bottom);
    glass::draw_text(hdc, &pill_text, &pill_inner, glass::TEXT, f.body, false);

    glass::card_shadow(hdc, &l.src_card, s(10));
    glass::round_panel(hdc, &l.src_card, s(10), glass::CARD, Some(glass::CARD_BORDER));
    let src_lbl = glass::rect(l.src_card.left + s(12), l.src_card.top + s(6), l.src_card.right - s(12), l.src_card.top + s(24));
    glass::draw_text(hdc, "原文", &src_lbl, glass::DIM, f.small, false);
    glass::card_shadow(hdc, &l.out_card, s(10));
    glass::round_panel(hdc, &l.out_card, s(10), glass::CARD, Some(glass::CARD_BORDER));
    let out_lbl = glass::rect(l.out_card.left + s(12), l.out_card.top + s(6), l.out_card.right - s(12), l.out_card.top + s(24));
    glass::draw_text(hdc, "译文", &out_lbl, glass::DIM, f.small, false);

    let tr_fill = if st.hover == Some(Hit::Translate) { glass::HOVER } else { glass::BTN2 };
    glass::round_panel(hdc, &l.btn_translate, s(8), tr_fill, Some(glass::BTN2_BORDER));
    glass::draw_text(hdc, "翻译", &l.btn_translate, glass::BTN2_TEXT, f.bold, true);

    let copy_fill = if st.press == Some(Hit::Copy) { glass::rgb(29, 78, 214) } else { glass::ACCENT };
    glass::round_panel(hdc, &l.btn_copy, s(8), copy_fill, None);
    glass::draw_text(hdc, "复制译文", &l.btn_copy, glass::ACCENT_TEXT, f.bold, true);
}

fn main() {
    unsafe { SetProcessDPIAware() };
    let scale = unsafe { GetDpiForSystem() } as f32 / 96.0;
    nwg::init().expect("初始化 Win32 GUI 失败");
    let _ = nwg::Font::set_global_family("Microsoft YaHei");

    let mut window = nwg::Window::default();
    let mut icon = nwg::Icon::default();
    let mut tray = nwg::TrayNotification::default();
    let mut tray_menu = nwg::Menu::default();
    let mut mi_show = nwg::MenuItem::default();
    let mut mi_exit = nwg::MenuItem::default();
    let mut mi_hk_custom = nwg::MenuItem::default();
    let mut mi_hk_default = nwg::MenuItem::default();
    let mut sep1 = nwg::MenuSeparator::default();
    let mut sep2 = nwg::MenuSeparator::default();
    let mut notice = nwg::Notice::default();
    let mut txt_src = nwg::TextBox::default();
    let mut txt_out = nwg::TextBox::default();
    let mut lang_menu = nwg::Menu::default();
    let mut lang_items: Vec<nwg::MenuItem> = (0..LANGS.len()).map(|_| nwg::MenuItem::default()).collect();

    if nwg::Icon::builder().source_embed_id(1).build(&mut icon).is_err() {
        nwg::Icon::builder().source_system(Some(nwg::OemIcon::Sample)).build(&mut icon).unwrap();
    }

    nwg::Window::builder()
        .flags(nwg::WindowFlags::WINDOW)
        .size((glass::px(470, scale), glass::px(360, scale)))
        .position((500, 320))
        .title("右键翻译")
        .icon(Some(&icon))
        .build(&mut window)
        .unwrap();

    let hwnd = window.handle.hwnd().unwrap();
    unsafe {
        SetWindowLongW(hwnd, GWL_STYLE, (WS_POPUP | WS_CLIPCHILDREN) as i32);
        SetWindowPos(hwnd, std::ptr::null_mut(), 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOZORDER | 0x0002 | 0x0001);
        glass::force_window_icons(hwnd);
    }
    glass::apply_round_corners(hwnd);

    let tb_flags = nwg::TextBoxFlags::VISIBLE | nwg::TextBoxFlags::VSCROLL;
    nwg::TextBox::builder().parent(&window).position((0, 0)).size((10, 10)).flags(tb_flags).text("").build(&mut txt_src).unwrap();
    nwg::TextBox::builder().parent(&window).position((0, 0)).size((10, 10)).flags(tb_flags).readonly(true).text("").build(&mut txt_out).unwrap();
    let h_src = txt_src.handle.hwnd().unwrap();
    let h_out = txt_out.handle.hwnd().unwrap();

    let fonts = glass::Fonts::new(scale);
    let no_theme: Vec<u16> = vec![0];
    unsafe {
        for h in [h_src, h_out] {
            let s = GetWindowLongW(h, GWL_STYLE);
            SetWindowLongW(h, GWL_STYLE, s & !(WS_BORDER as i32));
            let e = GetWindowLongW(h, GWL_EXSTYLE);
            SetWindowLongW(h, GWL_EXSTYLE, e & !(WS_EX_CLIENTEDGE as i32));
            SetWindowPos(h, std::ptr::null_mut(), 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOZORDER | 0x0002 | 0x0001);
            SetWindowTheme(h, no_theme.as_ptr(), no_theme.as_ptr());
            SendMessageW(h, WM_SETFONT, fonts.body as WPARAM, TRUE as LPARAM);
        }
    }

    nwg::TrayNotification::builder().parent(&window).icon(Some(&icon)).tip(Some("右键翻译 · 快捷键翻译剪贴板")).build(&mut tray).unwrap();
    unsafe { glass::force_tray_icon(hwnd) };
    nwg::Menu::builder().parent(&window).popup(true).build(&mut tray_menu).unwrap();
    nwg::MenuItem::builder().parent(&tray_menu).text("显示窗口").build(&mut mi_show).unwrap();
    nwg::MenuSeparator::builder().parent(&tray_menu).build(&mut sep1).unwrap();
    nwg::MenuItem::builder().parent(&tray_menu).text("自定义快捷键…").build(&mut mi_hk_custom).unwrap();
    nwg::MenuItem::builder().parent(&tray_menu).text("恢复默认 Ctrl+Shift+T").build(&mut mi_hk_default).unwrap();
    nwg::MenuSeparator::builder().parent(&tray_menu).build(&mut sep2).unwrap();
    nwg::MenuItem::builder().parent(&tray_menu).text("退出").build(&mut mi_exit).unwrap();

    nwg::Menu::builder().parent(&window).popup(true).build(&mut lang_menu).unwrap();
    for (i, item) in lang_items.iter_mut().enumerate() {
        nwg::MenuItem::builder().parent(&lang_menu).text(LANGS[i].0).build(item).unwrap();
    }
    let lang_handles: Vec<nwg::ControlHandle> = lang_items.iter().map(|m| m.handle).collect();

    nwg::Notice::builder().parent(&window).build(&mut notice).unwrap();

    let cfg = config::load();
    let (mods, vk) = parse_hotkey(&cfg.hotkey).unwrap_or(((MOD_CONTROL | MOD_SHIFT) as u32, 'T' as u32));
    unsafe { RegisterHotKey(hwnd, HOTKEY_ID, mods, vk) };

    let link_prompt = prompt::Prompt::new(scale, &icon);
    let state = Rc::new(RefCell::new(State { cfg, latest_id: 0, hover: None, press: None }));
    let shared = Arc::new(Shared { payload: Mutex::new(None), next_id: AtomicU64::new(0) });
    let notice_sender = notice.sender();

    let h_window = window.handle;
    let h_notice = notice.handle;
    let h_show = mi_show.handle;
    let h_exit = mi_exit.handle;
    let h_hk_custom = mi_hk_custom.handle;
    let h_hk_default = mi_hk_default.handle;

    let mut cr: RECT = unsafe { std::mem::zeroed() };
    unsafe { GetClientRect(hwnd, &mut cr) };
    let l0 = glass::layout(cr.right, cr.bottom, scale);
    unsafe {
        MoveWindow(h_src, l0.src_edit.left, l0.src_edit.top, l0.src_edit.right - l0.src_edit.left, l0.src_edit.bottom - l0.src_edit.top, TRUE);
        MoveWindow(h_out, l0.out_edit.left, l0.out_edit.top, l0.out_edit.right - l0.out_edit.left, l0.out_edit.bottom - l0.out_edit.top, TRUE);
        trim_edit_border(h_src, l0.src_edit.right - l0.src_edit.left, l0.src_edit.bottom - l0.src_edit.top);
        trim_edit_border(h_out, l0.out_edit.right - l0.out_edit.left, l0.out_edit.bottom - l0.out_edit.top);
    }

    let redraw = move || unsafe { InvalidateRect(hwnd, std::ptr::null(), FALSE) };

    // 翻译一段文本：置译文占位、弹窗、后台请求
    let tt_state = state.clone();
    let tt_shared = shared.clone();
    let tt_sender = notice_sender.clone();
    let translate_text: Rc<dyn Fn(String)> = Rc::new(move |text: String| {
        if text.trim().is_empty() {
            return;
        }
        set_edit_text(h_out, "翻译中…");
        unsafe {
            InvalidateRect(hwnd, std::ptr::null(), FALSE);
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
        }
        let id = tt_shared.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        tt_state.borrow_mut().latest_id = id;
        let lang = tt_state.borrow().cfg.target_lang.clone();
        let sh = tt_shared.clone();
        let sender = tt_sender.clone();
        std::thread::spawn(move || {
            let r = translate::translate(&text, &lang);
            *sh.payload.lock().unwrap() = Some((id, r));
            sender.notice();
        });
    });

    // 快捷键动作：读剪贴板，纯链接弹确认窗，否则翻译
    let ca_tt = translate_text.clone();
    let clipboard_action: Rc<dyn Fn()> = Rc::new(move || {
        let text = match nwg::Clipboard::data_text(h_window) {
            Some(t) => t,
            None => return,
        };
        if text.trim().is_empty() {
            return;
        }
        if let Some(url) = detect_url(&text) {
            link_prompt.show(&url);
            return;
        }
        link_prompt.hide();
        set_edit_text(h_src, &text);
        ca_tt(text);
    });

    // 自定义快捷键保存回调：重新注册并落盘
    let hs_state = state.clone();
    let on_hk_save: Rc<dyn Fn(u32, u32)> = Rc::new(move |m, v| {
        unsafe {
            UnregisterHotKey(hwnd, HOTKEY_ID);
            RegisterHotKey(hwnd, HOTKEY_ID, m, v);
        }
        let mut s = hs_state.borrow_mut();
        s.cfg.hotkey = hkcap::hotkey_string(m, v);
        config::save(&s.cfg);
    });
    let hk_capture = hkcap::HkCap::new(scale, &icon, on_hk_save);

    let handler_state = state.clone();
    let handler_shared = shared.clone();
    let handler_redraw = redraw.clone();
    let handler_tt = translate_text.clone();
    let handler = nwg::full_bind_event_handler(&h_window, move |evt, _data, handle| {
        let st = &handler_state;
        match evt {
            nwg::Event::OnWindowClose => {
                if handle == h_window {
                    unsafe { ShowWindow(hwnd, SW_HIDE) };
                }
            }
            nwg::Event::OnNotice => {
                if handle != h_notice {
                    return;
                }
                let taken = handler_shared.payload.lock().unwrap().take();
                if let Some((id, r)) = taken {
                    if id == st.borrow().latest_id {
                        match r {
                            Ok(t) => set_edit_text(h_out, &t),
                            Err(e) => set_edit_text(h_out, &format!("翻译失败：{e}")),
                        }
                        handler_redraw();
                    }
                }
            }
            nwg::Event::OnMenuItemSelected => {
                if handle == h_show {
                    unsafe {
                        ShowWindow(hwnd, SW_SHOW);
                        SetForegroundWindow(hwnd);
                    }
                } else if handle == h_exit {
                    nwg::stop_thread_dispatch();
                } else if handle == h_hk_custom {
                    hk_capture.show();
                } else if handle == h_hk_default {
                    unsafe { UnregisterHotKey(hwnd, HOTKEY_ID) };
                    if let Some((m, v)) = parse_hotkey(DEFAULT_HOTKEY) {
                        unsafe { RegisterHotKey(hwnd, HOTKEY_ID, m, v) };
                    }
                    let mut s = st.borrow_mut();
                    s.cfg.hotkey = DEFAULT_HOTKEY.to_string();
                    config::save(&s.cfg);
                } else if let Some(idx) = lang_handles.iter().position(|h| *h == handle) {
                    let code = LANGS[idx].1.to_string();
                    {
                        let mut s = st.borrow_mut();
                        s.cfg.target_lang = code.clone();
                        config::save(&s.cfg);
                    }
                    handler_redraw();
                    let src = window_text(h_src);
                    handler_tt(src);
                }
            }
            nwg::Event::OnContextMenu => {
                let (x, y) = nwg::GlobalCursor::position();
                tray_menu.popup(x, y);
            }
            _ => {}
        }
    });

    let hook_state = state.clone();
    let hook_tt = translate_text.clone();
    let hook_ca = clipboard_action.clone();
    let orig = Rc::new(Cell::new(0isize));
    let hook_orig = orig.clone();
    let white = glass::white_brush();
    let bg = glass::bg_brush();
    let hicon = unsafe { LoadIconW(GetModuleHandleW(std::ptr::null()), 1 as *const u16) };
    let hook = Box::new(move |h: HWND, msg: UINT, w: WPARAM, l: LPARAM| -> LRESULT {
        let st = &hook_state;
        let mut cr: RECT = unsafe { std::mem::zeroed() };
        unsafe { GetClientRect(h, &mut cr) };
        let lay = glass::layout(cr.right, cr.bottom, scale);
        let mut cx = (l as i32) & 0xFFFF;
        let mut cy = ((l as i32) >> 16) & 0xFFFF;
        if cx > 32767 { cx -= 65536; }
        if cy > 32767 { cy -= 65536; }
        match msg {
            glass::WM_ERASEBKGND => 1,
            WM_HOTKEY => {
                if w as i32 == HOTKEY_ID {
                    hook_ca();
                }
                0
            }
            glass::WM_SIZE => unsafe {
                MoveWindow(h_src, lay.src_edit.left, lay.src_edit.top, lay.src_edit.right - lay.src_edit.left, lay.src_edit.bottom - lay.src_edit.top, TRUE);
                MoveWindow(h_out, lay.out_edit.left, lay.out_edit.top, lay.out_edit.right - lay.out_edit.left, lay.out_edit.bottom - lay.out_edit.top, TRUE);
                trim_edit_border(h_src, lay.src_edit.right - lay.src_edit.left, lay.src_edit.bottom - lay.src_edit.top);
                trim_edit_border(h_out, lay.out_edit.right - lay.out_edit.left, lay.out_edit.bottom - lay.out_edit.top);
                glass::call_orig(hook_orig.get(), h, msg, w, l)
            },
            glass::WM_CTLCOLOREDIT | glass::WM_CTLCOLORSTATIC => {
                unsafe {
                    SetTextColor(w as HDC, glass::TEXT);
                    SetBkColor(w as HDC, glass::CARD);
                }
                white as LRESULT
            }
            glass::WM_NCHITTEST => {
                let mut pt = winapi::shared::windef::POINT { x: cx, y: cy };
                unsafe { ScreenToClient(h, &mut pt) };
                if glass::in_rect(&lay.title, pt.x, pt.y) && !glass::in_rect(&lay.close, pt.x, pt.y) {
                    glass::HTCAPTION as LRESULT
                } else {
                    glass::HTCLIENT as LRESULT
                }
            }
            glass::WM_MOUSEMOVE => {
                let hit = glass::hit_test(&lay, cx, cy);
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
                let hit = glass::hit_test(&lay, cx, cy);
                st.borrow_mut().press = hit;
                unsafe { SetCapture(h) };
                unsafe { InvalidateRect(h, std::ptr::null(), FALSE) };
                0
            }
            glass::WM_LBUTTONUP => {
                unsafe { ReleaseCapture() };
                let hit = glass::hit_test(&lay, cx, cy);
                let pressed = st.borrow().press;
                st.borrow_mut().press = None;
                if pressed == hit {
                    match hit {
                        Some(Hit::Close) => { unsafe { ShowWindow(h, SW_HIDE); } }
                        Some(Hit::Pill) => {
                            let (x, y) = nwg::GlobalCursor::position();
                            lang_menu.popup(x, y);
                        }
                        Some(Hit::Translate) => {
                            let src = window_text(h_src);
                            hook_tt(src);
                        }
                        Some(Hit::Copy) => {
                            let out = window_text(h_out);
                            if !out.is_empty() {
                                nwg::Clipboard::set_data_text(h_window, &out);
                            }
                        }
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
                        paint(mdc, &st.borrow(), &lay, &fonts, hicon, scale);
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

    nwg::dispatch_thread_events();
    nwg::unbind_event_handler(&handler);
}
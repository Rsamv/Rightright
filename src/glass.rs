// 浅色 Fluent 卡片风 UI 的 Win32 底层：圆角、GDI 绘制与布局命中。
use winapi::shared::minwindef::{DWORD, LPARAM, LRESULT, UINT, WPARAM};
use winapi::shared::windef::{HBRUSH, HDC, HFONT, HICON, HWND, RECT};
use winapi::um::dwmapi::DwmSetWindowAttribute;
use winapi::um::libloaderapi::GetModuleHandleW;
use winapi::um::shellapi::{Shell_NotifyIconW, NIF_ICON, NIM_MODIFY, NOTIFYICONDATAW};
use winapi::um::wingdi::{
    BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateFontW, CreateSolidBrush, DeleteDC,
    DeleteObject, GetStockObject, RoundRect, SelectObject, SetBkMode, SetTextColor,
    CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_PITCH, OUT_DEFAULT_PRECIS,
    SRCCOPY, TRANSPARENT,
};
use winapi::um::winuser::{
    CallWindowProcW, DefWindowProcW, GetWindowLongPtrW, LoadImageW, SendMessageW, SetWindowLongPtrW,
    GWLP_USERDATA, GWLP_WNDPROC, ICON_BIG, ICON_SMALL, IMAGE_ICON, WM_SETICON, WNDPROC,
};

pub const fn rgb(r: u32, g: u32, b: u32) -> u32 {
    r | (g << 8) | (b << 16)
}

// 按 DPI 缩放像素常量
pub fn px(v: i32, scale: f32) -> i32 {
    (v as f32 * scale).round() as i32
}

// 浅色 Fluent 配色（baoyu-design：toned 中性色 + 发丝描边 + 单一柔和阴影）
pub const BG: u32 = rgb(250, 249, 247);
pub const CARD: u32 = rgb(255, 255, 255);
pub const CARD_BORDER: u32 = rgb(231, 229, 224);
pub const CARD_SHADOW: u32 = rgb(236, 234, 229);
pub const TEXT: u32 = rgb(28, 27, 26);
pub const DIM: u32 = rgb(110, 106, 99);
pub const ACCENT: u32 = rgb(37, 99, 235);
pub const ACCENT_TEXT: u32 = rgb(255, 255, 255);
pub const BTN2: u32 = rgb(255, 255, 255);
pub const BTN2_BORDER: u32 = rgb(214, 211, 205);
pub const BTN2_TEXT: u32 = rgb(63, 60, 55);
pub const HOVER: u32 = rgb(243, 241, 236);

const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWCP_ROUND: u32 = 2;

pub fn apply_round_corners(hwnd: HWND) {
    unsafe {
        let pref: u32 = DWMWCP_ROUND;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &pref as *const u32 as *const _,
            4,
        );
    }
}

pub struct Fonts {
    pub title: HFONT,
    pub body: HFONT,
    pub bold: HFONT,
    pub small: HFONT,
}
impl Fonts {
    pub fn new(scale: f32) -> Fonts {
        unsafe {
            let face: Vec<u16> = "Microsoft YaHei\0".encode_utf16().collect();
            let mk = |h: i32, w: i32| {
                CreateFontW(
                    px(h, scale), 0, 0, 0, w, 0, 0, 0, DEFAULT_CHARSET, OUT_DEFAULT_PRECIS,
                    CLIP_DEFAULT_PRECIS, CLEARTYPE_QUALITY, DEFAULT_PITCH, face.as_ptr(),
                )
            };
            Fonts { title: mk(-16, 700), body: mk(-14, 400), bold: mk(-14, 700), small: mk(-12, 400) }
        }
    }
}
impl Drop for Fonts {
    fn drop(&mut self) {
        unsafe {
            DeleteObject(self.title as *mut _);
            DeleteObject(self.body as *mut _);
            DeleteObject(self.bold as *mut _);
            DeleteObject(self.small as *mut _);
        }
    }
}

pub fn round_panel(hdc: HDC, r: &RECT, rad: i32, fill: u32, border: Option<u32>) {
    unsafe {
        let brush = CreateSolidBrush(fill);
        let pen_color = border.unwrap_or(fill);
        let pen = winapi::um::wingdi::CreatePen(winapi::um::wingdi::PS_SOLID as i32, 1, pen_color);
        let ob = SelectObject(hdc, brush as *mut _);
        let op = SelectObject(hdc, pen as *mut _);
        RoundRect(hdc, r.left, r.top, r.right, r.bottom, rad, rad);
        SelectObject(hdc, ob);
        SelectObject(hdc, op);
        DeleteObject(brush as *mut _);
        DeleteObject(pen as *mut _);
    }
}

pub fn draw_text(hdc: HDC, s: &str, r: &RECT, color: u32, font: HFONT, center: bool) {
    unsafe {
        let of = SelectObject(hdc, font as *mut _);
        SetBkMode(hdc, TRANSPARENT as i32);
        SetTextColor(hdc, color);
        let wide: Vec<u16> = s.encode_utf16().chain(std::iter::once(0)).collect();
        let mut rr = *r;
        let flags = if center {
            winapi::um::winuser::DT_CENTER | winapi::um::winuser::DT_SINGLELINE
                | winapi::um::winuser::DT_VCENTER | winapi::um::winuser::DT_END_ELLIPSIS
        } else {
            winapi::um::winuser::DT_LEFT | winapi::um::winuser::DT_SINGLELINE
                | winapi::um::winuser::DT_VCENTER | winapi::um::winuser::DT_END_ELLIPSIS
        };
        winapi::um::winuser::DrawTextW(hdc, wide.as_ptr(), -1, &mut rr, flags);
        SelectObject(hdc, of);
    }
}

pub fn in_rect(r: &RECT, x: i32, y: i32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

// 卡片下一层柔和阴影：比卡片略大、向下偏移的单色圆角，模拟 0 4px 24px rgba(0,0,0,.06)
pub fn card_shadow(hdc: HDC, r: &RECT, rad: i32) {
    let s = RECT { left: r.left - 1, top: r.top + 1, right: r.right + 1, bottom: r.bottom + 3 };
    round_panel(hdc, &s, rad + 2, CARD_SHADOW, None);
}

pub const fn rect(l: i32, t: i32, r: i32, b: i32) -> RECT {
    RECT { left: l, top: t, right: r, bottom: b }
}

#[derive(Clone, Copy)]
pub struct Layout {
    pub title: RECT,
    pub close: RECT,
    pub pill: RECT,
    pub src_card: RECT,
    pub src_edit: RECT,
    pub out_card: RECT,
    pub out_edit: RECT,
    pub btn_translate: RECT,
    pub btn_copy: RECT,
}

pub fn layout(w: i32, _h: i32, scale: f32) -> Layout {
    let p = px(16, scale);
    let s = |v: i32| px(v, scale);
    Layout {
        title: rect(p, s(12), w - p, s(40)),
        close: rect(w - s(40), s(12), w - s(16), s(36)),
        pill: rect(s(86), s(46), s(250), s(74)),
        src_card: rect(p, s(86), w - p, s(178)),
        src_edit: rect(p + s(10), s(112), w - p - s(10), s(170)),
        out_card: rect(p, s(186), w - p, s(300)),
        out_edit: rect(p + s(10), s(212), w - p - s(10), s(292)),
        btn_translate: rect(p, s(312), w / 2 - s(4), s(344)),
        btn_copy: rect(w / 2 + s(4), s(312), w - p, s(344)),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hit {
    Close,
    Pill,
    Translate,
    Copy,
}

pub fn hit_test(l: &Layout, x: i32, y: i32) -> Option<Hit> {
    if in_rect(&l.close, x, y) {
        Some(Hit::Close)
    } else if in_rect(&l.pill, x, y) {
        Some(Hit::Pill)
    } else if in_rect(&l.btn_translate, x, y) {
        Some(Hit::Translate)
    } else if in_rect(&l.btn_copy, x, y) {
        Some(Hit::Copy)
    } else {
        None
    }
}

// ---- 子类化钩子：把捕获闭包挂到 GWLP_USERDATA ----
pub type Hook = dyn FnMut(HWND, UINT, WPARAM, LPARAM) -> LRESULT;

pub unsafe extern "system" fn trampoline(
    hwnd: HWND,
    msg: UINT,
    w: WPARAM,
    l: LPARAM,
) -> LRESULT {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Box<Hook>;
    if ptr.is_null() {
        return DefWindowProcW(hwnd, msg, w, l);
    }
    (*ptr)(hwnd, msg, w, l)
}

pub unsafe fn install_hook(hwnd: HWND, hook: Box<Hook>) -> isize {
    let orig = GetWindowLongPtrW(hwnd, GWLP_WNDPROC);
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, Box::into_raw(Box::new(hook)) as isize);
    SetWindowLongPtrW(hwnd, GWLP_WNDPROC, trampoline as *const () as isize);
    orig
}

pub unsafe fn call_orig(orig: isize, hwnd: HWND, msg: UINT, w: WPARAM, l: LPARAM) -> LRESULT {
    CallWindowProcW(std::mem::transmute::<isize, WNDPROC>(orig), hwnd, msg, w, l)
}

pub fn white_brush() -> HBRUSH {
    unsafe { CreateSolidBrush(CARD) }
}

pub fn bg_brush() -> HBRUSH {
    unsafe { CreateSolidBrush(BG) }
}

pub fn stock_null_brush() -> HBRUSH {
    unsafe { GetStockObject(winapi::um::wingdi::NULL_BRUSH as i32) as HBRUSH }
}

// 离屏绘制后一次 BitBlt，避免 FillRect+逐笔上屏造成的闪烁
pub unsafe fn double_buffer(hdc: HDC, w: i32, h: i32, draw: impl FnOnce(HDC)) {
    let mdc = CreateCompatibleDC(hdc);
    let bmp = CreateCompatibleBitmap(hdc, w, h);
    let ob = SelectObject(mdc, bmp as *mut _);
    draw(mdc);
    BitBlt(hdc, 0, 0, w, h, mdc, 0, 0, SRCCOPY);
    SelectObject(mdc, ob);
    DeleteObject(bmp as *mut _);
    DeleteDC(mdc);
}

fn load_embed_icon(size: i32) -> HICON {
    unsafe {
        LoadImageW(
            GetModuleHandleW(std::ptr::null()),
            1 as *const u16,
            IMAGE_ICON,
            size,
            size,
            0,
        ) as HICON
    }
}

// 任务栏/标题栏图标：显式 WM_SETICON，不依赖 nwg 的图标句柄
pub unsafe fn force_window_icons(hwnd: HWND) {
    let small = load_embed_icon(16);
    let big = load_embed_icon(32);
    if !small.is_null() {
        SendMessageW(hwnd, WM_SETICON, ICON_SMALL as WPARAM, small as LPARAM);
    }
    if !big.is_null() {
        SendMessageW(hwnd, WM_SETICON, ICON_BIG as WPARAM, big as LPARAM);
    }
}

// 托盘图标：nwg 用 uID=0 + 父窗口句柄注册，这里 NIM_MODIFY 覆盖为嵌入 ico
pub unsafe fn force_tray_icon(hwnd: HWND) {
    let small = load_embed_icon(16);
    if small.is_null() {
        return;
    }
    let mut data: NOTIFYICONDATAW = std::mem::zeroed();
    data.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = 0;
    data.uFlags = NIF_ICON;
    data.hIcon = small;
    Shell_NotifyIconW(NIM_MODIFY, &mut data);
}

// 供 main 使用的消息常量 re-export
pub use winapi::um::winuser::{
    WM_CTLCOLOREDIT, WM_CTLCOLORSTATIC, WM_ERASEBKGND, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSELEAVE,
    WM_MOUSEMOVE, WM_NCHITTEST, WM_PAINT, WM_SIZE, WM_SETCURSOR, HTCAPTION, HTCLIENT,
};
pub type Msg = UINT;
pub type Wp = WPARAM;
pub type Lp = LPARAM;
pub type Lr = LRESULT;
pub type Dw = DWORD;
#![allow(unsafe_op_in_unsafe_fn)]
use super::*;
use std::result::Result::Ok;
use std::sync::OnceLock;
use windows::Win32::Graphics::GdiPlus::*;

pub const INK: COLORREF = COLORREF(0x00292119);
pub const MUTED: COLORREF = COLORREF(0x007d746b);
pub const BLUE: COLORREF = COLORREF(0x00ff680a);
pub const SIDE: COLORREF = COLORREF(0x00f8f6f3);
pub const WHITE: COLORREF = COLORREF(0x00ffffff);
pub const LINE: COLORREF = COLORREF(0x00e8e4df);

pub struct Theme {
    pub normal: HFONT,
    pub small: HFONT,
    pub heading: HFONT,
    pub title: HFONT,
    pub icons: HFONT,
    pub menu_font: HFONT,
    pub white: HBRUSH,
    pub side: HBRUSH,
}
impl Theme {
    pub unsafe fn new(scale: f32) -> Self {
        let font = |size: f32, weight: i32| {
            CreateFontW(
                -(size * scale).round() as i32,
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                DEFAULT_PITCH.0 as u32,
                w!("Microsoft YaHei"),
            )
        };
        Self {
            normal: font(16.0, 400),
            small: font(13.0, 400),
            heading: font(18.0, 600),
            title: font(28.0, 600),
            menu_font: font(14.0, 400),
            icons: CreateFontW(
                -(17.0 * scale).round() as i32,
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                0,
                w!("Segoe MDL2 Assets"),
            ),
            white: CreateSolidBrush(WHITE),
            side: CreateSolidBrush(SIDE),
        }
    }
}
impl Drop for Theme {
    fn drop(&mut self) {
        unsafe {
            for font in [
                self.normal,
                self.small,
                self.heading,
                self.title,
                self.icons,
                self.menu_font,
            ] {
                let _ = DeleteObject(font.into());
            }
            let _ = DeleteObject(self.white.into());
            let _ = DeleteObject(self.side.into());
        }
    }
}
// Painting must remain available during synchronous notifications emitted by native controls.
// This snapshot is independent of the mutably borrowed application model.
pub const PAINT_STATE: PCWSTR = w!("MouseSpeedSwitcher.PaintState");
pub struct PaintState {
    pub scale: f32,
    pub font: HFONT,
    pub small: HFONT,
    pub heading: HFONT,
    pub icons: HFONT,
    pub menu_font: HFONT,
    pub autostart: bool,
    pub white: HBRUSH,
    pub side: HBRUSH,
    pub rows: Vec<(String, String)>,
    pub devices: Vec<(String, String)>,
    pub status: HWND,
    pub saved: HWND,
    pub running: HWND,
    pub subtitle: HWND,
    pub dirty: bool,
    pub error: bool,
    pub paused: bool,
    pub selected: bool,
    pub scroll: i32,
}
impl PaintState {
    pub fn new(theme: &Theme, scale: f32) -> Self {
        Self {
            scale,
            font: theme.normal,
            small: theme.small,
            heading: theme.heading,
            icons: theme.icons,
            menu_font: theme.menu_font,
            autostart: false,
            white: theme.white,
            side: theme.side,
            rows: Vec::new(),
            devices: Vec::new(),
            status: HWND::default(),
            saved: HWND::default(),
            running: HWND::default(),
            subtitle: HWND::default(),
            dirty: false,
            error: false,
            paused: false,
            selected: false,
            scroll: 0,
        }
    }
    pub fn px(&self, value: i32) -> i32 {
        (value as f32 * self.scale).round() as i32
    }
}
pub unsafe fn paint_message(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> Option<LRESULT> {
    let ptr = GetPropW(hwnd, PAINT_STATE).0 as *const RefCell<PaintState>;
    if ptr.is_null() {
        return None;
    }
    let state = (*ptr).borrow();
    match message {
        WM_ERASEBKGND => {
            let dc = HDC(wp.0 as *mut _);
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
            fill(dc, &rect, WHITE);
            rect.right = state.px(270);
            fill(dc, &rect, SIDE);
            fill(
                dc,
                &RECT {
                    left: state.px(270),
                    right: state.px(960),
                    top: state.px(666),
                    bottom: state.px(666) + 1,
                },
                LINE,
            );
            Some(LRESULT(1))
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX => {
            let child = HWND(lp.0 as *mut _);
            let dc = HDC(wp.0 as *mut _);
            let mut point = POINT::default();
            let _ = ClientToScreen(child, &mut point);
            let _ = ScreenToClient(hwnd, &mut point);
            let side = point.x < state.px(270);
            let color = if child == state.status && state.error {
                COLORREF(0x004b3bc7)
            } else if (child == state.saved && state.dirty)
                || (child == state.running && state.paused)
            {
                COLORREF(0x000b8ab2)
            } else if child == state.saved && state.selected {
                COLORREF(0x00579f24)
            } else if child == state.status || child == state.subtitle || child == state.running {
                MUTED
            } else {
                INK
            };
            SetBkMode(dc, TRANSPARENT);
            SetBkColor(dc, if side { SIDE } else { WHITE });
            SetTextColor(dc, color);
            Some(LRESULT(if side {
                state.side.0 as isize
            } else {
                state.white.0 as isize
            }))
        }
        WM_MEASUREITEM => {
            let item = &mut *(lp.0 as *mut MEASUREITEMSTRUCT);
            if item.CtlType != ODT_MENU {
                return None;
            }
            item.itemWidth = state.px(150) as u32;
            item.itemHeight = state.px(34) as u32;
            Some(LRESULT(1))
        }
        WM_DRAWITEM => {
            let source = &*(lp.0 as *const DRAWITEMSTRUCT);
            let frame = Buffer::new(source.hDC, source.rcItem);
            let mut draw = *source;
            draw.hDC = frame.dc();
            if !frame.memory.is_invalid() {
                draw.rcItem = RECT {
                    left: 0,
                    top: 0,
                    right: source.rcItem.right - source.rcItem.left,
                    bottom: source.rcItem.bottom - source.rcItem.top,
                };
            }
            let item = &draw;
            if item.CtlType == ODT_MENU {
                super::tray::draw(&state, item);
            } else if item.CtlID == LIST as u32 {
                list(&state, item);
            } else if item.CtlID == CONNECTED as u32 {
                combo_item(&state, item);
            } else {
                button(&state, item);
            }
            Some(LRESULT(1))
        }
        _ => None,
    }
}
// Compose a complete control frame offscreen before presenting it to avoid a visible
// white clear between native and GDI+ drawing operations. Render into local buffer coordinates.
pub struct Buffer {
    target: HDC,
    memory: HDC,
    bitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
    rect: RECT,
}
impl Buffer {
    pub unsafe fn new(target: HDC, rect: RECT) -> Self {
        let memory = CreateCompatibleDC(Some(target));
        let bitmap = CreateCompatibleBitmap(
            target,
            (rect.right - rect.left).max(1),
            (rect.bottom - rect.top).max(1),
        );
        if memory.is_invalid() || bitmap.is_invalid() {
            if !memory.is_invalid() {
                let _ = DeleteDC(memory);
            }
            if !bitmap.is_invalid() {
                let _ = DeleteObject(bitmap.into());
            }
            return Self {
                target,
                memory: HDC::default(),
                bitmap: HBITMAP::default(),
                old_bitmap: HGDIOBJ::default(),
                rect,
            };
        }
        let old_bitmap = SelectObject(memory, bitmap.into());
        Self {
            target,
            memory,
            bitmap,
            old_bitmap,
            rect,
        }
    }
    pub fn dc(&self) -> HDC {
        if self.memory.is_invalid() {
            self.target
        } else {
            self.memory
        }
    }
}
impl Drop for Buffer {
    fn drop(&mut self) {
        unsafe {
            if !self.memory.is_invalid() {
                let r = self.rect;
                let _ = BitBlt(
                    self.target,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    Some(self.memory),
                    0,
                    0,
                    SRCCOPY,
                );
                SelectObject(self.memory, self.old_bitmap);
                let _ = DeleteObject(self.bitmap.into());
                let _ = DeleteDC(self.memory);
            }
        }
    }
}
pub const HOVER: PCWSTR = w!("MouseSpeedSwitcher.Hover");
pub unsafe fn fill(dc: HDC, rect: &RECT, color: COLORREF) {
    let brush = CreateSolidBrush(color);
    FillRect(dc, rect, brush);
    let _ = DeleteObject(brush.into());
}
pub unsafe fn init_graphics() -> bool {
    static STARTUP: OnceLock<usize> = OnceLock::new();
    let token = STARTUP.get_or_init(|| {
        let mut token = 0;
        let input = GdiplusStartupInput {
            GdiplusVersion: 1,
            ..Default::default()
        };
        if GdiplusStartup(&mut token, &input, std::ptr::null_mut()).0 != 0 {
            return 0;
        }
        token
    });
    *token != 0
}
// GDI+ smooths geometric edges; text remains native ClearType at the target resolution.
unsafe fn shape(dc: HDC, rect: RECT, diameter: i32, color: Option<COLORREF>, border: COLORREF) {
    if !init_graphics() {
        return;
    }
    let mut graphics = std::ptr::null_mut();
    if GdipCreateFromHDC(dc, &mut graphics).0 != 0 {
        return;
    }
    GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias);
    let mut path = std::ptr::null_mut();
    if GdipCreatePath(FillModeAlternate, &mut path).0 == 0 {
        let x = rect.left as f32 + 0.5;
        let y = rect.top as f32 + 0.5;
        let width = (rect.right - rect.left - 1).max(1) as f32;
        let height = (rect.bottom - rect.top - 1).max(1) as f32;
        let d = (diameter as f32).min(width).min(height).max(1.0);
        for (left, top, angle) in [
            (x, y, 180.0),
            (x + width - d, y, 270.0),
            (x + width - d, y + height - d, 0.0),
            (x, y + height - d, 90.0),
        ] {
            GdipAddPathArc(path, left, top, d, d, angle, 90.0);
        }
        GdipClosePathFigure(path);
        let argb = |color: COLORREF| {
            0xff000000 | ((color.0 & 0xff) << 16) | (color.0 & 0xff00) | ((color.0 >> 16) & 0xff)
        };
        if let Some(color) = color {
            let mut brush = std::ptr::null_mut();
            if GdipCreateSolidFill(argb(color), &mut brush).0 == 0 {
                GdipFillPath(graphics, brush.cast(), path);
                GdipDeleteBrush(brush.cast());
            }
        }
        let mut pen = std::ptr::null_mut();
        if GdipCreatePen1(argb(border), 1.0, UnitPixel, &mut pen).0 == 0 {
            GdipDrawPath(graphics, pen, path);
            GdipDeletePen(pen);
        }
        GdipDeletePath(path);
    }
    GdipDeleteGraphics(graphics);
}
pub unsafe fn rounded(dc: HDC, rect: RECT, diameter: i32, color: COLORREF, border: COLORREF) {
    shape(dc, rect, diameter, Some(color), border);
}
pub unsafe fn focus_ring(dc: HDC, rect: RECT, diameter: i32) {
    shape(dc, rect, diameter, None, BLUE);
}
pub unsafe fn keyboard_focus(hwnd: HWND) -> bool {
    send(hwnd, WM_QUERYUISTATE, 0, 0) as u32 & UISF_HIDEFOCUS == 0
}
pub unsafe fn text(
    dc: HDC,
    text: &str,
    mut rect: RECT,
    font: HFONT,
    color: COLORREF,
    format: DRAW_TEXT_FORMAT,
) {
    // Win10 DrawTextW may inspect the pointer even when cchText is zero.
    // An empty Vec<u16> exposes the non-dereferenceable sentinel 0x2.
    if text.is_empty() {
        return;
    }
    let old = SelectObject(dc, font.into());
    SetBkMode(dc, TRANSPARENT);
    SetTextColor(dc, color);
    let mut value: Vec<u16> = text.encode_utf16().collect();
    DrawTextW(dc, &mut value, &mut rect, format | DT_NOPREFIX);
    SelectObject(dc, old);
}

pub unsafe extern "system" fn control_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    kind: usize,
) -> LRESULT {
    if msg == WM_NCDESTROY {
        let _ = RemoveWindowSubclass(hwnd, Some(control_proc), 1);
        let _ = RemovePropW(hwnd, HOVER);
    }
    // Wheel input over settings belongs to the outer viewport. In particular a closed
    // combo or trackbar must not consume it by silently changing a setting.
    if msg == WM_MOUSEWHEEL {
        let root = GetAncestor(hwnd, GA_ROOT);
        if let Ok(pane) = GetDlgItem(Some(root), PANE as i32)
            && hwnd != pane
            && IsChild(pane, hwnd).as_bool()
            && !(kind == 5 && send(hwnd, CB_GETDROPPEDSTATE, 0, 0) != 0)
        {
            return SendMessageW(pane, msg, Some(wp), Some(lp));
        }
    }
    if GetDlgCtrlID(hwnd) as u16 == LIST
        && matches!(
            msg,
            WM_LBUTTONDOWN | WM_LBUTTONDBLCLK | WM_MOUSEMOVE | WM_LBUTTONUP
        )
    {
        let outside = !list_card_hit(hwnd, lp);
        if outside && matches!(msg, WM_LBUTTONDOWN | WM_LBUTTONDBLCLK) {
            return LRESULT(0);
        }
        if outside && msg == WM_MOUSEMOVE && wp.0 & 0x0001 /* MK_LBUTTON */ != 0 {
            return LRESULT(0);
        }
        if outside && msg == WM_LBUTTONUP {
            if GetCapture() == hwnd {
                let _ = ReleaseCapture();
            }
            return LRESULT(0);
        }
    }
    if msg == WM_LBUTTONDOWN {
        send(
            GetAncestor(hwnd, GA_ROOT),
            WM_CHANGEUISTATE,
            (UIS_SET | (UISF_HIDEFOCUS << 16)) as usize,
            0,
        );
    }
    if msg == WM_SETFOCUS && keyboard_focus(hwnd) {
        let root = GetAncestor(hwnd, GA_ROOT);
        let _ = PostMessageW(
            Some(root),
            FOCUS_VISIBLE,
            WPARAM(0),
            LPARAM(hwnd.0 as isize),
        );
    }
    // Hover only changes appearance on owner-drawn buttons, and only at entry/exit.
    // Static labels, containers and unchanged cursor motion must not trigger new frames.
    let hover_button = matches!(
        GetDlgCtrlID(hwnd) as u16,
        SAVE | APPLY | BIND | PAUSE | ASSOCIATION | DETAILS
    );
    if hover_button && msg == WM_MOUSEMOVE && GetPropW(hwnd, HOVER).is_invalid() {
        let _ = SetPropW(hwnd, HOVER, Some(HANDLE(std::ptr::dangling_mut())));
        let mut tracking = TRACKMOUSEEVENT {
            cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: hwnd,
            ..Default::default()
        };
        let _ = TrackMouseEvent(&mut tracking);
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
    if hover_button && msg == WM_MOUSELEAVE && !GetPropW(hwnd, HOVER).is_invalid() {
        let _ = RemovePropW(hwnd, HOVER);
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
    if matches!(msg, WM_SETFOCUS | WM_KILLFOCUS | WM_ENABLE) {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
    if (kind != 0 || hover_button) && msg == WM_ERASEBKGND {
        return LRESULT(1);
    }
    if kind == 5 && (msg == WM_ERASEBKGND || msg == WM_NCPAINT) {
        return LRESULT(1);
    }
    if (kind == 6 || kind == 7) && msg == WM_PAINT {
        let mut paint = PAINTSTRUCT::default();
        let target = BeginPaint(hwnd, &mut paint);
        let mut rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut rect);
        let frame = Buffer::new(target, rect);
        let dc = frame.dc();
        let ptr = GetPropW(GetAncestor(hwnd, GA_ROOT), PAINT_STATE).0 as *const RefCell<PaintState>;
        if !ptr.is_null() {
            status_label(&(*ptr).borrow(), hwnd, dc, rect, kind == 7);
        }
        drop(frame);
        let _ = EndPaint(hwnd, &paint);
        return LRESULT(0);
    }
    if kind == 5 && msg == WM_PAINT {
        let mut paint = PAINTSTRUCT::default();
        let dc = BeginPaint(hwnd, &mut paint);
        let mut rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut rect);
        let frame = Buffer::new(dc, rect);
        let dc = frame.dc();
        let root = GetAncestor(hwnd, GA_ROOT);
        let ptr = GetPropW(root, PAINT_STATE).0 as *const RefCell<PaintState>;
        if !ptr.is_null() {
            combo_field(&(*ptr).borrow(), hwnd, dc, rect);
        }
        drop(frame);
        let _ = EndPaint(hwnd, &paint);
        return LRESULT(0);
    }
    if kind == 4 {
        if matches!(msg, WM_COMMAND | WM_CTLCOLOREDIT | WM_CTLCOLORSTATIC) {
            return SendMessageW(GetAncestor(hwnd, GA_ROOT), msg, Some(wp), Some(lp));
        }
        if msg == WM_PAINT {
            let mut paint = PAINTSTRUCT::default();
            let dc = BeginPaint(hwnd, &mut paint);
            let mut rect = RECT::default();
            let _ = GetClientRect(hwnd, &mut rect);
            let frame = Buffer::new(dc, rect);
            let dc = frame.dc();
            fill(dc, &rect, WHITE);
            let color = if IsChild(hwnd, GetFocus()).as_bool() {
                BLUE
            } else {
                LINE
            };
            let radius = (rect.bottom as f32 / 38.0 * 10.0).round() as i32;
            rounded(dc, rect, radius, WHITE, color);
            drop(frame);
            let _ = EndPaint(hwnd, &paint);
            return LRESULT(0);
        }
        if msg == WM_ERASEBKGND {
            return LRESULT(1);
        }
    }
    if matches!(msg, WM_SETFOCUS | WM_KILLFOCUS)
        && let Ok(parent) = GetParent(hwnd)
    {
        let _ = InvalidateRect(Some(parent), None, false);
    }
    if kind == 2 && msg == WM_PAINT {
        let mut paint = PAINTSTRUCT::default();
        let dc = BeginPaint(hwnd, &mut paint);
        let mut rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut rect);
        let mut flags = 0;
        if !IsWindowEnabled(hwnd).as_bool() {
            flags |= ODS_DISABLED.0;
        }
        if GetFocus() == hwnd {
            flags |= ODS_FOCUS.0;
        }
        if send(hwnd, BM_GETSTATE, 0, 0) as u32 & BST_PUSHED != 0 {
            flags |= ODS_SELECTED.0;
        }
        let item = DRAWITEMSTRUCT {
            CtlType: ODT_BUTTON,
            CtlID: GetDlgCtrlID(hwnd) as u32,
            hwndItem: hwnd,
            hDC: dc,
            rcItem: rect,
            itemState: ODS_FLAGS(flags),
            ..Default::default()
        };
        if let Ok(parent) = GetParent(hwnd) {
            SendMessageW(
                parent,
                WM_DRAWITEM,
                Some(WPARAM(item.CtlID as usize)),
                Some(LPARAM((&item as *const DRAWITEMSTRUCT) as isize)),
            );
        }
        let _ = EndPaint(hwnd, &paint);
        return LRESULT(0);
    }
    if kind == 1 && msg == WM_PAINT {
        let mut paint = PAINTSTRUCT::default();
        let dc = BeginPaint(hwnd, &mut paint);
        let mut rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut rect);
        let frame = Buffer::new(dc, rect);
        let dc = frame.dc();
        fill(dc, &rect, WHITE);
        let scale = rect.bottom as f32 / 38.0;
        let inset = (12.0 * scale) as i32;
        let center = rect.bottom / 2;
        let width = (rect.right - 2 * inset).max(1);
        let value = send(hwnd, TBM_GETPOS, 0, 0) as i32;
        let thumb = inset + width * (value - 1) / 19;
        rounded(
            dc,
            RECT {
                left: inset,
                top: center - 2,
                right: rect.right - inset,
                bottom: center + 3,
            },
            5,
            LINE,
            LINE,
        );
        if thumb > inset && IsWindowEnabled(hwnd).as_bool() {
            rounded(
                dc,
                RECT {
                    left: inset,
                    top: center - 2,
                    right: thumb,
                    bottom: center + 3,
                },
                5,
                BLUE,
                BLUE,
            );
        }
        let radius = (9.0 * scale).round() as i32;
        rounded(
            dc,
            RECT {
                left: thumb - radius,
                top: center - radius,
                right: thumb + radius,
                bottom: center + radius,
            },
            radius * 2,
            WHITE,
            LINE,
        );
        if GetFocus() == hwnd && keyboard_focus(hwnd) {
            focus_ring(dc, rect, (10.0 * scale) as i32);
        }
        drop(frame);
        let _ = EndPaint(hwnd, &paint);
        return LRESULT(0);
    }
    // Repaint after the native trackbar updates its value; retain native keyboard/mouse semantics.
    let previous_position = if kind == 1 && msg == WM_MOUSEMOVE {
        Some(send(hwnd, TBM_GETPOS, 0, 0))
    } else {
        None
    };
    let result = DefSubclassProc(hwnd, msg, wp, lp);
    if kind != 0
        && matches!(
            msg,
            WM_LBUTTONDOWN
                | WM_KEYDOWN
                | WM_KEYUP
                | TBM_SETPOS
                | BM_SETCHECK
                | BM_CLICK
                | WM_LBUTTONUP
        )
    {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
    if previous_position.is_some_and(|old| old != send(hwnd, TBM_GETPOS, 0, 0)) {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
    result
}

pub unsafe fn button(app: &PaintState, item: &DRAWITEMSTRUCT) {
    let dc = item.hDC;
    let mut rect = item.rcItem;
    fill(dc, &rect, WHITE);
    let id = item.CtlID as u16;
    let disabled = item.itemState.0 & ODS_DISABLED.0 != 0;
    let down = item.itemState.0 & ODS_SELECTED.0 != 0;
    let hover = !GetPropW(item.hwndItem, HOVER).is_invalid();
    let label = get_text(item.hwndItem);
    let p = |v: i32| app.px(v);
    if id == PRECISION || id == STARTUP {
        let on = checked(item.hwndItem);
        let track = RECT {
            left: rect.right - p(47),
            right: rect.right - p(1),
            top: (rect.bottom - p(26)) / 2,
            bottom: (rect.bottom + p(26)) / 2,
        };
        let color = if disabled {
            LINE
        } else if on {
            BLUE
        } else {
            COLORREF(0x00d0ccc7)
        };
        rounded(dc, track, p(26), color, color);
        let left = if on {
            track.right - p(23)
        } else {
            track.left + p(3)
        };
        rounded(
            dc,
            RECT {
                left,
                top: track.top + p(3),
                right: left + p(20),
                bottom: track.bottom - p(3),
            },
            p(20),
            WHITE,
            WHITE,
        );
        rect.right = track.left - p(12);
        text(
            dc,
            &label,
            rect,
            app.font,
            if disabled { MUTED } else { INK },
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
    } else {
        let primary = id == SAVE;
        let disclosure = id == ASSOCIATION || id == DETAILS;
        let bg = if disabled {
            COLORREF(0x00f7f5f2)
        } else if primary {
            if down {
                COLORREF(0x00dc5000)
            } else if hover {
                COLORREF(0x00ff7b2e)
            } else {
                BLUE
            }
        } else if down || hover {
            COLORREF(0x00f7f4f0)
        } else {
            WHITE
        };
        rounded(
            dc,
            rect,
            p(10),
            bg,
            if primary || disclosure { bg } else { LINE },
        );
        if disclosure {
            rect.left += p(2);
            text(
                dc,
                label.split("  ").next().unwrap_or(&label),
                rect,
                app.heading,
                INK,
                DT_LEFT | DT_VCENTER | DT_SINGLELINE,
            );
            text(
                dc,
                if label.ends_with('›') { "⌄" } else { "⌃" },
                RECT {
                    left: rect.right - p(28),
                    ..rect
                },
                app.font,
                MUTED,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
        } else {
            text(
                dc,
                &label,
                rect,
                app.font,
                if disabled {
                    MUTED
                } else if primary {
                    WHITE
                } else {
                    INK
                },
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
        }
    }
    if item.itemState.0 & ODS_FOCUS.0 != 0 && keyboard_focus(item.hwndItem) {
        let mut focus = item.rcItem;
        let _ = InflateRect(&mut focus, -p(3), -p(3));
        focus_ring(dc, focus, p(8));
    }
}

pub unsafe fn list(app: &PaintState, item: &DRAWITEMSTRUCT) {
    fill(item.hDC, &item.rcItem, SIDE);
    let Some((_, label)) = app.rows.get(item.itemID as usize) else {
        return;
    };
    let p = |v: i32| app.px(v);
    let mut rect = item.rcItem;
    if history_header(&app.rows, item.itemID as usize) {
        fill(
            item.hDC,
            &RECT {
                left: rect.left + p(12),
                right: rect.right - p(12),
                top: rect.top + p(12),
                bottom: rect.top + p(12) + 1,
            },
            LINE,
        );
        text(
            item.hDC,
            "历史鼠标",
            RECT {
                left: rect.left + p(12),
                right: rect.right - p(12),
                top: rect.top + p(21),
                bottom: rect.top + p(43),
            },
            app.small,
            MUTED,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE,
        );
    }
    rect = card_rect(&app.rows, item.itemID as usize, app.scale, item.rcItem);
    let selection_rect = rect;
    if item.itemState.0 & ODS_SELECTED.0 != 0 {
        rounded(
            item.hDC,
            rect,
            p(12),
            COLORREF(0x00fce9db),
            COLORREF(0x00fce9db),
        );
    }
    let online = label.starts_with('●');
    let dot = RECT {
        left: rect.left + p(12),
        top: rect.top + p(20),
        right: rect.left + p(19),
        bottom: rect.top + p(27),
    };
    let color = if online { COLORREF(0x00579f24) } else { MUTED };
    rounded(item.hDC, dot, p(7), color, color);
    rect.left += p(30);
    rect.right -= p(8);
    let status_rect = RECT {
        top: rect.top + p(30),
        ..rect
    };
    rect.bottom = rect.top + p(29);
    text(
        item.hDC,
        if online { "已连接" } else { "未连接" },
        status_rect,
        app.small,
        MUTED,
        DT_LEFT | DT_SINGLELINE | DT_VCENTER,
    );
    text(
        item.hDC,
        label.trim_start_matches(['●', '○', ' ']),
        rect,
        app.font,
        if online { INK } else { MUTED },
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
    );
    if item.itemState.0 & ODS_FOCUS.0 != 0 && keyboard_focus(item.hwndItem) {
        focus_ring(item.hDC, selection_rect, p(12));
    }
}

pub fn card_rect(rows: &[(String, String)], index: usize, scale: f32, mut rect: RECT) -> RECT {
    let p = |v: i32| (v as f32 * scale).round() as i32;
    if history_header(rows, index) {
        rect.top += p(48);
    }
    rect.left += p(4);
    rect.right -= p(4);
    rect.top += p(4);
    rect.bottom -= p(4);
    rect
}
pub unsafe fn list_card_hit(hwnd: HWND, lp: LPARAM) -> bool {
    let point = POINT {
        x: (lp.0 as u16) as i16 as i32,
        y: ((lp.0 as usize >> 16) as u16) as i16 as i32,
    };
    let mut bounds = RECT::default();
    let _ = GetClientRect(hwnd, &mut bounds);
    if !PtInRect(&bounds, point).as_bool() {
        return false;
    }
    let hit = send(hwnd, LB_ITEMFROMPOINT, 0, lp.0) as u32;
    if hit >> 16 != 0 {
        return false;
    }
    let index = (hit & 0xffff) as usize;
    let ptr = GetPropW(GetAncestor(hwnd, GA_ROOT), PAINT_STATE).0 as *const RefCell<PaintState>;
    if ptr.is_null() {
        return false;
    }
    let state = (*ptr).borrow();
    if index >= state.rows.len() {
        return false;
    }
    let mut row = RECT::default();
    if send(
        hwnd,
        LB_GETITEMRECT,
        index,
        (&mut row as *mut RECT) as isize,
    ) == LB_ERR as isize
    {
        return false;
    }
    PtInRect(&card_rect(&state.rows, index, state.scale, row), point).as_bool()
}
pub fn history_header(rows: &[(String, String)], index: usize) -> bool {
    rows.get(index)
        .is_some_and(|(_, label)| label.starts_with('○'))
        && (index == 0 || rows[index - 1].1.starts_with('●'))
}
pub unsafe fn combo_field(app: &PaintState, hwnd: HWND, dc: HDC, rect: RECT) {
    fill(dc, &rect, WHITE);
    let p = |v| app.px(v);
    rounded(
        dc,
        rect,
        p(10),
        WHITE,
        if GetFocus() == hwnd && keyboard_focus(hwnd) {
            BLUE
        } else {
            LINE
        },
    );
    let index = send(hwnd, CB_GETCURSEL, 0, 0);
    let label = usize::try_from(index)
        .ok()
        .and_then(|i| app.devices.get(i))
        .map(|(_, s)| s.as_str())
        .unwrap_or("暂无已连接设备");
    let name = label.split(" [").next().unwrap_or(label);
    text(
        dc,
        name,
        RECT {
            left: rect.left + p(12),
            right: rect.right - p(32),
            ..rect
        },
        app.font,
        if IsWindowEnabled(hwnd).as_bool() {
            INK
        } else {
            MUTED
        },
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
    );
    text(
        dc,
        "⌄",
        RECT {
            left: rect.right - p(30),
            right: rect.right - p(8),
            ..rect
        },
        app.font,
        MUTED,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
}
pub unsafe fn combo_item(app: &PaintState, item: &DRAWITEMSTRUCT) {
    if item.itemID == u32::MAX || item.itemState.0 & ODS_COMBOBOXEDIT.0 != 0 {
        combo_field(app, item.hwndItem, item.hDC, item.rcItem);
        return;
    }
    let selected = item.itemState.0 & ODS_SELECTED.0 != 0;
    fill(item.hDC, &item.rcItem, WHITE);
    let p = |v| app.px(v);
    let mut rect = item.rcItem;
    rect.left += p(4);
    rect.right -= p(4);
    rect.top += p(3);
    rect.bottom -= p(3);
    if selected {
        rounded(
            item.hDC,
            rect,
            p(8),
            COLORREF(0x00fce9db),
            COLORREF(0x00fce9db),
        );
    }
    if let Some((_, label)) = app.devices.get(item.itemID as usize) {
        let (name, identity) = label.split_once(" [").unwrap_or((label, ""));
        rect.left += p(9);
        rect.right -= p(9);
        let title = RECT {
            bottom: rect.top + p(25),
            ..rect
        };
        text(
            item.hDC,
            name,
            title,
            app.font,
            INK,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
        rect.top += p(26);
        text(
            item.hDC,
            identity.trim_end_matches(']'),
            rect,
            app.small,
            MUTED,
            DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS,
        );
    }
}

pub unsafe fn status_label(app: &PaintState, hwnd: HWND, dc: HDC, rect: RECT, running: bool) {
    let p = |v| app.px(v);
    let value = get_text(hwnd);
    fill(dc, &rect, if running { SIDE } else { WHITE });
    let (first, rest) = value.split_once("\r\n").unwrap_or((&value, ""));
    let green = COLORREF(0x00579f24);
    let color = if running && app.paused {
        COLORREF(0x000b8ab2)
    } else if running || value.starts_with('●') {
        green
    } else {
        MUTED
    };
    if running {
        let mut measured = RECT {
            left: 0,
            top: 0,
            right: (rect.right - p(28)).max(1),
            bottom: 0,
        };
        let old_font = SelectObject(dc, app.small.into());
        let mut encoded: Vec<u16> = rest.encode_utf16().collect();
        if !encoded.is_empty() {
            DrawTextW(
                dc,
                &mut encoded,
                &mut measured,
                DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX,
            );
        }
        SelectObject(dc, old_font);
        let offset = (rect.bottom - p(30) - measured.bottom).max(0);
        let icon = RECT {
            left: 0,
            top: offset + p(4),
            right: p(19),
            bottom: offset + p(23),
        };
        rounded(dc, icon, p(19), SIDE, color);
        text(
            dc,
            if app.paused { "Ⅱ" } else { "✓" },
            icon,
            app.small,
            color,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE,
        );
        text(
            dc,
            first.trim_start_matches(['✓', 'Ⅱ', ' ']),
            RECT {
                left: p(28),
                top: offset,
                right: rect.right,
                bottom: offset + p(28),
            },
            app.font,
            INK,
            DT_LEFT | DT_SINGLELINE | DT_VCENTER,
        );
        text(
            dc,
            rest,
            RECT {
                left: p(28),
                top: offset + p(30),
                ..rect
            },
            app.small,
            MUTED,
            DT_LEFT | DT_WORDBREAK,
        );
    } else if value.starts_with(['●', '○']) {
        let dot = RECT {
            left: p(1),
            top: p(12),
            right: p(9),
            bottom: p(20),
        };
        rounded(dc, dot, p(8), color, color);
        text(
            dc,
            value.trim_start_matches(['●', '○', ' ']),
            RECT {
                left: p(17),
                ..rect
            },
            app.font,
            MUTED,
            DT_LEFT | DT_SINGLELINE | DT_VCENTER,
        );
    } else {
        text(dc, &value, rect, app.font, MUTED, DT_LEFT | DT_WORDBREAK);
    }
}

#![allow(unsafe_op_in_unsafe_fn)]
mod theme;
mod tray;
use crate::{
    model::*,
    platform,
    storage::wide,
    worker::{Background, Command, View},
};
use std::{cell::RefCell, mem::size_of, time::Duration};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{LibraryLoader::GetModuleHandleW, Threading::CreateMutexW},
        UI::{Controls::*, HiDpi::*, Shell::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, w},
};

const CLASS: PCWSTR = w!("MouseSpeedSwitcher.Settings.v1");
const TRAY: u32 = WM_APP + 20;
const SHOW: u32 = WM_APP + 21;
// TBM_GETPOS = WM_USER, omitted from the Windows metadata used by windows 0.61.
const TBM_GETPOS: u32 = WM_USER;
const LIST: u16 = 101;
const SPEED: u16 = 102;
const SAVE: u16 = 103;
const APPLY: u16 = 104;
const BIND: u16 = 105;
const PAUSE: u16 = 106;
const STARTUP: u16 = 107;
const EXIT: u16 = 108;
const OPEN: u16 = 109;
const PRECISION: u16 = 110;
const ASSOCIATION: u16 = 111;
const DETAILS: u16 = 112;
const EDIT_NAME: u16 = 113;
const EDIT_FRAME: u16 = 114;
const CONNECTED: u16 = 115;
const SUBTITLE: u16 = 116;
const RUNNING: u16 = 117;
const PANE: u16 = 118;
const FOCUS_VISIBLE: u32 = WM_APP + 22;
const REPAINT: u32 = WM_APP + 23;

pub fn error_box(error: &str) {
    unsafe {
        let text = wide(error);
        MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            w!("Mouse Speed Switcher"),
            MB_OK | MB_ICONERROR,
        );
    }
}
unsafe fn set_text(hwnd: HWND, text: &str) {
    if get_text(hwnd) == text {
        return;
    }
    let s = wide(text);
    let _ = SetWindowTextW(hwnd, PCWSTR(s.as_ptr()));
}
unsafe fn get_text(hwnd: HWND) -> String {
    let mut data = vec![0u16; GetWindowTextLengthW(hwnd) as usize + 1];
    let n = GetWindowTextW(hwnd, &mut data);
    String::from_utf16_lossy(&data[..n as usize])
}
unsafe fn send(hwnd: HWND, message: u32, wp: usize, lp: isize) -> isize {
    SendMessageW(hwnd, message, Some(WPARAM(wp)), Some(LPARAM(lp))).0
}
unsafe fn checked(hwnd: HWND) -> bool {
    send(hwnd, BM_GETCHECK, 0, 0) == BST_CHECKED.0 as isize
}
unsafe fn check(hwnd: HWND, value: bool) {
    send(
        hwnd,
        BM_SETCHECK,
        if value {
            BST_CHECKED.0 as usize
        } else {
            BST_UNCHECKED.0 as usize
        },
        0,
    );
    let _ = InvalidateRect(Some(hwnd), None, false);
}

unsafe fn layout_scale() -> f32 {
    let dpi = GetDpiForSystem() as f32 / 96.0;
    let mut area = RECT::default();
    if SystemParametersInfoW(
        SPI_GETWORKAREA,
        0,
        Some((&mut area as *mut RECT).cast()),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    )
    .is_ok()
    {
        dpi.min((area.bottom - area.top - 64) as f32 / 740.0)
            .min((area.right - area.left - 40) as f32 / 960.0)
            .max(0.5)
    } else {
        dpi
    }
}

struct App {
    hwnd: HWND,
    _theme: theme::Theme,
    tray_icon: tray::Icon,
    paint: Box<RefCell<theme::PaintState>>,
    scale: f32,
    pane: HWND,
    title: HWND,
    subtitle: HWND,
    running: HWND,
    saved: HWND,
    association: HWND,
    detail_toggle: HWND,
    association_help: HWND,
    bind: HWND,
    save: HWND,
    apply: HWND,
    association_open: bool,
    details_open: bool,
    scroll: i32,
    wheel_remainder: i32,
    thumb_tracking: bool,
    last_track: Option<i32>,
    scroll_extent: Option<(i32, u32)>,
    content: Vec<(HWND, i32, i32, i32, i32)>,
    render_signature: String,
    background: Option<Background>,
    list: HWND,
    alias: HWND,
    speed: HWND,
    speed_label: HWND,
    precision: HWND,
    details: HWND,
    connected: HWND,
    status: HWND,
    pause: HWND,
    startup: HWND,
    rows: Vec<(String, String)>,
    device_rows: Vec<(String, String)>,
    selected: Option<String>,
    editing: Option<Parameters>,
    taskbar_created: u32,
}
impl App {
    fn px(&self, value: i32) -> i32 {
        (value as f32 * self.scale).round() as i32
    }
    unsafe fn controls(hwnd: HWND) -> Result<Self, String> {
        let scale = layout_scale();
        let px = |v: i32| (v as f32 * scale).round() as i32;
        let theme = theme::Theme::new(scale);
        let font = theme.normal;
        let create = |parent: HWND,
                      class: PCWSTR,
                      text: &str,
                      style: u32,
                      id: u16,
                      x,
                      y,
                      width,
                      height,
                      f: HFONT|
         -> Result<HWND, String> {
            let text = wide(text);
            let child = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                PCWSTR(text.as_ptr()),
                WS_CHILD | WS_VISIBLE | WINDOW_STYLE(style),
                px(x),
                px(y),
                px(width),
                px(height),
                Some(parent),
                Some(HMENU(id as usize as *mut _)),
                None,
                None,
            )
            .map_err(|e| e.to_string())?;
            send(child, WM_SETFONT, f.0 as usize, 1);
            let kind = if id == SUBTITLE {
                6
            } else if id == RUNNING {
                7
            } else if id == CONNECTED {
                5
            } else if id == EDIT_FRAME {
                4
            } else if id == PRECISION || id == STARTUP {
                2
            } else {
                0
            };
            let _ = SetWindowSubclass(child, Some(theme::control_proc), 1, kind);
            Ok(child)
        };
        create(
            hwnd,
            w!("STATIC"),
            "我的鼠标",
            0,
            0,
            22,
            24,
            225,
            30,
            theme.heading,
        )?;
        let list = create(
            hwnd,
            w!("LISTBOX"),
            "",
            WS_TABSTOP.0
                | WS_VSCROLL.0
                | LBS_NOTIFY as u32
                | LBS_OWNERDRAWVARIABLE as u32
                | LBS_HASSTRINGS as u32,
            LIST,
            12,
            70,
            244,
            476,
            font,
        )?;

        let running = create(
            hwnd,
            w!("STATIC"),
            "正在初始化…",
            0,
            RUNNING,
            22,
            624,
            224,
            96,
            font,
        )?;
        let pane = create(
            hwnd,
            w!("STATIC"),
            "",
            WS_CLIPCHILDREN.0 | WS_VSCROLL.0,
            PANE,
            270,
            0,
            690,
            548,
            font,
        )?;
        SetWindowLongPtrW(pane, GWL_EXSTYLE, WS_EX_CONTROLPARENT.0 as isize);
        let _ = SetWindowSubclass(pane, Some(pane_proc), 1, hwnd.0 as usize);
        let mut content = Vec::new();
        let mut field = |class, text: &str, style, id, x, y, w, h, f| -> Result<HWND, String> {
            let child = create(pane, class, text, style, id, x, y, w, h, f)?;
            content.push((child, x, y, w, h));
            Ok(child)
        };
        let title = field(
            w!("STATIC"),
            "鼠标配置",
            0x4000,
            0,
            30,
            28,
            606,
            42,
            theme.title,
        )?;
        let subtitle = field(
            w!("STATIC"),
            "正在识别设备…",
            0,
            SUBTITLE,
            30,
            80,
            606,
            36,
            font,
        )?;
        field(
            w!("STATIC"),
            "指针设置",
            0,
            0,
            30,
            138,
            300,
            28,
            theme.heading,
        )?;
        let speed_label = field(
            w!("STATIC"),
            "10 / 20",
            2,
            0,
            466,
            182,
            170,
            28,
            theme.heading,
        )?;
        field(w!("STATIC"), "指针速度", 0, 0, 30, 182, 300, 28, font)?;
        let speed = field(
            TRACKBAR_CLASSW,
            "",
            WS_TABSTOP.0 | TBS_NOTICKS | TBS_FIXEDLENGTH,
            SPEED,
            24,
            216,
            618,
            38,
            font,
        )?;
        send(speed, TBM_SETRANGE, 1, 1 | (20 << 16));
        send(speed, TBM_SETPOS, 1, 10);
        let _ = SetWindowSubclass(speed, Some(theme::control_proc), 1, 1);
        field(w!("STATIC"), "慢", 0, 0, 36, 256, 30, 22, theme.small)?;
        field(w!("STATIC"), "快", 2, 0, 606, 256, 30, 22, theme.small)?;
        let precision = field(
            w!("BUTTON"),
            "提高指针精确度",
            WS_TABSTOP.0 | BS_AUTOCHECKBOX as u32,
            PRECISION,
            30,
            296,
            606,
            38,
            font,
        )?;
        field(
            w!("STATIC"),
            "根据移动速度调整指针响应",
            0,
            0,
            30,
            340,
            606,
            24,
            theme.small,
        )?;
        field(w!("STATIC"), "配置名称", 0, 0, 30, 376, 606, 26, font)?;
        let edit_frame = field(
            w!("STATIC"),
            "",
            WS_CLIPCHILDREN.0,
            EDIT_FRAME,
            30,
            410,
            606,
            38,
            font,
        )?;
        let alias = create(
            edit_frame,
            w!("EDIT"),
            "",
            WS_TABSTOP.0 | ES_AUTOHSCROLL as u32,
            EDIT_NAME,
            10,
            7,
            586,
            24,
            font,
        )?;
        send(alias, EM_SETLIMITTEXT, 100, 0);
        send(
            alias,
            EM_SETMARGINS,
            (EC_LEFTMARGIN | EC_RIGHTMARGIN) as usize,
            (10 | (10 << 16)) as isize,
        );
        let association = create(
            pane,
            w!("BUTTON"),
            "设备关联  ›",
            WS_TABSTOP.0 | BS_OWNERDRAW as u32,
            ASSOCIATION,
            30,
            486,
            606,
            40,
            theme.heading,
        )?;
        let association_help = create(
            pane,
            w!("STATIC"),
            "更换端口后，可将已连接设备关联到选中配置。",
            0,
            0,
            30,
            536,
            606,
            42,
            theme.small,
        )?;
        let connected = create(
            pane,
            w!("COMBOBOX"),
            "",
            WS_TABSTOP.0
                | WS_VSCROLL.0
                | CBS_DROPDOWNLIST as u32
                | CBS_OWNERDRAWFIXED as u32
                | CBS_HASSTRINGS as u32,
            CONNECTED,
            30,
            578,
            388,
            220,
            font,
        )?;
        send(connected, CB_SETITEMHEIGHT, usize::MAX, px(32) as isize);
        send(connected, CB_SETITEMHEIGHT, 0, px(58) as isize);
        send(connected, CB_SETDROPPEDWIDTH, px(500) as usize, 0);
        let bind = create(
            pane,
            w!("BUTTON"),
            "关联到选中配置",
            WS_TABSTOP.0 | BS_OWNERDRAW as u32,
            BIND,
            434,
            578,
            202,
            36,
            font,
        )?;
        let detail_toggle = create(
            pane,
            w!("BUTTON"),
            "设备详情  ›",
            WS_TABSTOP.0 | BS_OWNERDRAW as u32,
            DETAILS,
            30,
            540,
            606,
            40,
            theme.heading,
        )?;
        let details = create(
            pane,
            w!("EDIT"),
            "",
            WS_BORDER.0
                | ES_MULTILINE as u32
                | ES_READONLY as u32
                | ES_AUTOVSCROLL as u32
                | WS_VSCROLL.0
                | WS_HSCROLL.0
                | ES_AUTOHSCROLL as u32
                | WS_TABSTOP.0,
            0,
            30,
            590,
            606,
            154,
            theme.small,
        )?;
        let saved = create(
            hwnd,
            w!("STATIC"),
            "选择配置后开始编辑",
            0,
            0,
            300,
            574,
            240,
            26,
            theme.small,
        )?;
        let apply = create(
            hwnd,
            w!("BUTTON"),
            "应用已保存配置",
            WS_TABSTOP.0 | BS_OWNERDRAW as u32,
            APPLY,
            560,
            568,
            190,
            40,
            font,
        )?;
        let save = create(
            hwnd,
            w!("BUTTON"),
            "保存配置",
            WS_TABSTOP.0 | BS_OWNERDRAW as u32,
            SAVE,
            766,
            568,
            164,
            40,
            font,
        )?;
        create(
            hwnd,
            w!("STATIC"),
            "应用已保存配置不会保存当前修改",
            0,
            0,
            560,
            614,
            370,
            22,
            theme.small,
        )?;
        let status = create(
            hwnd,
            w!("EDIT"),
            "正在初始化设备…",
            ES_MULTILINE as u32 | ES_READONLY as u32 | ES_AUTOVSCROLL as u32 | WS_VSCROLL.0,
            0,
            300,
            640,
            630,
            22,
            theme.small,
        )?;
        let pause = create(
            hwnd,
            w!("BUTTON"),
            "暂停自动切换",
            WS_TABSTOP.0 | BS_OWNERDRAW as u32,
            PAUSE,
            300,
            675,
            158,
            30,
            font,
        )?;
        let startup = create(
            hwnd,
            w!("BUTTON"),
            "开机自启",
            WS_TABSTOP.0 | BS_AUTOCHECKBOX as u32,
            STARTUP,
            758,
            687,
            172,
            34,
            font,
        )?;
        create(
            hwnd,
            w!("STATIC"),
            "关闭窗口后仍在托盘运行",
            0,
            0,
            300,
            711,
            420,
            22,
            theme.small,
        )?;
        check(startup, platform::autostart());
        let paint = Box::new(RefCell::new(theme::PaintState::new(&theme, scale)));
        let _ = SetPropW(
            hwnd,
            theme::PAINT_STATE,
            Some(HANDLE(
                (&*paint as *const RefCell<theme::PaintState>) as *mut _,
            )),
        );
        let mut app = Self {
            hwnd,
            _theme: theme,
            tray_icon: tray::Icon::new(),
            paint,
            scale,
            pane,
            title,
            subtitle,
            running,
            saved,
            association,
            detail_toggle,
            association_help,
            bind,
            save,
            apply,
            association_open: false,
            details_open: false,
            scroll: 0,
            wheel_remainder: 0,
            thumb_tracking: false,
            last_track: None,
            scroll_extent: None,
            content,
            render_signature: String::new(),
            background: None,
            list,
            alias,
            speed,
            speed_label,
            precision,
            details,
            connected,
            status,
            pause,
            startup,
            rows: Vec::new(),
            device_rows: Vec::new(),
            selected: None,
            editing: None,
            taskbar_created: RegisterWindowMessageW(w!("TaskbarCreated")),
        };
        app.sync_paint(None);
        app.layout();
        Ok(app)
    }
    unsafe fn sync_paint(&self, view: Option<&View>) {
        let dirty = view.is_some_and(|v| self.dirty(v));
        let mut paint = self.paint.borrow_mut();
        paint.rows.clone_from(&self.rows);
        paint.devices.clone_from(&self.device_rows);
        paint.autostart = checked(self.startup);
        paint.status = self.status;
        paint.saved = self.saved;
        paint.running = self.running;
        paint.subtitle = self.subtitle;
        paint.dirty = dirty;
        paint.error = view.is_some_and(|v| v.error.is_some());
        paint.paused = view.is_some_and(|v| v.paused);
        paint.selected = self.selected.is_some();
        paint.scroll = self.scroll;
    }
    unsafe fn schedule_paint(&self) {
        let _ = PostMessageW(Some(self.hwnd), REPAINT, WPARAM(0), LPARAM(0));
    }
    unsafe fn place(&self, hwnd: HWND, x: i32, y: i32, w: i32, h: i32) {
        let _ = MoveWindow(
            hwnd,
            self.px(x),
            self.px(y - self.scroll),
            self.px(w),
            self.px(h),
            true,
        );
    }
    unsafe fn layout(&mut self) {
        let details_y = if self.association_open { 610 } else { 512 };
        let total = details_y + if self.details_open { 218 } else { 36 };
        self.scroll = self.scroll.clamp(0, (total - 548).max(0));
        for &(hwnd, x, y, w, h) in &self.content {
            self.place(hwnd, x, y, w, h);
        }
        self.place(self.association, 30, 458, 606, 40);
        self.place(self.association_help, 30, 508, 606, 36);
        self.place(self.connected, 30, 552, 388, 220);
        self.place(self.bind, 434, 550, 202, 38);
        self.place(self.detail_toggle, 30, details_y, 606, 40);
        self.place(self.details, 30, details_y + 50, 606, 154);
        for (child, visible) in [
            (self.association_help, self.association_open),
            (self.connected, self.association_open),
            (self.bind, self.association_open),
            (self.details, self.details_open),
        ] {
            let shown = GetWindowLongPtrW(child, GWL_STYLE) as u32 & WS_VISIBLE.0 != 0;
            if shown != visible {
                let _ = ShowWindow(child, if visible { SW_SHOW } else { SW_HIDE });
            }
        }
        let extent = (total, 548);
        let changed = self.scroll_extent != Some(extent);
        if changed || !self.thumb_tracking {
            let info = SCROLLINFO {
                cbSize: size_of::<SCROLLINFO>() as u32,
                fMask: if changed {
                    SIF_RANGE | SIF_PAGE | SIF_POS
                } else {
                    SIF_POS
                },
                nMin: 0,
                nMax: total - 1,
                nPage: 548,
                nPos: self.scroll,
                ..Default::default()
            };
            SetScrollInfo(self.pane, SB_VERT, &info, true);
            self.scroll_extent = Some(extent);
        }
        set_text(
            self.association,
            if self.association_open {
                "设备关联  ⌃"
            } else {
                "设备关联  ›"
            },
        );
        set_text(
            self.detail_toggle,
            if self.details_open {
                "设备详情  ⌃"
            } else {
                "设备详情  ›"
            },
        );
        self.paint.borrow_mut().scroll = self.scroll;
        let _ = InvalidateRect(Some(self.pane), None, true);
        self.schedule_paint();
    }
    unsafe fn wheel(&mut self, delta: i32) {
        self.wheel_remainder += delta;
        let steps = self.wheel_remainder / 120;
        self.wheel_remainder %= 120;
        if steps == 0 {
            return;
        }
        let mut lines = 3u32;
        let _ = SystemParametersInfoW(
            SPI_GETWHEELSCROLLLINES,
            0,
            Some((&mut lines as *mut u32).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );
        let amount = if lines == u32::MAX {
            520
        } else {
            lines.min(1000) as i32 * 28
        };
        self.scroll = self.scroll.saturating_sub(steps.saturating_mul(amount));
        self.layout();
    }
    unsafe fn scroll_bar(&mut self, code: u32, thumb: i32) {
        match code {
            0 => self.scroll -= 28,
            1 => self.scroll += 28,
            2 => self.scroll -= 520,
            3 => self.scroll += 520,
            5 => {
                self.thumb_tracking = true;
                self.last_track = Some(thumb);
                self.scroll = thumb;
            }
            4 => {
                self.scroll = self.last_track.take().unwrap_or(thumb);
                self.thumb_tracking = false;
            }
            6 => self.scroll = 0,
            7 => self.scroll = i32::MAX,
            8 => {
                self.thumb_tracking = false;
                self.last_track = None;
            }
            _ => return,
        }
        self.layout();
    }
    unsafe fn reveal(&mut self, child: HWND) {
        let child = if GetParent(child).ok() == Some(self.pane) {
            child
        } else {
            GetParent(child).unwrap_or(child)
        };
        if GetParent(child).ok() != Some(self.pane) {
            return;
        }
        let mut rect = RECT::default();
        let _ = GetWindowRect(child, &mut rect);
        let mut top = POINT {
            x: rect.left,
            y: rect.top,
        };
        let _ = ScreenToClient(self.pane, &mut top);
        let height = rect.bottom - rect.top;
        let adjustment = if top.y < 0 {
            top.y
        } else if top.y + height > self.px(548) {
            top.y + height - self.px(548)
        } else {
            0
        };
        if adjustment != 0 {
            self.scroll += (adjustment as f32 / self.scale).ceil() as i32;
            self.layout();
        }
    }
    unsafe fn dirty(&self, view: &View) -> bool {
        self.selected
            .as_ref()
            .and_then(|id| view.config.profiles.get(id))
            .is_some_and(|profile| {
                get_text(self.alias) != profile.alias
                    || send(self.speed, TBM_GETPOS, 0, 0) as i32 != profile.parameters.speed
                    || checked(self.precision) != profile.parameters.precision()
            })
    }
    unsafe fn update_saved(&self, view: &View) {
        let selected = self
            .selected
            .as_ref()
            .is_some_and(|id| view.config.profiles.contains_key(id));
        for (control, enabled) in [
            (self.alias, selected),
            (self.speed, selected),
            (self.precision, selected),
            (self.save, selected),
            (self.apply, selected),
            (self.bind, selected && !self.device_rows.is_empty()),
            (self.connected, !self.device_rows.is_empty()),
        ] {
            if IsWindowEnabled(control).as_bool() != enabled {
                let _ = EnableWindow(control, enabled);
            }
        }
        set_text(
            self.saved,
            if !selected {
                "尚无配置"
            } else if self.dirty(view) {
                "● 尚未保存"
            } else {
                "✓ 已保存"
            },
        );
    }
    fn view(&self) -> View {
        self.background
            .as_ref()
            .unwrap()
            .view
            .lock()
            .unwrap()
            .clone()
    }
    fn command(&self, c: Command) {
        if let Some(bg) = &self.background {
            if bg.is_preview() {
                let mut view = bg.view.lock().unwrap();
                match c {
                    Command::Save {
                        id,
                        alias,
                        parameters,
                    } => {
                        if alias.trim().is_empty() {
                            view.error = Some("配置名称不能为空".into());
                            return;
                        }
                        if let Some(profile) = view.config.profiles.get_mut(&id) {
                            profile.alias = alias;
                            profile.parameters = parameters;
                        }
                        view.notice = "预览：配置已保存（仅内存）".into();
                    }
                    Command::Apply(id) => {
                        view.active = Some(id);
                        view.notice = "预览：已应用保存值（未修改系统）".into();
                    }
                    Command::Bind { device, profile } => {
                        view.config.bindings.insert(device, profile);
                        view.notice = "预览：关联成功（仅内存）".into();
                    }
                    Command::Pause => {
                        view.paused = !view.paused;
                    }
                    Command::Shutdown => {}
                }
                view.error = None;
            } else {
                bg.send(c);
            }
        }
    }
    unsafe fn refresh(&mut self) {
        if self.background.is_none() {
            return;
        }
        let view = self.view();
        let mut rows: Vec<_> = view
            .config
            .profiles
            .iter()
            .map(|(id, p)| {
                let online = view
                    .connected
                    .keys()
                    .any(|key| view.config.bindings.get(key) == Some(id));
                (
                    id.clone(),
                    format!("{} {}", if online { "●" } else { "○" }, p.alias),
                )
            })
            .collect();
        rows.sort_by_key(|(_, label)| (!label.starts_with('●'), label.clone()));
        if rows != self.rows {
            self.rows = rows;
            self.sync_paint(Some(&view));
            send(self.list, LB_RESETCONTENT, 0, 0);
            for (index, (_, label)) in self.rows.iter().enumerate() {
                let text = wide(label);
                send(self.list, LB_ADDSTRING, 0, text.as_ptr() as isize);
                let height = if theme::history_header(&self.rows, index) {
                    104
                } else {
                    56
                };
                send(self.list, LB_SETITEMHEIGHT, index, self.px(height) as isize);
            }
            if self
                .selected
                .as_ref()
                .is_none_or(|id| !view.config.profiles.contains_key(id))
            {
                self.selected = view
                    .active
                    .clone()
                    .or_else(|| self.rows.first().map(|r| r.0.clone()));
                self.load_editor(&view);
            }
            if let Some(i) = self
                .rows
                .iter()
                .position(|r| Some(&r.0) == self.selected.as_ref())
            {
                send(self.list, LB_SETCURSEL, i, 0);
            }
        }
        let devices: Vec<_> = view
            .connected
            .iter()
            .map(|(key, d)| (key.clone(), format!("{} [{}]", d.name, d.instance)))
            .collect();
        if devices != self.device_rows {
            let old = send(self.connected, CB_GETCURSEL, 0, 0);
            let old_key = usize::try_from(old)
                .ok()
                .and_then(|i| self.device_rows.get(i))
                .map(|r| r.0.clone());
            send(self.connected, CB_RESETCONTENT, 0, 0);
            for (_, label) in &devices {
                let text = wide(label);
                send(self.connected, CB_ADDSTRING, 0, text.as_ptr() as isize);
            }
            self.device_rows = devices;
            if !self.device_rows.is_empty() {
                let i = self
                    .device_rows
                    .iter()
                    .position(|r| Some(&r.0) == old_key.as_ref())
                    .unwrap_or(0);
                send(self.connected, CB_SETCURSEL, i, 0);
            }
        }
        self.update_saved(&view);
        self.sync_paint(Some(&view));
        if let Some(profile) = self
            .selected
            .as_ref()
            .and_then(|id| view.config.profiles.get(id))
        {
            set_text(self.title, &profile.alias);
            let online = view
                .connected
                .keys()
                .any(|key| view.config.bindings.get(key) == self.selected.as_ref());
            let subtitle = if !online {
                "○ 未连接 · 历史配置"
            } else if self.selected == view.active {
                "● 已连接 · 当前使用中"
            } else {
                "● 已连接 · 正在编辑此配置"
            };
            set_text(self.subtitle, subtitle);
        } else {
            set_text(self.title, "尚未发现可识别的鼠标");
            set_text(self.subtitle, "连接鼠标并移动后，将自动创建配置");
        }
        let name = view
            .active
            .as_ref()
            .and_then(|id| view.config.profiles.get(id))
            .map(|p| p.alias.as_str())
            .unwrap_or("等待鼠标输入");
        set_text(
            self.running,
            &format!(
                "{}\r\n当前配置：{}",
                if view.paused {
                    "Ⅱ 自动切换已暂停"
                } else {
                    "✓ 自动切换已开启"
                },
                name
            ),
        );
        let mut status = if view.paused {
            format!("已暂停；仍可手动保存或应用。{}", view.notice)
        } else {
            view.notice.clone()
        };
        if let Some(error) = &view.error {
            status = format!("提示：{error}\r\n{status}");
        }
        // Avoid resetting selection / scroll position in the status control every timer tick.
        if get_text(self.status) != status {
            set_text(self.status, &status);
            let _ = ShowScrollBar(
                self.status,
                SB_VERT,
                send(self.status, EM_GETLINECOUNT, 0, 0) > 1,
            );
        }
        let _ = ShowWindow(
            self.status,
            if status.is_empty() { SW_HIDE } else { SW_SHOW },
        );
        set_text(
            self.pause,
            if view.paused {
                "恢复自动切换"
            } else {
                "暂停自动切换"
            },
        );
        let signature = format!(
            "{:?}{:?}{:?}{:?}{:?}{}{}{}",
            self.rows,
            self.device_rows,
            self.selected,
            view.active,
            view.error,
            view.paused,
            view.notice,
            self.dirty(&view)
        );
        if signature != self.render_signature {
            self.render_signature = signature;
            self.schedule_paint();
        }
    }
    unsafe fn load_editor(&mut self, view: &View) {
        let Some(p) = self
            .selected
            .as_ref()
            .and_then(|id| view.config.profiles.get(id))
        else {
            self.editing = None;
            set_text(self.alias, "");
            set_text(self.details, "");
            send(self.speed, TBM_SETPOS, 1, 10);
            check(self.precision, false);
            self.update_speed();
            return;
        };
        self.editing = Some(p.parameters);
        set_text(self.alias, &p.alias);
        send(self.speed, TBM_SETPOS, 1, p.parameters.speed as isize);
        self.update_speed();
        check(self.precision, p.parameters.precision());
        set_text(
            self.details,
            &format!(
                "设备：{}\r\n实例：{}\r\nContainer：{}\r\n序列号：{}\r\n接口：{}",
                p.identity.name,
                p.identity.instance,
                p.identity.container.as_deref().unwrap_or("无"),
                p.identity.serial.as_deref().unwrap_or("系统未提供"),
                p.identity.interface
            ),
        );
    }
    unsafe fn update_speed(&self) {
        set_text(
            self.speed_label,
            &format!("{} / 20", send(self.speed, TBM_GETPOS, 0, 0)),
        );
    }
    unsafe fn on_command(&mut self, id: u16, notification: u16) {
        match id {
            LIST if notification == LBN_SELCHANGE as u16 => {
                let i = send(self.list, LB_GETCURSEL, 0, 0);
                self.selected = usize::try_from(i)
                    .ok()
                    .and_then(|i| self.rows.get(i))
                    .map(|r| r.0.clone());
                self.load_editor(&self.view());
            }
            SAVE => {
                if let (Some(id), Some(parameters)) = (self.selected.clone(), self.editing) {
                    let mut parameters = parameters.with_precision(checked(self.precision));
                    parameters.speed = send(self.speed, TBM_GETPOS, 0, 0) as i32;
                    self.command(Command::Save {
                        id,
                        alias: get_text(self.alias),
                        parameters,
                    });
                    self.editing = Some(parameters);
                }
            }
            APPLY => {
                if let Some(id) = &self.selected {
                    self.command(Command::Apply(id.clone()));
                }
            }
            BIND => {
                let i = send(self.connected, CB_GETCURSEL, 0, 0);
                if let (Some(device), Some(profile)) = (
                    usize::try_from(i)
                        .ok()
                        .and_then(|i| self.device_rows.get(i)),
                    &self.selected,
                ) {
                    self.command(Command::Bind {
                        device: device.0.clone(),
                        profile: profile.clone(),
                    });
                }
            }
            PAUSE => self.command(Command::Pause),
            STARTUP => {
                if self.background.as_ref().is_some_and(Background::is_preview) {
                    return;
                }
                let enable = !platform::autostart();
                if let Err(e) = platform::set_autostart(enable) {
                    error_box(&format!("开机自启设置失败：{e}"));
                }
                check(self.startup, platform::autostart());
            }
            OPEN => self.show(),
            EXIT => {
                let _ = DestroyWindow(self.hwnd);
            }
            PRECISION => {
                self.update_saved(&self.view());
            }
            EDIT_NAME if notification == EN_CHANGE as u16 => self.update_saved(&self.view()),
            ASSOCIATION => {
                self.association_open = !self.association_open;
                self.layout();
            }
            DETAILS => {
                self.details_open = !self.details_open;
                self.layout();
            }
            _ => {}
        }
    }
    unsafe fn show(&self) {
        let _ = ShowWindow(self.hwnd, SW_RESTORE);
        let _ = SetForegroundWindow(self.hwnd);
    }
    unsafe fn tray_data(&self) -> NOTIFYICONDATAW {
        let mut data = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: TRAY,
            hIcon: self.tray_icon.handle,
            ..Default::default()
        };
        let tip = wide("Mouse Speed Switcher");
        data.szTip[..tip.len()].copy_from_slice(&tip);
        data
    }
    unsafe fn add_tray(&self) -> bool {
        Shell_NotifyIconW(NIM_ADD, &self.tray_data()).as_bool()
    }
    unsafe fn menu(&self) {
        let enabled = if self.background.as_ref().is_some_and(Background::is_preview) {
            checked(self.startup)
        } else {
            platform::autostart()
        };
        check(self.startup, enabled);
        self.sync_paint(Some(&self.view()));
        let Ok(menu) = tray::create_menu(&self.paint.borrow()) else {
            return;
        };
        let mut point = POINT::default();
        let _ = GetCursorPos(&mut point);
        let _ = SetForegroundWindow(self.hwnd);
        // Return the command instead of dispatching it inside the nested menu message loop.
        let corners = tray::PopupCorners::install();
        let command = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_RETURNCMD,
            point.x,
            point.y,
            None,
            self.hwnd,
            None,
        );
        drop(corners);
        if command.0 != 0 {
            let _ = PostMessageW(
                Some(self.hwnd),
                WM_COMMAND,
                WPARAM(command.0 as usize),
                LPARAM(0),
            );
        }
        let _ = PostMessageW(Some(self.hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
    }
}
unsafe extern "system" fn pane_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
    _id: usize,
    root: usize,
) -> LRESULT {
    if msg == WM_NCDESTROY {
        let _ = RemoveWindowSubclass(hwnd, Some(pane_proc), 1);
    }
    if msg == WM_PAINT {
        let mut paint = PAINTSTRUCT::default();
        let dc = BeginPaint(hwnd, &mut paint);
        let mut rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut rect);
        let frame = theme::Buffer::new(dc, rect);
        let dc = frame.dc();
        theme::fill(dc, &rect, theme::WHITE);
        let ptr = GetPropW(HWND(root as *mut _), theme::PAINT_STATE).0
            as *const RefCell<theme::PaintState>;
        if !ptr.is_null()
            && let Ok(app) = (*ptr).try_borrow()
        {
            for y in [124, 284, 370, 504] {
                let top = app.px(y - app.scroll);
                theme::fill(
                    dc,
                    &RECT {
                        left: app.px(30),
                        right: app.px(636),
                        top,
                        bottom: top + 1,
                    },
                    theme::LINE,
                );
            }
        }
        drop(frame);
        let _ = EndPaint(hwnd, &paint);
        return LRESULT(0);
    }
    if msg == WM_ERASEBKGND {
        let mut rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut rect);
        theme::fill(HDC(wp.0 as *mut _), &rect, theme::WHITE);
        return LRESULT(1);
    }
    if matches!(
        msg,
        WM_COMMAND
            | WM_HSCROLL
            | WM_DRAWITEM
            | WM_CTLCOLORSTATIC
            | WM_CTLCOLOREDIT
            | WM_CTLCOLORLISTBOX
            | WM_VSCROLL
            | WM_MOUSEWHEEL
    ) {
        return SendMessageW(HWND(root as *mut _), msg, Some(wp), Some(lp));
    }
    DefSubclassProc(hwnd, msg, wp, lp)
}

unsafe extern "system" fn procedure(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if message == WM_NCDESTROY {
        let _ = RemovePropW(hwnd, theme::PAINT_STATE);
    }
    if message == REPAINT {
        let _ = RedrawWindow(
            Some(hwnd),
            None,
            None,
            RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN,
        );
        return LRESULT(0);
    }
    if let Some(result) = theme::paint_message(hwnd, message, wp, lp) {
        return result;
    }
    // Control notifications and modal menus can reenter this procedure. Never alias mutable App references.
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<App>;
    if !ptr.is_null()
        && let Ok(mut app) = (*ptr).try_borrow_mut()
    {
        match message {
            FOCUS_VISIBLE => {
                let child = HWND(lp.0 as *mut _);
                if GetFocus() == child && theme::keyboard_focus(child) {
                    app.reveal(child);
                }
                return LRESULT(0);
            }
            WM_MOUSEWHEEL => {
                app.wheel(((wp.0 >> 16) as u16) as i16 as i32);
                return LRESULT(0);
            }
            WM_VSCROLL => {
                let code = (wp.0 & 0xffff) as u32;
                let mut position = ((wp.0 >> 16) & 0xffff) as i32;
                if code == 5 {
                    let mut info = SCROLLINFO {
                        cbSize: size_of::<SCROLLINFO>() as u32,
                        fMask: SIF_TRACKPOS,
                        ..Default::default()
                    };
                    if GetScrollInfo(app.pane, SB_VERT, &mut info).is_ok()
                        && GetCapture() == app.pane
                    {
                        position = info.nTrackPos;
                    }
                }
                app.scroll_bar(code, position);
                return LRESULT(0);
            }
            WM_COMMAND => {
                let id = (wp.0 & 0xffff) as u16;
                if id == STARTUP
                    && lp.0 == 0
                    && app.background.as_ref().is_some_and(Background::is_preview)
                {
                    check(app.startup, !checked(app.startup));
                }
                app.on_command(id, ((wp.0 >> 16) & 0xffff) as u16);
                app.refresh();
                return LRESULT(0);
            }
            WM_HSCROLL => {
                app.update_speed();
                app.update_saved(&app.view());
                app.sync_paint(Some(&app.view()));
                app.schedule_paint();
                return LRESULT(0);
            }
            WM_TIMER => {
                app.refresh();
                return LRESULT(0);
            }
            WM_CLOSE => {
                if app.background.as_ref().is_some_and(Background::is_preview) {
                    let _ = DestroyWindow(hwnd);
                    return LRESULT(0);
                }
                let _ = ShowWindow(hwnd, SW_HIDE);
                return LRESULT(0);
            }
            SHOW => {
                app.show();
                return LRESULT(0);
            }
            TRAY => {
                match lp.0 as u32 {
                    WM_LBUTTONDBLCLK => app.show(),
                    WM_RBUTTONUP | WM_CONTEXTMENU => app.menu(),
                    _ => {}
                }
                return LRESULT(0);
            }
            WM_QUERYENDSESSION => return LRESULT(1),
            WM_ENDSESSION if wp.0 != 0 => {
                if let Some(bg) = &mut app.background {
                    let _ = bg.stop();
                }
                return LRESULT(0);
            }
            _ if message == app.taskbar_created => {
                app.add_tray();
                return LRESULT(0);
            }
            _ => {}
        }
    }
    if message == WM_DESTROY {
        PostQuitMessage(0);
        return LRESULT(0);
    }
    if matches!(message, WM_COMMAND | WM_TIMER | WM_HSCROLL) {
        return LRESULT(0);
    }
    DefWindowProcW(hwnd, message, wp, lp)
}

struct MutexHandle(HANDLE);
impl Drop for MutexHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub fn run() -> Result<(), String> {
    unsafe {
        if std::env::args().any(|a| a == "--diagnose") {
            return diagnose();
        }
        let preview = std::env::args().any(|a| a == "--preview")
            || std::env::current_exe()
                .ok()
                .and_then(|p| {
                    p.file_stem()
                        .map(|s| s.to_string_lossy().ends_with("-preview"))
                })
                .unwrap_or(false);
        let window_title = w!("Mouse Speed Switcher");
        let mutex_name = if preview {
            w!("Local\\MouseSpeedSwitcher.Preview.v1.11")
        } else {
            w!("Local\\MouseSpeedSwitcher.Singleton.v1")
        };
        let mutex = CreateMutexW(None, false, mutex_name).map_err(|e| e.to_string())?;
        let exists = GetLastError() == ERROR_ALREADY_EXISTS;
        let _guard = MutexHandle(mutex);
        if exists {
            for _ in 0..30 {
                if let Ok(hwnd) = FindWindowW(CLASS, window_title) {
                    let _ = PostMessageW(Some(hwnd), SHOW, WPARAM(0), LPARAM(0));
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            return Err("程序已在运行，但设置窗口尚未就绪，请稍后重试".into());
        }
        let _ = SetProcessDPIAware();
        InitCommonControlsEx(&INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_BAR_CLASSES,
        })
        .ok()
        .map_err(|e| e.to_string())?;
        let instance = GetModuleHandleW(None).map_err(|e| e.to_string())?;
        let class = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: instance.into(),
            lpszClassName: CLASS,
            hCursor: LoadCursorW(None, IDC_ARROW).map_err(|e| e.to_string())?,
            hbrBackground: HBRUSH((COLOR_WINDOW.0 + 1) as usize as *mut _),
            hIcon: LoadIconW(
                Some(instance.into()),
                PCWSTR(std::ptr::without_provenance(1)),
            )
            .map_err(|e| e.to_string())?,
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            return Err("无法注册设置窗口".into());
        }
        let scale = layout_scale();
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_CLIPCHILDREN;
        let mut bounds = RECT {
            left: 0,
            top: 0,
            right: (960.0 * scale) as i32,
            bottom: (740.0 * scale) as i32,
        };
        AdjustWindowRectEx(&mut bounds, style, false, WS_EX_CONTROLPARENT)
            .map_err(|e| e.to_string())?;
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            CLASS,
            window_title,
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .map_err(|e| e.to_string())?;
        let mut app = match App::controls(hwnd) {
            Ok(a) => Box::new(a),
            Err(e) => {
                let _ = DestroyWindow(hwnd);
                return Err(e);
            }
        };
        match if preview {
            Ok(Background::preview(preview_view()))
        } else {
            Background::start()
        } {
            Ok(bg) => app.background = Some(bg),
            Err(e) => {
                let _ = DestroyWindow(hwnd);
                return Err(e);
            }
        }
        let app = Box::new(RefCell::new(*app));
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, (&*app as *const RefCell<App>) as isize);
        let tray_ok = app.borrow().add_tray();
        if !tray_ok {
            error_box("托盘图标创建失败，请从设置窗口重启程序。");
        }
        app.borrow_mut().refresh();
        send(
            hwnd,
            WM_CHANGEUISTATE,
            (UIS_SET | (UISF_HIDEFOCUS << 16)) as usize,
            0,
        );
        if !std::env::args().any(|a| a == "--background") || !tray_ok {
            app.borrow().show();
            // A launcher may supply SW_HIDE in STARTUPINFO; the first ShowWindow consumes it.
            app.borrow().show();
        }
        SetTimer(Some(hwnd), 1, 500, None);
        let mut msg = MSG::default();
        let mut loop_error = None;
        loop {
            let n = GetMessageW(&mut msg, None, 0, 0).0;
            if n <= 0 {
                if n < 0 {
                    loop_error = Some("设置窗口消息循环失败".into());
                }
                break;
            }
            if msg.message == WM_KEYDOWN {
                send(
                    hwnd,
                    WM_CHANGEUISTATE,
                    (UIS_CLEAR | (UISF_HIDEFOCUS << 16)) as usize,
                    0,
                );
            }
            if !IsDialogMessageW(hwnd, &msg).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        let _ = KillTimer(Some(hwnd), 1);
        let _ = Shell_NotifyIconW(NIM_DELETE, &app.borrow().tray_data());
        let result = app.borrow_mut().background.as_mut().unwrap().stop();
        // WM_DESTROY has already destroyed controls on normal exit.
        if IsWindow(Some(hwnd)).as_bool() {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            let _ = DestroyWindow(hwnd);
        }
        loop_error.map_or(result, Err)
    }
}

fn preview_view() -> View {
    let mut view = View::default();
    if std::env::args().any(|a| a == "--preview-empty") {
        return view;
    }
    for (key, alias, speed, online) in [
        ("preview-a", "罗技 MX Master 3S", 12, true),
        ("preview-b", "雷蛇 Viper V2 Pro", 8, true),
        ("preview-c", "罗技 G502 HERO", 10, false),
    ] {
        let identity = Identity {
            key: key.into(),
            name: alias.into(),
            instance: format!("HID\\{key}"),
            interface: format!("preview://{key}"),
            container: Some(format!("{{{key}}}")),
            serial: None,
        };
        view.config.discover(
            &identity,
            Parameters {
                speed,
                acceleration: [6, 10, 1],
            },
        );
        if online {
            view.connected.insert(key.into(), identity);
        }
    }
    view.active = Some("preview-a".into());
    view.notice = "预览模式：不监听输入、不保存配置、不更改系统设置".into();
    view
}

fn diagnose() -> Result<(), String> {
    use crate::model::MouseSystem;
    let parameters = platform::WindowsMouse.read()?;
    let devices = platform::enumerate()?;
    let report = serde_json::json!({"parameters": parameters, "devices": devices.iter().map(|(_, d)| d).collect::<Vec<_>>()});
    // A read-only diagnostic mode: stdout is available when launched by a console/tool.
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn empty_menu_glyph_and_status_text_are_safe() {
        unsafe {
            let theme = theme::Theme::new(1.0);
            let state = theme::PaintState::new(&theme, 1.0);
            let screen = GetDC(None);
            let dc = CreateCompatibleDC(Some(screen));
            let bitmap = CreateCompatibleBitmap(screen, 252, 96);
            ReleaseDC(None, screen);
            let old = SelectObject(dc, bitmap.into());
            let rect = RECT {
                left: 0,
                top: 0,
                right: 252,
                bottom: 44,
            };
            theme::fill(dc, &rect, theme::BLUE);
            theme::text(dc, "", rect, theme.normal, theme::INK, DT_LEFT);
            assert_eq!(GetPixel(dc, 10, 10), theme::BLUE);
            let item = DRAWITEMSTRUCT {
                CtlType: ODT_MENU,
                itemID: STARTUP as u32,
                hDC: dc,
                rcItem: rect,
                ..Default::default()
            };
            tray::draw(&state, &item); // Startup disabled -> empty glyph, actual native draw path.
            assert_eq!(GetPixel(dc, 20, 20), theme::WHITE);
            SelectObject(dc, old);
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(dc);
        }
    }
    #[test]
    fn native_controls_dispatch_through_window_procedure() {
        unsafe {
            InitCommonControlsEx(&INITCOMMONCONTROLSEX {
                dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_BAR_CLASSES,
            })
            .ok()
            .unwrap();
            let instance = GetModuleHandleW(None).unwrap();
            let class = w!("MouseSpeedSwitcher.NativeCommandTest");
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(procedure),
                hInstance: instance.into(),
                lpszClassName: class,
                ..Default::default()
            });
            let hwnd = CreateWindowExW(
                WS_EX_CONTROLPARENT,
                class,
                w!("Command test"),
                WS_OVERLAPPED,
                0,
                0,
                960,
                740,
                None,
                None,
                Some(instance.into()),
                None,
            )
            .unwrap();
            let mut app = App::controls(hwnd).unwrap();
            app.background = Some(Background::preview(preview_view()));
            app.refresh();
            let app = Box::new(RefCell::new(app));
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (&*app as *const RefCell<App>) as isize);
            // Reentrant paints must succeed even while an update holds the App borrow.
            // Previously these messages fell through to default gray colors / blank owner-drawn rows.
            {
                let busy = app.borrow_mut();
                let screen = GetDC(None);
                let dc = CreateCompatibleDC(Some(screen));
                let bitmap = CreateCompatibleBitmap(screen, busy.px(244), busy.px(104));
                ReleaseDC(None, screen);
                let old_bitmap = SelectObject(dc, bitmap.into());
                let brush = send(
                    hwnd,
                    WM_CTLCOLORSTATIC,
                    dc.0 as usize,
                    busy.running.0 as isize,
                );
                assert_eq!(brush, busy.paint.borrow().side.0 as isize);
                for (index, expected) in [(1, COLORREF(0x00579f24)), (2, theme::MUTED)] {
                    let item = DRAWITEMSTRUCT {
                        CtlType: ODT_LISTBOX,
                        CtlID: LIST as u32,
                        itemID: index,
                        hwndItem: busy.list,
                        hDC: dc,
                        rcItem: RECT {
                            left: 0,
                            top: 0,
                            right: busy.px(244),
                            bottom: busy.px(if index == 2 { 104 } else { 56 }),
                        },
                        ..Default::default()
                    };
                    assert_eq!(
                        send(
                            hwnd,
                            WM_DRAWITEM,
                            LIST as usize,
                            (&item as *const DRAWITEMSTRUCT) as isize
                        ),
                        1
                    );
                    assert_eq!(
                        GetPixel(dc, busy.px(19), busy.px(if index == 2 { 75 } else { 27 })),
                        expected
                    );
                }
                let offset_row = DRAWITEMSTRUCT {
                    CtlType: ODT_LISTBOX,
                    CtlID: LIST as u32,
                    itemID: 1,
                    hwndItem: busy.list,
                    hDC: dc,
                    rcItem: RECT {
                        left: 0,
                        top: 20,
                        right: busy.px(244),
                        bottom: 20 + busy.px(56),
                    },
                    ..Default::default()
                };
                send(
                    hwnd,
                    WM_DRAWITEM,
                    LIST as usize,
                    (&offset_row as *const DRAWITEMSTRUCT) as isize,
                );
                assert_eq!(
                    GetPixel(dc, busy.px(19), 20 + busy.px(27)),
                    COLORREF(0x00579f24),
                    "buffered list rows must keep their window position"
                );
                let area = RECT {
                    left: 0,
                    top: 0,
                    right: busy.px(244),
                    bottom: busy.px(104),
                };
                theme::fill(dc, &area, theme::BLUE);
                {
                    let frame = theme::Buffer::new(dc, area);
                    theme::fill(frame.dc(), &area, theme::WHITE);
                    assert_eq!(
                        GetPixel(dc, 20, 20),
                        theme::BLUE,
                        "partial frame must stay offscreen"
                    );
                }
                assert_eq!(
                    GetPixel(dc, 20, 20),
                    theme::WHITE,
                    "complete frame must be presented on drop"
                );
                let old_font = SelectObject(dc, busy.paint.borrow().font.into());
                let mut face = [0u16; 128];
                let length = GetTextFaceW(dc, Some(&mut face));
                let actual_face =
                    String::from_utf16_lossy(&face[..length.saturating_sub(1) as usize]);
                assert!(
                    actual_face.contains("YaHei") || actual_face.contains("微软雅黑"),
                    "realized face: {actual_face}, handle: {:?}",
                    busy.paint.borrow().font
                );
                SelectObject(dc, old_font);
                let area = RECT {
                    left: 0,
                    top: 0,
                    right: busy.px(244),
                    bottom: busy.px(104),
                };
                theme::fill(dc, &area, theme::WHITE);
                theme::rounded(
                    dc,
                    RECT {
                        left: 10,
                        top: 10,
                        right: 56,
                        bottom: 36,
                    },
                    26,
                    theme::BLUE,
                    theme::BLUE,
                );
                let blended = (10..36).any(|y| {
                    (10..56).any(|x| {
                        let color = GetPixel(dc, x, y);
                        color != theme::WHITE && color != theme::BLUE && color.0 != u32::MAX
                    })
                });
                assert!(
                    blended,
                    "rounded switch edges must contain antialiased pixels"
                );
                SelectObject(dc, old_bitmap);
                let _ = DeleteObject(bitmap.into());
                let _ = DeleteDC(dc);
            }
            let hover_button = app.borrow().save;
            let _ = ValidateRect(Some(hover_button), None);
            send(hover_button, WM_MOUSEMOVE, 0, 0);
            assert!(!GetPropW(hover_button, theme::HOVER).is_invalid());
            let _ = ValidateRect(Some(hover_button), None);
            for _ in 0..200 {
                send(hover_button, WM_MOUSEMOVE, 0, 0);
            }
            assert!(
                !GetUpdateRect(hover_button, None, false).as_bool(),
                "unchanged hover must not keep invalidating"
            );
            send(hover_button, WM_MOUSELEAVE, 0, 0);
            assert!(GetPropW(hover_button, theme::HOVER).is_invalid());
            let title = app.borrow().title;
            let _ = ValidateRect(Some(title), None);
            for _ in 0..200 {
                send(title, WM_MOUSEMOVE, 0, 0);
            }
            assert!(
                !GetUpdateRect(title, None, false).as_bool(),
                "static labels must not repaint on hover"
            );
            // A visible historical heading changes row height, not the profile selection mapping.
            let list = app.borrow().list;
            assert_eq!(send(list, LB_GETCOUNT, 0, 0), 3);
            assert_eq!(
                send(list, LB_GETITEMHEIGHT, 2, 0),
                app.borrow().px(104) as isize
            );
            send(list, LB_SETCURSEL, 2, 0);
            send(
                hwnd,
                WM_COMMAND,
                LIST as usize | ((LBN_SELCHANGE as usize) << 16),
                list.0 as isize,
            );
            assert_eq!(get_text(app.borrow().alias), "罗技 G502 HERO");
            send(list, LB_SETCURSEL, 0, 0);
            send(
                hwnd,
                WM_COMMAND,
                LIST as usize | ((LBN_SELCHANGE as usize) << 16),
                list.0 as isize,
            );
            send(
                hwnd,
                WM_CHANGEUISTATE,
                (UIS_SET | (UISF_HIDEFOCUS << 16)) as usize,
                0,
            );
            assert!(!theme::keyboard_focus(list));
            send(
                hwnd,
                WM_CHANGEUISTATE,
                (UIS_CLEAR | (UISF_HIDEFOCUS << 16)) as usize,
                0,
            );
            assert!(theme::keyboard_focus(list));
            // Native LISTBOX normally maps blank space to its nearest item. Only the
            // visible device cards should accept pointer selection, including grouped rows.
            let click = |x: i32, y: i32| {
                let scale = app.borrow().scale;
                let x = (x as f32 * scale).round() as i32;
                let y = (y as f32 * scale).round() as i32;
                let location = ((x as u16 as u32) | ((y as u16 as u32) << 16)) as isize;
                send(list, WM_LBUTTONDOWN, 1, location);
                send(list, WM_LBUTTONUP, 0, location);
            };
            for (x, y) in [(30, 400), (30, 130), (1, 20), (30, 55)] {
                click(x, y);
                assert_eq!(
                    send(list, LB_GETCURSEL, 0, 0),
                    0,
                    "blank space, group heading and card margins must not select"
                );
            }
            click(30, 182);
            assert_eq!(send(list, LB_GETCURSEL, 0, 0), 2);
            assert_eq!(get_text(app.borrow().alias), "罗技 G502 HERO");
            click(30, 400);
            assert_eq!(send(list, LB_GETCURSEL, 0, 0), 2);
            click(30, 20);
            assert_eq!(send(list, LB_GETCURSEL, 0, 0), 0);
            let precision = app.borrow().precision;
            assert!(checked(precision));
            send(precision, BM_CLICK, 0, 0);
            assert!(!checked(precision));
            assert!(app.borrow().dirty(&app.borrow().view()));
            let save = app.borrow().save;
            send(save, BM_CLICK, 0, 0);
            assert!(!app.borrow().dirty(&app.borrow().view()));
            let pause = app.borrow().pause;
            send(pause, BM_CLICK, 0, 0);
            assert!(app.borrow().view().paused);
            let association = app.borrow().association;
            send(association, BM_CLICK, 0, 0);
            assert!(app.borrow().association_open);
            let details = app.borrow().detail_toggle;
            send(details, BM_CLICK, 0, 0);
            assert!(app.borrow().details_open);
            let connected = app.borrow().connected;
            assert_eq!(send(connected, CB_GETCOUNT, 0, 0), 2);
            send(connected, CB_SETCURSEL, 1, 0);
            let bound_device = app.borrow().device_rows[1].0.clone();
            let selected_profile = app.borrow().selected.clone().unwrap();
            let bind = app.borrow().bind;
            send(bind, BM_CLICK, 0, 0);
            assert_eq!(
                app.borrow().view().config.bindings[&bound_device],
                selected_profile
            );
            app.borrow().sync_paint(Some(&app.borrow().view()));
            let menu = tray::create_menu(&app.borrow().paint.borrow()).unwrap();
            assert_eq!(GetMenuItemCount(Some(menu)), 5);
            assert_eq!(GetMenuItemID(menu, 0), OPEN as u32);
            assert_eq!(GetMenuItemID(menu, 1), PAUSE as u32);
            assert_eq!(GetMenuItemID(menu, 2), STARTUP as u32);
            assert_eq!(GetMenuItemID(menu, 4), EXIT as u32);
            let mut menu_text = [0u16; 64];
            let count = GetMenuStringW(menu, OPEN as u32, Some(&mut menu_text), MF_BYCOMMAND);
            assert_eq!(
                String::from_utf16_lossy(&menu_text[..count as usize]),
                "打开设置"
            );
            let mut info = MENUITEMINFOW {
                cbSize: size_of::<MENUITEMINFOW>() as u32,
                fMask: MIIM_FTYPE | MIIM_STATE,
                ..Default::default()
            };
            GetMenuItemInfoW(menu, STARTUP as u32, false, &mut info).unwrap();
            assert!(info.fType.0 & MFT_OWNERDRAW.0 != 0);
            assert_eq!(
                info.fState.0 & MFS_CHECKED.0 != 0,
                checked(app.borrow().startup)
            );
            let mut measure = MEASUREITEMSTRUCT {
                CtlType: ODT_MENU,
                itemID: OPEN as u32,
                ..Default::default()
            };
            assert_eq!(
                send(
                    hwnd,
                    WM_MEASUREITEM,
                    0,
                    (&mut measure as *mut MEASUREITEMSTRUCT) as isize
                ),
                1
            );
            assert_eq!(measure.itemHeight, app.borrow().px(34) as u32);
            assert_eq!(measure.itemWidth, app.borrow().px(150) as u32);
            assert_eq!(tray::label(PAUSE, true), "恢复自动切换");
            assert_eq!(tray::label(PAUSE, false), "暂停自动切换");
            DestroyMenu(menu).unwrap();
            let startup_before = checked(app.borrow().startup);
            send(hwnd, WM_COMMAND, STARTUP as usize, 0);
            assert_eq!(checked(app.borrow().startup), !startup_before);
            assert!(app.borrow().background.as_ref().unwrap().is_preview());
            assert!(!app.borrow().tray_icon.handle.is_invalid());
            assert_ne!(
                app.borrow().tray_icon.handle,
                LoadIconW(None, IDI_APPLICATION).unwrap()
            );
            // Exercise scroll messages through the real viewport subclass and child controls.
            let pane = app.borrow().pane;
            let speed = app.borrow().speed;
            let original_speed = send(speed, TBM_GETPOS, 0, 0);
            let original_device = send(connected, CB_GETCURSEL, 0, 0);
            send(pane, WM_VSCROLL, 7, 0); // Bottom.
            let maximum = app.borrow().scroll;
            assert!(maximum > 0);
            let mut bounds = RECT::default();
            let mut detail_bounds = RECT::default();
            GetWindowRect(pane, &mut bounds).unwrap();
            GetWindowRect(app.borrow().details, &mut detail_bounds).unwrap();
            assert!(
                detail_bounds.bottom <= bounds.bottom,
                "device details must be fully reachable"
            );
            send(pane, WM_VSCROLL, 6, 0); // Top.
            assert_eq!(app.borrow().scroll, 0);
            let wheel = |target: HWND, delta: i16| {
                send(target, WM_MOUSEWHEEL, (delta as u16 as usize) << 16, 0);
            };
            wheel(speed, -60);
            assert_eq!(app.borrow().scroll, 0);
            wheel(speed, -60);
            assert!(
                app.borrow().scroll > 0,
                "fractional wheel input must accumulate"
            );
            assert_eq!(send(speed, TBM_GETPOS, 0, 0), original_speed);
            wheel(connected, -120);
            assert_eq!(send(connected, CB_GETCURSEL, 0, 0), original_device);
            for _ in 0..10 {
                wheel(connected, -120);
            }
            assert_eq!(app.borrow().scroll, maximum);
            send(pane, WM_VSCROLL, 6, 0);
            for position in [50usize, 140, 220] {
                send(pane, WM_VSCROLL, 5 | (position << 16), 0);
                assert_eq!(app.borrow().scroll, position as i32);
                assert!(app.borrow().thumb_tracking);
            }
            send(pane, WM_VSCROLL, 4 | (220 << 16), 0);
            send(pane, WM_VSCROLL, 8, 0);
            assert_eq!(app.borrow().scroll, 220, "thumb release must not jump back");
            let mut info = SCROLLINFO {
                cbSize: size_of::<SCROLLINFO>() as u32,
                fMask: SIF_POS,
                ..Default::default()
            };
            GetScrollInfo(pane, SB_VERT, &mut info).unwrap();
            assert_eq!(info.nPos, 220);
            send(pane, WM_VSCROLL, 6, 0);
            // Keyboard focus reveals the full details field without moving the footer.
            let details = app.borrow().details;
            app.borrow_mut().reveal(details);
            assert!(app.borrow().scroll > 0);
            unsafe extern "system" fn cancel_test_menu(
                _hwnd: HWND,
                _msg: u32,
                _id: usize,
                _time: u32,
            ) {
                let _ = EndMenu();
            }
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = RedrawWindow(
                Some(hwnd),
                None,
                None,
                RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN | RDW_UPDATENOW,
            );
            assert_ne!(SetTimer(Some(hwnd), 99, 150, Some(cancel_test_menu)), 0);
            check(app.borrow().startup, false);
            tray::ROUNDED_POPUPS.store(0, std::sync::atomic::Ordering::Relaxed);
            app.borrow_mut().menu(); // Unchecked startup icon is deliberately empty.
            let _ = KillTimer(Some(hwnd), 99);
            assert_ne!(SetTimer(Some(hwnd), 99, 150, Some(cancel_test_menu)), 0);
            check(app.borrow().startup, true);
            app.borrow_mut().menu();
            let _ = KillTimer(Some(hwnd), 99);
            assert!(
                tray::ROUNDED_POPUPS.load(std::sync::atomic::Ordering::Relaxed) >= 2,
                "native popup windows must receive rounded regions"
            );
            DestroyWindow(hwnd).unwrap();
            let _ = UnregisterClassW(class, Some(instance.into()));
        }
    }
    #[test]
    fn native_editor_survives_activity_and_device_changes() {
        unsafe {
            InitCommonControlsEx(&INITCOMMONCONTROLSEX {
                dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_BAR_CLASSES,
            })
            .ok()
            .unwrap();
            // Hidden native controls; no live listener, parameter writes, config files or startup changes.
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!("UI test"),
                WS_OVERLAPPED,
                0,
                0,
                800,
                700,
                None,
                None,
                None,
                None,
            )
            .unwrap();
            let mut app = App::controls(hwnd).unwrap();
            let identity = Identity {
                key: "test-a".into(),
                name: "测试鼠标 A".into(),
                instance: "test-instance".into(),
                interface: "test-interface".into(),
                container: None,
                serial: None,
            };
            let parameters = Parameters {
                speed: 7,
                acceleration: [6, 10, 1],
            };
            let mut view = View::default();
            view.config.discover(&identity, parameters);
            view.connected
                .insert(identity.key.clone(), identity.clone());
            view.active = Some(identity.key.clone());
            app.background = Some(Background::preview(view));
            app.refresh();
            assert_eq!(get_text(app.alias), "测试鼠标 A");
            assert_eq!(send(app.speed, TBM_GETPOS, 0, 0), 7);
            assert!(checked(app.precision));
            set_text(app.alias, "未保存的编辑");
            send(app.speed, TBM_SETPOS, 1, 16);
            check(app.precision, false);
            {
                let mut view = app.background.as_ref().unwrap().view.lock().unwrap();
                view.active = None;
                view.connected.clear();
            }
            app.refresh();
            assert_eq!(get_text(app.alias), "未保存的编辑");
            assert_eq!(send(app.speed, TBM_GETPOS, 0, 0), 16);
            assert!(!checked(app.precision));
            assert_eq!(app.rows.len(), 1);
            assert!(app.rows[0].1.starts_with('○'));
            send(app.speed, TBM_SETPOS, 1, 100);
            assert_eq!(send(app.speed, TBM_GETPOS, 0, 0), 20);
            send(app.speed, TBM_SETPOS, 1, -10);
            assert_eq!(send(app.speed, TBM_GETPOS, 0, 0), 1);
            // Dirty tracking reflects actual saved values, and applying does not save the form.
            assert!(app.dirty(&app.view()));
            app.on_command(APPLY, 0);
            assert_eq!(app.view().config.profiles["test-a"].alias, "测试鼠标 A");
            assert_eq!(get_text(app.alias), "未保存的编辑");
            app.on_command(SAVE, 0);
            app.refresh();
            assert!(!app.dirty(&app.view()));
            assert_eq!(app.view().config.profiles["test-a"].alias, "未保存的编辑");
            app.on_command(PAUSE, 0);
            app.refresh();
            assert!(app.view().paused);
            assert_eq!(get_text(app.pause), "恢复自动切换");
            let startup = checked(app.startup);
            send(app.startup, BM_CLICK, 0, 0);
            app.on_command(STARTUP, 0);
            assert_eq!(checked(app.startup), !startup); // Preview only, no registry writes.
            app.on_command(ASSOCIATION, 0);
            app.on_command(DETAILS, 0);
            assert!(app.association_open && app.details_open);
            assert!(
                IsWindowVisible(app.connected).as_bool() == IsWindowVisible(app.pane).as_bool()
            );
            let mut apply_before = RECT::default();
            GetWindowRect(app.apply, &mut apply_before).unwrap();
            app.scroll = 9999;
            app.layout();
            assert!(app.scroll > 0 && app.scroll < 9999);
            let mut apply_after = RECT::default();
            GetWindowRect(app.apply, &mut apply_after).unwrap();
            assert_eq!(apply_before, apply_after); // Footer stays fixed during scrolling.
            assert!(get_text(app.details).contains("test-instance"));
            app.association_open = false;
            app.details_open = false;
            app.layout();
            assert_eq!(app.scroll, 0);
            assert!(!IsWindowVisible(app.details).as_bool());
            {
                let mut view = app.background.as_ref().unwrap().view.lock().unwrap();
                view.config.profiles.clear();
                view.config.bindings.clear();
                view.active = None;
            }
            app.refresh();
            assert!(!IsWindowEnabled(app.save).as_bool());
            assert!(!IsWindowEnabled(app.apply).as_bool());
            assert!(!IsWindowEnabled(app.bind).as_bool());
            assert_eq!(get_text(app.title), "尚未发现可识别的鼠标");
            assert!(get_text(app.alias).is_empty());
            DestroyWindow(hwnd).unwrap();
        }
    }
}

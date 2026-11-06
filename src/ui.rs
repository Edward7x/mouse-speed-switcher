#![allow(unsafe_op_in_unsafe_fn)]
use crate::{
    model::*,
    platform,
    storage::wide,
    worker::{Background, Command, View},
};
use std::{cell::RefCell, mem::size_of, time::Duration};
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

pub fn error_box(error: &str) {
    unsafe {
        let text = wide(error);
        MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            w!("鼠标灵敏度切换工具"),
            MB_OK | MB_ICONERROR,
        );
    }
}
unsafe fn set_text(hwnd: HWND, text: &str) {
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
        dpi.min((area.bottom - area.top - 64) as f32 / 688.0)
            .min((area.right - area.left - 40) as f32 / 796.0)
            .max(0.5)
    } else {
        dpi
    }
}

struct App {
    hwnd: HWND,
    font: HFONT,
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
    unsafe fn controls(hwnd: HWND) -> Result<Self, String> {
        let scale = layout_scale();
        let px = |v: i32| (v as f32 * scale).round() as i32;
        let font = CreateFontW(
            -px(17),
            0,
            0,
            0,
            FW_NORMAL.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            DEFAULT_PITCH.0 as u32,
            w!("Microsoft YaHei UI"),
        );
        if font.is_invalid() {
            return Err("无法创建界面字体".into());
        }
        let create = |class: PCWSTR,
                      text: &str,
                      style: u32,
                      id: u16,
                      x,
                      y,
                      width,
                      height|
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
                Some(hwnd),
                Some(HMENU(id as usize as *mut _)),
                None,
                None,
            )
            .map_err(|e| e.to_string())?;
            send(child, WM_SETFONT, font.0 as usize, 1);
            Ok(child)
        };
        create(w!("STATIC"), "鼠标配置", 0, 0, 22, 18, 260, 28)?;
        create(
            w!("STATIC"),
            "移动哪只鼠标，就自动加载哪套配置",
            0,
            0,
            310,
            18,
            470,
            28,
        )?;
        let list = create(
            w!("LISTBOX"),
            "",
            WS_BORDER.0 | WS_TABSTOP.0 | WS_VSCROLL.0 | LBS_NOTIFY as u32,
            LIST,
            22,
            54,
            265,
            342,
        )?;
        create(w!("STATIC"), "配置名称", 0, 0, 310, 56, 150, 24)?;
        let alias = create(
            w!("EDIT"),
            "",
            WS_BORDER.0 | WS_TABSTOP.0 | ES_AUTOHSCROLL as u32,
            0,
            310,
            84,
            460,
            32,
        )?;
        send(alias, EM_SETLIMITTEXT, 100, 0);
        let speed_label = create(w!("STATIC"), "指针速度：10 / 20", 0, 0, 310, 130, 460, 26)?;
        let speed = create(
            TRACKBAR_CLASSW,
            "",
            WS_TABSTOP.0 | TBS_AUTOTICKS,
            SPEED,
            300,
            162,
            480,
            44,
        )?;
        send(speed, TBM_SETRANGE, 1, 1 | (20 << 16));
        send(speed, TBM_SETPOS, 1, 10);
        let precision = create(
            w!("BUTTON"),
            "提高指针精确度",
            WS_TABSTOP.0 | BS_AUTOCHECKBOX as u32,
            0,
            310,
            214,
            330,
            30,
        )?;
        create(
            w!("BUTTON"),
            "保存配置",
            WS_TABSTOP.0 | BS_DEFPUSHBUTTON as u32,
            SAVE,
            310,
            260,
            140,
            36,
        )?;
        create(
            w!("BUTTON"),
            "立即应用已保存配置",
            WS_TABSTOP.0,
            APPLY,
            466,
            260,
            230,
            36,
        )?;
        let details = create(
            w!("EDIT"),
            "选择左侧配置查看设备信息",
            WS_BORDER.0
                | ES_MULTILINE as u32
                | ES_READONLY as u32
                | ES_AUTOVSCROLL as u32
                | WS_VSCROLL.0,
            0,
            310,
            310,
            460,
            86,
        )?;
        create(
            w!("STATIC"),
            "更换端口后，可将下面的已连接设备关联到左侧选中的配置：",
            0,
            0,
            22,
            414,
            750,
            26,
        )?;
        let connected = create(
            w!("COMBOBOX"),
            "",
            WS_TABSTOP.0 | WS_VSCROLL.0 | CBS_DROPDOWNLIST as u32,
            0,
            22,
            448,
            510,
            210,
        )?;
        create(
            w!("BUTTON"),
            "关联到选中配置",
            WS_TABSTOP.0,
            BIND,
            550,
            446,
            220,
            34,
        )?;
        let pause = create(
            w!("BUTTON"),
            "暂停自动切换",
            WS_TABSTOP.0,
            PAUSE,
            22,
            502,
            180,
            36,
        )?;
        let startup = create(
            w!("BUTTON"),
            "开机自启",
            WS_TABSTOP.0 | BS_AUTOCHECKBOX as u32,
            STARTUP,
            228,
            504,
            180,
            30,
        )?;
        check(startup, platform::autostart());
        create(
            w!("STATIC"),
            "关闭窗口后仍在托盘运行；编辑后请点击保存。",
            0,
            0,
            22,
            550,
            750,
            26,
        )?;
        let status = create(
            w!("EDIT"),
            "正在初始化设备…",
            WS_BORDER.0
                | ES_MULTILINE as u32
                | ES_READONLY as u32
                | ES_AUTOVSCROLL as u32
                | WS_VSCROLL.0,
            0,
            22,
            584,
            748,
            82,
        )?;
        Ok(Self {
            hwnd,
            font,
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
        })
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
            bg.send(c);
        }
    }
    unsafe fn refresh(&mut self) {
        if self.background.is_none() {
            return;
        }
        let view = self.view();
        let rows: Vec<_> = view
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
        if rows != self.rows {
            send(self.list, LB_RESETCONTENT, 0, 0);
            for (_, label) in &rows {
                let text = wide(label);
                send(self.list, LB_ADDSTRING, 0, text.as_ptr() as isize);
            }
            self.rows = rows;
            if self.selected.is_none() {
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
        let name = view
            .active
            .as_ref()
            .and_then(|id| view.config.profiles.get(id))
            .map(|p| p.alias.as_str())
            .unwrap_or("等待鼠标输入");
        let mut status = format!(
            "{} · 当前配置：{}\r\n{}",
            if view.paused {
                "已暂停"
            } else {
                "自动切换中"
            },
            name,
            view.notice
        );
        if let Some(error) = &view.error {
            status.push_str(&format!("\r\n提示：{error}"));
        }
        // Avoid resetting selection / scroll position in the status control every timer tick.
        if get_text(self.status) != status {
            set_text(self.status, &status);
        }
        set_text(
            self.pause,
            if view.paused {
                "恢复自动切换"
            } else {
                "暂停自动切换"
            },
        );
    }
    unsafe fn load_editor(&mut self, view: &View) {
        let Some(p) = self
            .selected
            .as_ref()
            .and_then(|id| view.config.profiles.get(id))
        else {
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
            &format!("指针速度：{} / 20", send(self.speed, TBM_GETPOS, 0, 0)),
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
            hIcon: LoadIconW(None, IDI_APPLICATION).unwrap_or_default(),
            ..Default::default()
        };
        let tip = wide("鼠标灵敏度切换 · 双击打开设置");
        data.szTip[..tip.len()].copy_from_slice(&tip);
        data
    }
    unsafe fn add_tray(&self) -> bool {
        Shell_NotifyIconW(NIM_ADD, &self.tray_data()).as_bool()
    }
    unsafe fn menu(&self) {
        let Ok(menu) = CreatePopupMenu() else {
            return;
        };
        let _ = AppendMenuW(menu, MF_STRING, OPEN as usize, w!("打开设置"));
        let paused = self.view().paused;
        let pause = wide(if paused {
            "恢复自动切换"
        } else {
            "暂停自动切换"
        });
        let _ = AppendMenuW(menu, MF_STRING, PAUSE as usize, PCWSTR(pause.as_ptr()));
        let _ = AppendMenuW(
            menu,
            MF_STRING
                | if platform::autostart() {
                    MF_CHECKED
                } else {
                    MF_UNCHECKED
                },
            STARTUP as usize,
            w!("开机自启"),
        );
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, EXIT as usize, w!("退出并恢复原设置"));
        let mut point = POINT::default();
        let _ = GetCursorPos(&mut point);
        let _ = SetForegroundWindow(self.hwnd);
        // Return the command instead of dispatching it inside the nested menu message loop.
        let command = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_RETURNCMD,
            point.x,
            point.y,
            None,
            self.hwnd,
            None,
        );
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
unsafe extern "system" fn procedure(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // Control notifications and modal menus can reenter this procedure. Never alias mutable App references.
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<App>;
    if !ptr.is_null()
        && let Ok(mut app) = (*ptr).try_borrow_mut()
    {
        match message {
            WM_COMMAND => {
                app.on_command((wp.0 & 0xffff) as u16, ((wp.0 >> 16) & 0xffff) as u16);
                return LRESULT(0);
            }
            WM_HSCROLL => {
                app.update_speed();
                return LRESULT(0);
            }
            WM_TIMER => {
                app.refresh();
                return LRESULT(0);
            }
            WM_CLOSE => {
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
        let mutex = CreateMutexW(None, false, w!("Local\\MouseSpeedSwitcher.Singleton.v1"))
            .map_err(|e| e.to_string())?;
        let exists = GetLastError() == ERROR_ALREADY_EXISTS;
        let _guard = MutexHandle(mutex);
        if exists {
            for _ in 0..30 {
                if let Ok(hwnd) = FindWindowW(CLASS, None) {
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
            hIcon: LoadIconW(None, IDI_APPLICATION).map_err(|e| e.to_string())?,
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            return Err("无法注册设置窗口".into());
        }
        let scale = layout_scale();
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        let mut bounds = RECT {
            left: 0,
            top: 0,
            right: (796.0 * scale) as i32,
            bottom: (688.0 * scale) as i32,
        };
        AdjustWindowRectEx(&mut bounds, style, false, WS_EX_CONTROLPARENT)
            .map_err(|e| e.to_string())?;
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            CLASS,
            w!("鼠标灵敏度切换工具"),
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
        match Background::start() {
            Ok(bg) => app.background = Some(bg),
            Err(e) => {
                let _ = DestroyWindow(hwnd);
                let _ = DeleteObject(app.font.into());
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
        if !std::env::args().any(|a| a == "--background") || !tray_ok {
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
        let _ = DeleteObject(app.borrow().font.into());
        loop_error.map_or(result, Err)
    }
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
            DestroyWindow(hwnd).unwrap();
            let _ = DeleteObject(app.font.into());
        }
    }
}

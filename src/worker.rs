#![allow(unsafe_op_in_unsafe_fn)]
use crate::{
    model::*,
    platform::{self, WindowsMouse},
    storage,
};
use std::{
    collections::BTreeMap,
    mem::size_of,
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{Input::*, WindowsAndMessaging::*},
    },
    core::w,
};

pub const WAKE: u32 = WM_APP + 1;
#[derive(Clone, Default)]
pub struct View {
    pub config: Config,
    pub connected: BTreeMap<String, Identity>,
    pub active: Option<String>,
    pub paused: bool,
    pub error: Option<String>,
    pub notice: String,
}
pub enum Command {
    Save {
        id: String,
        alias: String,
        parameters: Parameters,
    },
    Apply(String),
    Bind {
        device: String,
        profile: String,
    },
    Pause,
    Shutdown,
}
pub struct Background {
    pub hwnd: usize,
    pub tx: mpsc::Sender<Command>,
    pub view: Arc<Mutex<View>>,
    thread: Option<thread::JoinHandle<Result<(), String>>>,
}
impl Background {
    /// Isolated UI preview: no listener, disk writes or system parameter changes.
    pub fn preview(view: View) -> Self {
        let (tx, _) = mpsc::channel();
        Self {
            hwnd: 0,
            tx,
            view: Arc::new(Mutex::new(view)),
            thread: None,
        }
    }
    pub fn is_preview(&self) -> bool {
        self.thread.is_none()
    }
    pub fn start() -> Result<Self, String> {
        let view = Arc::new(Mutex::new(View::default()));
        let shared = view.clone();
        let (tx, rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = thread::spawn(move || unsafe { run(rx, shared, ready_tx) });
        match ready_rx.recv().map_err(|e| e.to_string())? {
            Ok(hwnd) => Ok(Self {
                hwnd,
                tx,
                view,
                thread: Some(thread),
            }),
            Err(e) => {
                let _ = thread.join();
                Err(e)
            }
        }
    }
    pub fn send(&self, command: Command) {
        if self.tx.send(command).is_ok() {
            unsafe {
                let _ = PostMessageW(Some(HWND(self.hwnd as *mut _)), WAKE, WPARAM(0), LPARAM(0));
            }
        }
    }
    pub fn stop(&mut self) -> Result<(), String> {
        self.send(Command::Shutdown);
        self.thread
            .take()
            .map(|t| {
                t.join().unwrap_or_else(|_| {
                    Err(crate::i18n::text!(
                        "监听线程异常退出",
                        "Input monitoring thread terminated unexpectedly"
                    )
                    .into())
                })
            })
            .unwrap_or(Ok(()))
    }
}
impl Drop for Background {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

struct Worker {
    engine: Switcher<WindowsMouse>,
    config: Config,
    path: PathBuf,
    devices: BTreeMap<usize, Identity>,
    active_handle: Option<usize>,
    desired: Option<usize>,
    rx: mpsc::Receiver<Command>,
    view: Arc<Mutex<View>>,
    error: Option<String>,
    notice: String,
    failed: Option<(String, Parameters, Instant)>,
    stop_error: Option<String>,
}
impl Worker {
    fn publish(&self) {
        *self.view.lock().unwrap() = View {
            config: self.config.clone(),
            connected: self
                .devices
                .values()
                .map(|d| (d.key.clone(), d.clone()))
                .collect(),
            active: self.engine.active.clone(),
            paused: self.engine.paused,
            error: self.error.clone(),
            notice: self.notice.clone(),
        };
    }
    fn persist(&mut self, config: Config) -> bool {
        match storage::save(&self.path, &config) {
            Ok(()) => {
                self.config = config;
                self.notice = crate::i18n::text!("配置已保存", "Profile saved").into();
                true
            }
            Err(e) => {
                self.error = Some(crate::i18n::localized_format!(
                    "保存失败：{e}",
                    "Save failed: {e}"
                ));
                false
            }
        }
    }
    fn refresh(&mut self) {
        match platform::enumerate() {
            Ok(entries) => {
                self.devices = entries.into_iter().collect();
                let mut changed = false;
                for d in self.devices.values() {
                    changed |= self.config.discover(d, self.engine.initial).1;
                }
                if changed && let Err(e) = storage::save(&self.path, &self.config) {
                    self.error = Some(crate::i18n::localized_format!(
                        "新设备配置尚未保存：{e}",
                        "New device profile has not been saved: {e}"
                    ));
                }
            }
            Err(e) => self.error = Some(e),
        }
    }
    fn sole(&self) -> Option<usize> {
        sole_device(&self.devices)
    }
    fn activate(&mut self, handle: usize, force: bool) {
        self.desired = Some(handle);
        let Some(d) = self.devices.get(&handle) else {
            return;
        };
        let Some(id) = self.config.bindings.get(&d.key) else {
            return;
        };
        let parameters = self.config.profiles[id].parameters;
        if !force && self.engine.paused {
            return;
        }
        if !force
            && self.failed.as_ref().is_some_and(|(i, p, when)| {
                i == id && *p == parameters && when.elapsed() < Duration::from_secs(1)
            })
        {
            return;
        }
        let active_changed = self.engine.active.as_ref() != Some(id);
        let last = self.engine.last;
        match self.engine.activate(id, parameters, force) {
            Ok(()) => {
                self.active_handle = Some(handle);
                self.failed = None;
                if active_changed || last != self.engine.last || force {
                    self.publish();
                }
            }
            Err(e) => {
                self.failed = Some((id.clone(), parameters, Instant::now()));
                self.error = Some(e);
                self.publish();
            }
        }
    }
    fn command(&mut self, command: Command) {
        match command {
            Command::Save {
                id,
                alias,
                parameters,
            } => {
                let alias = alias.trim();
                if alias.is_empty() || !parameters.valid() {
                    self.error = Some(
                        crate::i18n::text!(
                            "请输入名称和有效的鼠标参数",
                            "Enter a name and valid mouse settings"
                        )
                        .into(),
                    );
                } else {
                    let mut next = self.config.clone();
                    if let Some(p) = next.profiles.get_mut(&id) {
                        p.alias = alias.into();
                        p.parameters = parameters;
                        if self.persist(next)
                            && self.engine.active.as_ref() == Some(&id)
                            && let Some(h) =
                                self.active_handle.filter(|h| self.devices.contains_key(h))
                        {
                            self.activate(h, true);
                        }
                    }
                }
            }
            Command::Apply(id) => {
                if let Some(h) = self
                    .devices
                    .iter()
                    .find(|(_, d)| self.config.bindings.get(&d.key) == Some(&id))
                    .map(|(h, _)| *h)
                {
                    self.activate(h, true);
                } else {
                    self.error = Some(
                        crate::i18n::text!(
                            "此配置没有已连接的鼠标；请先关联设备",
                            "No connected mouse uses this profile. Link a device first."
                        )
                        .into(),
                    );
                }
            }
            Command::Bind { device, profile } => {
                if self.config.profiles.contains_key(&profile)
                    && self.devices.values().any(|d| d.key == device)
                {
                    let mut next = self.config.clone();
                    next.bindings.insert(device.clone(), profile);
                    if self.persist(next)
                        && let Some(h) = self
                            .devices
                            .iter()
                            .find(|(_, d)| d.key == device)
                            .map(|(h, _)| *h)
                    {
                        self.activate(h, false);
                    }
                }
            }
            Command::Pause => {
                self.engine.paused = !self.engine.paused;
                if !self.engine.paused
                    && let Some(h) = self
                        .desired
                        .filter(|h| self.devices.contains_key(h))
                        .or_else(|| self.sole())
                {
                    self.activate(h, false);
                }
            }
            Command::Shutdown => {
                self.stop_error = self.engine.restore().err();
                unsafe {
                    PostQuitMessage(0);
                }
            }
        }
        self.publish();
    }
    unsafe fn input(&mut self, lparam: LPARAM) {
        // RAWINPUT is aligned; mouse reports have a fixed size. Reject HID/keyboard or oversized packets.
        let mut raw = RAWINPUT::default();
        let mut size = size_of::<RAWINPUT>() as u32;
        let n = GetRawInputData(
            HRAWINPUT(lparam.0 as *mut _),
            RID_INPUT,
            Some((&mut raw as *mut RAWINPUT).cast()),
            &mut size,
            size_of::<RAWINPUTHEADER>() as u32,
        );
        if n == u32::MAX
            || n < (size_of::<RAWINPUTHEADER>() + size_of::<RAWMOUSE>()) as u32
            || raw.header.dwType != RIM_TYPEMOUSE.0
        {
            return;
        }
        let mouse = raw.data.mouse;
        if mouse.lLastX == 0 && mouse.lLastY == 0 && mouse.Anonymous.Anonymous.usButtonFlags == 0 {
            return;
        }
        self.activate(raw.header.hDevice.0 as usize, false);
    }
}
unsafe extern "system" fn procedure(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if matches!(message, WM_INPUT | WM_INPUT_DEVICE_CHANGE | WAKE) {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Worker;
        if !ptr.is_null() {
            let worker = &mut *ptr;
            match message {
                WM_INPUT => worker.input(lp),
                WM_INPUT_DEVICE_CHANGE => {
                    let handle = lp.0 as usize;
                    let was_active = worker.active_handle == Some(handle);
                    worker.refresh();
                    if wp.0 == GIDC_ARRIVAL as usize {
                        worker.activate(handle, false);
                    } else if was_active {
                        worker.active_handle = None;
                        worker.engine.active = None;
                        if let Some(h) = worker.sole() {
                            worker.activate(h, false);
                        }
                    }
                    worker.publish();
                }
                WAKE => {
                    while let Ok(c) = worker.rx.try_recv() {
                        worker.command(c);
                    }
                }
                _ => {}
            }
        }
    }
    // Foreground WM_INPUT requires DefWindowProc cleanup, even though this window is hidden.
    DefWindowProcW(hwnd, message, wp, lp)
}
unsafe fn run(
    rx: mpsc::Receiver<Command>,
    view: Arc<Mutex<View>>,
    ready: mpsc::SyncSender<Result<usize, String>>,
) -> Result<(), String> {
    let setup = || -> Result<(HWND, Box<Worker>), String> {
        unsafe {
            let instance = GetModuleHandleW(None).map_err(|e| e.to_string())?;
            let class = WNDCLASSW {
                lpfnWndProc: Some(procedure),
                hInstance: instance.into(),
                lpszClassName: w!("MouseSpeedSwitcher.Input.v1"),
                ..Default::default()
            };
            if RegisterClassW(&class) == 0 {
                return Err(crate::i18n::text!(
                    "无法注册输入监听窗口",
                    "Cannot register the input monitoring window"
                )
                .into());
            }
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class.lpszClassName,
                w!(""),
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(instance.into()),
                None,
            )
            .map_err(|e| e.to_string())?;
            // Setup all fallible state before exposing the window to the UI.
            let result = (|| {
                let engine = Switcher::new(WindowsMouse)?;
                let path = storage::config_path()?;
                let (config, warning) = storage::load(&path)?;
                RegisterRawInputDevices(
                    &[RAWINPUTDEVICE {
                        usUsagePage: 1,
                        usUsage: 2,
                        dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
                        hwndTarget: hwnd,
                    }],
                    size_of::<RAWINPUTDEVICE>() as u32,
                )
                .map_err(|e| e.to_string())?;
                Ok(Box::new(Worker {
                    engine,
                    config,
                    path,
                    devices: BTreeMap::new(),
                    active_handle: None,
                    desired: None,
                    rx,
                    view,
                    error: warning,
                    notice: String::new(),
                    failed: None,
                    stop_error: None,
                }))
            })();
            match result {
                Ok(worker) => Ok((hwnd, worker)),
                Err(e) => {
                    let _ = DestroyWindow(hwnd);
                    Err(e)
                }
            }
        }
    };
    let (hwnd, mut worker) = match setup() {
        Ok(pair) => pair,
        Err(e) => {
            let _ = ready.send(Err(e.clone()));
            return Err(e);
        }
    };
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, (&mut *worker as *mut Worker) as isize);
    worker.refresh();
    if let Some(h) = worker.sole() {
        worker.activate(h, false);
    }
    worker.publish();
    let _ = ready.send(Ok(hwnd.0 as usize));
    let mut msg = MSG::default();
    loop {
        let result = GetMessageW(&mut msg, None, 0, 0).0;
        if result <= 0 {
            if result < 0 {
                worker.stop_error = Some(
                    crate::i18n::text!(
                        "输入监听消息循环失败",
                        "Input monitoring message loop failed"
                    )
                    .into(),
                );
                let _ = worker.engine.restore();
            }
            break;
        }
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
    let _ = DestroyWindow(hwnd);
    worker.stop_error.take().map_or(Ok(()), Err)
}

#![allow(unsafe_op_in_unsafe_fn)]
use super::*;
use std::result::Result::Ok;

pub fn label(id: u16, paused: bool) -> &'static str {
    match id {
        OPEN => crate::i18n::text!("打开设置", "Open settings"),
        PAUSE if paused => crate::i18n::text!("恢复自动切换", "Resume switching"),
        PAUSE => crate::i18n::text!("暂停自动切换", "Pause switching"),
        STARTUP => crate::i18n::text!("开机自启", "Run at startup"),
        LANGUAGE => crate::i18n::text!("语言", "Language"),
        LANGUAGE_ZH => crate::i18n::text!("中文", "Chinese"),
        LANGUAGE_EN => "English",
        EXIT => crate::i18n::text!("退出", "Exit"),
        _ => "",
    }
}

unsafe fn append_item(
    menu: HMENU,
    id: u16,
    state: &theme::PaintState,
) -> windows::core::Result<()> {
    let text = wide(label(id, state.paused));
    AppendMenuW(menu, MF_STRING, id as usize, PCWSTR(text.as_ptr()))?;
    SetMenuItemInfoW(
        menu,
        id as u32,
        false,
        &MENUITEMINFOW {
            cbSize: size_of::<MENUITEMINFOW>() as u32,
            fMask: MIIM_FTYPE | MIIM_DATA | MIIM_STATE,
            fType: MFT_OWNERDRAW,
            dwItemData: id as usize,
            fState: if (id == STARTUP && state.autostart)
                || (id == LANGUAGE_ZH && !crate::i18n::english())
                || (id == LANGUAGE_EN && crate::i18n::english())
            {
                MFS_CHECKED
            } else {
                MFS_UNCHECKED
            },
            ..Default::default()
        },
    )
}

pub unsafe fn create_menu(state: &theme::PaintState) -> Result<HMENU, String> {
    let menu = CreatePopupMenu().map_err(|e| e.to_string())?;
    let result = (|| -> windows::core::Result<()> {
        for id in [OPEN, PAUSE, STARTUP] {
            append_item(menu, id, state)?;
        }
        let languages = CreatePopupMenu()?;
        let text = wide(label(LANGUAGE, state.paused));
        // Attach first so destroying the root also cleans up the submenu on failure.
        if let Err(error) = AppendMenuW(menu, MF_POPUP, languages.0 as usize, PCWSTR(text.as_ptr()))
        {
            let _ = DestroyMenu(languages);
            return Err(error);
        }
        SetMenuItemInfoW(
            menu,
            3,
            true,
            &MENUITEMINFOW {
                cbSize: size_of::<MENUITEMINFOW>() as u32,
                fMask: MIIM_ID | MIIM_FTYPE | MIIM_DATA,
                wID: LANGUAGE as u32,
                fType: MFT_OWNERDRAW,
                dwItemData: LANGUAGE as usize,
                ..Default::default()
            },
        )?;
        for id in [LANGUAGE_ZH, LANGUAGE_EN] {
            append_item(languages, id, state)?;
        }
        AppendMenuW(menu, MF_SEPARATOR, 0, None)?;
        append_item(menu, EXIT, state)?;
        SetMenuInfo(
            menu,
            &MENUINFO {
                cbSize: size_of::<MENUINFO>() as u32,
                fMask: MIM_BACKGROUND | MIM_STYLE | MIM_APPLYTOSUBMENUS,
                hbrBack: state.white,
                dwStyle: MNS_NOCHECK,
                ..Default::default()
            },
        )?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = DestroyMenu(menu);
        return Err(error.to_string());
    }
    Ok(menu)
}

pub unsafe fn draw(state: &theme::PaintState, item: &DRAWITEMSTRUCT) {
    let dc = item.hDC;
    let p = |v| state.px(v);
    let mut rect = item.rcItem;
    theme::fill(dc, &rect, theme::POPOVER);
    let selected = item.itemState.0 & ODS_SELECTED.0 != 0;
    if selected {
        let inset = RECT {
            left: rect.left + p(5),
            right: rect.right - p(5),
            top: rect.top + p(3),
            bottom: rect.bottom - p(3),
        };
        theme::rounded(dc, inset, p(10), theme::ACCENT, theme::ACCENT);
    }
    let id = item.itemID as u16;
    let glyph = match id {
        OPEN => "\u{e713}",
        LANGUAGE => "\u{e774}",
        PAUSE if state.paused => "\u{e768}",
        PAUSE => "\u{e769}",
        STARTUP if state.autostart => "\u{e73e}",
        LANGUAGE_ZH if !crate::i18n::english() => "\u{e73e}",
        LANGUAGE_EN if crate::i18n::english() => "\u{e73e}",
        EXIT => "\u{e8ac}",
        _ => "",
    };
    theme::text(
        dc,
        glyph,
        RECT {
            left: rect.left + p(12),
            right: rect.left + p(34),
            ..rect
        },
        state.icons,
        if selected {
            theme::ACCENT_FOREGROUND
        } else {
            theme::POPOVER_FOREGROUND
        },
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    rect.left += p(44);
    rect.right -= p(14);
    theme::text(
        dc,
        label(id, state.paused),
        rect,
        state.menu_font,
        if selected {
            theme::ACCENT_FOREGROUND
        } else {
            theme::POPOVER_FOREGROUND
        },
        DT_LEFT | DT_VCENTER | DT_SINGLELINE,
    );
}

pub struct Icon {
    pub handle: HICON,
    owned: bool,
}
impl Icon {
    pub unsafe fn new() -> Self {
        match Self::mouse() {
            Some(handle) => Self {
                handle,
                owned: true,
            },
            None => Self {
                handle: LoadIconW(None, IDI_APPLICATION).unwrap_or_default(),
                owned: false,
            },
        }
    }
    unsafe fn mouse() -> Option<HICON> {
        let instance = GetModuleHandleW(None).ok()?;
        LoadImageW(
            Some(instance.into()),
            PCWSTR(std::ptr::without_provenance(1)),
            IMAGE_ICON,
            GetSystemMetrics(SM_CXSMICON),
            GetSystemMetrics(SM_CYSMICON),
            IMAGE_FLAGS(0),
        )
        .ok()
        .map(|handle| HICON(handle.0))
    }
}
impl Drop for Icon {
    fn drop(&mut self) {
        if self.owned {
            unsafe {
                let _ = DestroyIcon(self.handle);
            }
        }
    }
}

// The native popup window uses square corners on Windows 10. Scope the hook to
// this thread and this menu's lifetime; do not alter menus belonging to other apps.
pub struct PopupCorners(HHOOK);
impl PopupCorners {
    pub unsafe fn install() -> Option<Self> {
        SetWindowsHookExW(
            WH_CALLWNDPROC,
            Some(popup_message),
            None,
            windows::Win32::System::Threading::GetCurrentThreadId(),
        )
        .ok()
        .map(Self)
    }
}
impl Drop for PopupCorners {
    fn drop(&mut self) {
        unsafe {
            let _ = UnhookWindowsHookEx(self.0);
        }
    }
}
const ROUND_SIZE: PCWSTR = w!("MouseSpeedSwitcher.PopupRoundSize");
#[cfg(test)]
pub static ROUNDED_POPUPS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
unsafe extern "system" fn popup_message(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 && lp.0 != 0 {
        let event = &*(lp.0 as *const CWPSTRUCT);
        if matches!(
            event.message,
            WM_WINDOWPOSCHANGED | WM_SHOWWINDOW | WM_NCPAINT
        ) {
            let mut name = [0u16; 16];
            let length = GetClassNameW(event.hwnd, &mut name);
            if name[..length as usize]
                .iter()
                .copied()
                .eq("#32768".encode_utf16())
            {
                round_popup(event.hwnd);
            }
        }
    }
    CallNextHookEx(None, code, wp, lp)
}
unsafe fn round_popup(hwnd: HWND) {
    let mut rect = RECT::default();
    if GetWindowRect(hwnd, &mut rect).is_err() {
        return;
    }
    let width = rect.right - rect.left;
    let height = rect.bottom - rect.top;
    if width < 30 || height < 30 {
        return;
    }
    let stamp = ((width as usize) << 16) | height as usize;
    if GetPropW(hwnd, ROUND_SIZE).0 as usize == stamp {
        return;
    }
    // Set the guard first: changing the region can synchronously send more window messages.
    if SetPropW(hwnd, ROUND_SIZE, Some(HANDLE(stamp as *mut _))).is_err() {
        return;
    }
    let diameter = (16.0 * GetDpiForWindow(hwnd) as f32 / 96.0).round() as i32;
    let region = CreateRoundRectRgn(0, 0, width + 1, height + 1, diameter, diameter);
    if region.is_invalid() {
        let _ = RemovePropW(hwnd, ROUND_SIZE);
        return;
    }
    if SetWindowRgn(hwnd, Some(region), true) == 0 {
        let _ = DeleteObject(region.into());
        let _ = RemovePropW(hwnd, ROUND_SIZE);
    } else {
        #[cfg(test)]
        ROUNDED_POPUPS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

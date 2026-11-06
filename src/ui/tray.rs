#![allow(unsafe_op_in_unsafe_fn)]
use super::*;
use std::result::Result::Ok;

pub fn label(id: u16, paused: bool) -> &'static str {
    match id {
        OPEN => "打开设置",
        PAUSE if paused => "恢复自动切换",
        PAUSE => "暂停自动切换",
        STARTUP => "开机自启",
        EXIT => "退出",
        _ => "",
    }
}

pub unsafe fn create_menu(state: &theme::PaintState) -> Result<HMENU, String> {
    let menu = CreatePopupMenu().map_err(|e| e.to_string())?;
    let result = (|| -> windows::core::Result<()> {
        SetMenuInfo(
            menu,
            &MENUINFO {
                cbSize: size_of::<MENUINFO>() as u32,
                fMask: MIM_BACKGROUND | MIM_STYLE,
                hbrBack: state.white,
                dwStyle: MNS_NOCHECK,
                ..Default::default()
            },
        )?;
        for id in [OPEN, PAUSE, STARTUP, EXIT] {
            if id == EXIT {
                AppendMenuW(menu, MF_SEPARATOR, 0, None)?;
            }
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
                    fState: if id == STARTUP && state.autostart {
                        MFS_CHECKED
                    } else {
                        MFS_UNCHECKED
                    },
                    ..Default::default()
                },
            )?;
        }
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
    theme::fill(dc, &rect, theme::WHITE);
    let selected = item.itemState.0 & ODS_SELECTED.0 != 0;
    if selected {
        let inset = RECT {
            left: rect.left + p(5),
            right: rect.right - p(5),
            top: rect.top + p(3),
            bottom: rect.bottom - p(3),
        };
        theme::rounded(dc, inset, p(10), COLORREF(0x00faf2e9), COLORREF(0x00faf2e9));
    }
    let id = item.itemID as u16;
    let glyph = match id {
        OPEN => "\u{e713}",
        PAUSE if state.paused => "\u{e768}",
        PAUSE => "\u{e769}",
        STARTUP if state.autostart => "\u{e73e}",
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
        theme::INK,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE,
    );
    rect.left += p(44);
    rect.right -= p(14);
    theme::text(
        dc,
        label(id, state.paused),
        rect,
        state.menu_font,
        theme::INK,
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

#![allow(unsafe_op_in_unsafe_fn)] // Win32 wrappers keep raw pointers local to each synchronous call.
use crate::{
    model::{Identity, MouseSystem, Parameters},
    storage::wide,
};
use std::mem::size_of;
use windows::{
    Win32::{
        Devices::{DeviceAndDriverInstallation::*, Properties::*},
        Foundation::{DEVPROPKEY, HANDLE},
        System::Registry::*,
        UI::{Input::*, WindowsAndMessaging::*},
    },
    core::{GUID, PCWSTR, PWSTR},
};

pub struct WindowsMouse;
impl MouseSystem for WindowsMouse {
    fn read(&self) -> Result<Parameters, String> {
        unsafe {
            let mut p = Parameters {
                speed: 0,
                acceleration: [0; 3],
            };
            SystemParametersInfoW(
                SPI_GETMOUSESPEED,
                0,
                Some((&mut p.speed as *mut i32).cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
            .map_err(|e| e.to_string())?;
            SystemParametersInfoW(
                SPI_GETMOUSE,
                0,
                Some(p.acceleration.as_mut_ptr().cast()),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            )
            .map_err(|e| e.to_string())?;
            Ok(p)
        }
    }
    fn write(&mut self, p: Parameters) -> Result<(), String> {
        if !p.valid() {
            return Err("鼠标参数超出允许范围".into());
        }
        let old = self.read()?;
        if let Err(e) = set_parameters(p) {
            return Err(match set_parameters(old) {
                Ok(()) => format!("应用失败，已恢复原参数：{e}"),
                Err(rollback) => format!("应用失败：{e}；恢复失败：{rollback}"),
            });
        }
        Ok(())
    }
}
fn set_parameters(mut p: Parameters) -> Result<(), String> {
    unsafe {
        // Do not persist each device switch into the user's Windows registry settings.
        SystemParametersInfoW(
            SPI_SETMOUSE,
            0,
            Some(p.acceleration.as_mut_ptr().cast()),
            SPIF_SENDCHANGE,
        )
        .map_err(|e| e.to_string())?;
        SystemParametersInfoW(
            SPI_SETMOUSESPEED,
            0,
            Some(p.speed as usize as *mut _),
            SPIF_SENDCHANGE,
        )
        .map_err(|e| e.to_string())?;
        if WindowsMouse.read()? != p {
            return Err("系统未保留请求的鼠标参数".into());
        }
        Ok(())
    }
}

pub fn enumerate() -> Result<Vec<(usize, Identity)>, String> {
    unsafe {
        let mut count = 0;
        if GetRawInputDeviceList(None, &mut count, size_of::<RAWINPUTDEVICELIST>() as u32)
            == u32::MAX
        {
            return Err("无法枚举输入设备".into());
        }
        // The list may grow between calls; retry without accepting a partial list.
        for _ in 0..3 {
            let mut entries = vec![RAWINPUTDEVICELIST::default(); count as usize];
            let n = GetRawInputDeviceList(
                Some(entries.as_mut_ptr()),
                &mut count,
                size_of::<RAWINPUTDEVICELIST>() as u32,
            );
            if n == u32::MAX {
                continue;
            }
            return Ok(entries
                .into_iter()
                .take(n as usize)
                .filter(|d| d.dwType == RIM_TYPEMOUSE)
                .filter_map(|d| identity(d.hDevice).map(|i| (d.hDevice.0 as usize, i)))
                .collect());
        }
        Err("设备列表持续变化，请稍后重试".into())
    }
}

fn utf16(bytes: &[u8]) -> String {
    let words: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .take_while(|w| *w != 0)
        .collect();
    String::from_utf16_lossy(&words)
}
unsafe fn node_property(node: u32, key: &DEVPROPKEY) -> Option<(DEVPROPTYPE, Vec<u8>)> {
    let mut ty = DEVPROPTYPE::default();
    let mut len = 0;
    CM_Get_DevNode_PropertyW(node, key, &mut ty, None, &mut len, 0);
    if len == 0 || len > 65536 {
        return None;
    }
    let mut data = vec![0; len as usize];
    if CM_Get_DevNode_PropertyW(node, key, &mut ty, Some(data.as_mut_ptr()), &mut len, 0)
        != CR_SUCCESS
    {
        return None;
    }
    data.truncate(len as usize);
    Some((ty, data))
}
unsafe fn node_string(node: u32, key: &DEVPROPKEY) -> Option<String> {
    let (ty, data) = node_property(node, key)?;
    if ty != DEVPROP_TYPE_STRING {
        return None;
    }
    let s = utf16(&data);
    if s.is_empty() { None } else { Some(s) }
}
pub fn identity(handle: HANDLE) -> Option<Identity> {
    unsafe {
        if handle.is_invalid() {
            return None;
        }
        let mut len = 0;
        if GetRawInputDeviceInfoW(Some(handle), RIDI_DEVICENAME, None, &mut len) == u32::MAX
            || len == 0
        {
            return None;
        }
        let mut path = vec![0u16; len as usize + 1];
        if GetRawInputDeviceInfoW(
            Some(handle),
            RIDI_DEVICENAME,
            Some(path.as_mut_ptr().cast()),
            &mut len,
        ) == u32::MAX
        {
            return None;
        }
        let interface = String::from_utf16_lossy(
            &path[..path.iter().position(|x| *x == 0).unwrap_or(path.len())],
        );
        let upper = interface.to_uppercase();
        if upper.contains("RDP_MOU") || upper.contains("RDP_MOUSE") || !upper.starts_with("\\\\?\\")
        {
            return None;
        }
        let mut ty = DEVPROPTYPE::default();
        let mut size = 0;
        CM_Get_Device_Interface_PropertyW(
            PCWSTR(path.as_ptr()),
            &DEVPKEY_Device_InstanceId,
            &mut ty,
            None,
            &mut size,
            0,
        );
        if size == 0 || size > 65536 {
            return None;
        }
        let mut bytes = vec![0u8; size as usize];
        if CM_Get_Device_Interface_PropertyW(
            PCWSTR(path.as_ptr()),
            &DEVPKEY_Device_InstanceId,
            &mut ty,
            Some(bytes.as_mut_ptr()),
            &mut size,
            0,
        ) != CR_SUCCESS
            || ty != DEVPROP_TYPE_STRING
        {
            return None;
        }
        let instance = utf16(&bytes);
        let mut instance_wide = wide(&instance);
        let mut node = 0;
        if CM_Locate_DevNodeW(
            &mut node,
            PWSTR(instance_wide.as_mut_ptr()),
            CM_LOCATE_DEVNODE_NORMAL,
        ) != CR_SUCCESS
        {
            return None;
        }
        let mut name = node_string(node, &DEVPKEY_Device_FriendlyName)
            .or_else(|| node_string(node, &DEVPKEY_Device_DeviceDesc))
            .unwrap_or_else(|| "HID 鼠标".into());
        let container = node_property(node, &DEVPKEY_Device_ContainerId).and_then(|(ty, bytes)| {
            if ty != DEVPROP_TYPE_GUID || bytes.len() != size_of::<GUID>() {
                return None;
            }
            let id = std::ptr::read_unaligned(bytes.as_ptr().cast::<GUID>());
            if id == GUID::zeroed() || id == GUID::from_u128(u128::MAX) {
                None
            } else {
                Some(format!("{id:?}"))
            }
        });
        // Windows exposes USB serials as the instance suffix only when UNIQUEID is set.
        // Walk parents rather than mistaking port-generated HID suffixes for serial numbers.
        let mut serial = None;
        let mut product_name = None;
        let mut ancestor = node;
        for _ in 0..8 {
            if product_name.is_none() {
                product_name = node_string(ancestor, &DEVPKEY_Device_BusReportedDeviceDesc);
            }
            if let Some((ty, bytes)) = node_property(ancestor, &DEVPKEY_Device_Capabilities)
                && ty == DEVPROP_TYPE_UINT32
                && bytes.len() == 4
                && u32::from_le_bytes(bytes.try_into().ok()?) & 0x10 != 0
            {
                let mut buf = [0u16; 1024];
                if CM_Get_Device_IDW(ancestor, &mut buf, 0) == CR_SUCCESS {
                    let id = String::from_utf16_lossy(
                        &buf[..buf.iter().position(|v| *v == 0).unwrap_or(buf.len())],
                    );
                    if id.to_uppercase().starts_with("USB\\") && !id.contains("&MI_") {
                        serial = id.rsplit('\\').next().map(str::to_owned);
                        if let Some(friendly) = node_string(ancestor, &DEVPKEY_Device_FriendlyName)
                        {
                            name = friendly;
                        }
                        break;
                    }
                }
            }
            let mut parent = 0;
            if CM_Get_Parent(&mut parent, ancestor, 0) != CR_SUCCESS {
                break;
            }
            ancestor = parent;
        }
        if let Some(product) = product_name {
            name = product;
        }
        // Generic HID names otherwise make multiple devices indistinguishable in the profile list.
        let hardware = instance.split('\\').nth(1).unwrap_or(&instance);
        let hardware = hardware.split('&').take(2).collect::<Vec<_>>().join("&");
        name = format!("{name} [{hardware}]");
        let key = Identity::stable_key(container.as_deref(), &instance, &interface);
        Some(Identity {
            key,
            name,
            instance,
            interface,
            container,
            serial,
        })
    }
}

const RUN_KEY: PCWSTR = windows::core::w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_VALUE: PCWSTR = windows::core::w!("MouseSpeedSwitcher");
pub fn autostart() -> bool {
    unsafe {
        let mut len = 0;
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            RUN_VALUE,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut len),
        )
        .is_ok()
    }
}
pub fn set_autostart(enabled: bool) -> Result<(), String> {
    unsafe {
        let mut key = HKEY::default();
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
        .ok()
        .map_err(|e| e.to_string())?;
        let result = if enabled {
            let exe = std::env::current_exe().map_err(|e| e.to_string());
            match exe {
                Ok(exe) => {
                    let value = wide(&format!("\"{}\" --background", exe.display()));
                    let bytes =
                        std::slice::from_raw_parts(value.as_ptr().cast::<u8>(), value.len() * 2);
                    RegSetValueExW(key, RUN_VALUE, None, REG_SZ, Some(bytes))
                        .ok()
                        .map_err(|e| e.to_string())
                }
                Err(e) => Err(e),
            }
        } else {
            let status = RegDeleteValueW(key, RUN_VALUE);
            if status == windows::Win32::Foundation::ERROR_FILE_NOT_FOUND {
                Ok(())
            } else {
                status.ok().map_err(|e| e.to_string())
            }
        };
        let _ = RegCloseKey(key);
        result
    }
}

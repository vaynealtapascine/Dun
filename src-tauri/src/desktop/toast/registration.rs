//! Per-user registration that lets an unpackaged EXE own toast notifications
//! without a Start Menu shortcut:
//!
//! - `HKCU\Software\Classes\AppUserModelId\<aumid>` gives toasts a name and
//!   icon and names our COM activator (`CustomActivator`).
//! - `HKCU\Software\Classes\CLSID\{clsid}\LocalServer32` tells COM how to start
//!   Dun when a toast button is pressed while it isn't running.
//!
//! This mirrors the Windows Community Toolkit's no-shortcut registration. It is
//! rewritten on every start so a moved EXE (dev vs installed) stays correct.

use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_WRITE, REG_OPTION_NON_VOLATILE, REG_SZ,
};

use super::Identity;

/// Argument COM passes when it launches us to deliver an activation.
pub const TOAST_ACTIVATED_ARG: &str = "-ToastActivated";

pub fn register(
    id: &Identity,
    exe: &std::path::Path,
    icon: &std::path::Path,
) -> Result<(), String> {
    let aumid_key = format!(r"Software\Classes\AppUserModelId\{}", id.aumid);
    let clsid = format!("{{{}}}", id.clsid_string());

    set_string(&aumid_key, "DisplayName", id.display_name)?;
    set_string(&aumid_key, "IconUri", &icon.to_string_lossy())?;
    set_string(&aumid_key, "IconBackgroundColor", "FFDDDDDD")?;
    set_string(&aumid_key, "CustomActivator", &clsid)?;

    let server_key = format!(r"Software\Classes\CLSID\{clsid}\LocalServer32");
    let command = format!("\"{}\" {}", exe.display(), TOAST_ACTIVATED_ARG);
    set_string(&server_key, "", &command)?;
    Ok(())
}

/// Removes both keys (used by uninstall hooks and the dev "reset" path).
#[allow(dead_code)]
pub fn unregister(id: &Identity) -> Result<(), String> {
    for key in [
        format!(r"Software\Classes\AppUserModelId\{}", id.aumid),
        format!(r"Software\Classes\CLSID\{{{}}}", id.clsid_string()),
    ] {
        let status = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(key.as_str())) };
        if status != ERROR_SUCCESS && status.0 != 2 {
            return Err(format!("delete HKCU\\{key}: {status:?}"));
        }
    }
    Ok(())
}

fn set_string(key: &str, name: &str, value: &str) -> Result<(), String> {
    let mut hkey = HKEY::default();
    let status = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            &HSTRING::from(key),
            None,
            PCWSTR::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            None,
            &mut hkey,
            None,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(format!("create HKCU\\{key}: {status:?}"));
    }

    // REG_SZ data is UTF-16 including the terminating NUL.
    let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes: Vec<u8> = wide.iter().flat_map(|w| w.to_le_bytes()).collect();
    let value_name = HSTRING::from(name);
    let name_ptr = if name.is_empty() {
        PCWSTR::null()
    } else {
        PCWSTR(value_name.as_ptr())
    };
    let status = unsafe { RegSetValueExW(hkey, name_ptr, None, REG_SZ, Some(&bytes)) };
    unsafe {
        let _ = RegCloseKey(hkey);
    }
    if status != ERROR_SUCCESS {
        return Err(format!("set HKCU\\{key}\\{name}: {status:?}"));
    }
    Ok(())
}

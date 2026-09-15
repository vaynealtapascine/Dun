//! Windows toast notifications with Done/Snooze buttons that work while Dun
//! runs, after it quits, and from Action Center.

pub mod xml;

#[cfg(windows)]
mod activator;
#[cfg(windows)]
pub mod registration;

#[cfg(windows)]
pub use imp::*;

/// Who we are to the notification platform. Dev and installed builds use
/// different ids so neither steals the other's clicks.
#[derive(Debug, Clone, Copy)]
pub struct Identity {
    pub aumid: &'static str,
    pub display_name: &'static str,
    pub clsid: u128,
}

impl Identity {
    pub const RELEASE: Identity = Identity {
        aumid: "app.dun",
        display_name: "Dun",
        clsid: 0x5E207284_DEBB_4DA4_91F5_F05AD60E7F18,
    };
    pub const DEV: Identity = Identity {
        aumid: "app.dun.dev",
        display_name: "Dun (dev)",
        clsid: 0xD0015A10_DE2B_4104_8D25_A33092340D48,
    };

    pub fn current() -> Identity {
        if cfg!(debug_assertions) {
            Self::DEV
        } else {
            Self::RELEASE
        }
    }

    pub fn clsid_string(&self) -> String {
        let v = self.clsid;
        format!(
            "{:08X}-{:04X}-{:04X}-{:04X}-{:012X}",
            (v >> 96) as u32,
            (v >> 80) as u16,
            (v >> 64) as u16,
            (v >> 48) as u16,
            v & 0xFFFF_FFFF_FFFF
        )
    }
}

#[cfg(windows)]
mod imp {
    use std::path::Path;

    use windows::core::{GUID, HSTRING};
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
    use windows::UI::Notifications::{
        NotificationSetting, ToastNotification, ToastNotificationManager,
    };

    use super::xml::Toast;
    use super::{activator, registration, Identity};

    pub use activator::Sink;

    /// Registers identity and activator. Call as early as possible in `main`.
    pub fn init(id: Identity, icon: &Path, sink: Sink) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| format!("current_exe: {e}"))?;
        registration::register(&id, &exe, icon)?;
        activator::start(GUID::from_u128(id.clsid), sink)
    }

    /// WinRT needs COM on the calling thread; Tauri's threads may not have it.
    fn ensure_com() {
        // S_FALSE / RPC_E_CHANGED_MODE just mean the thread is already initialised.
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    }

    /// Shows `toast`. With `popup` false it goes straight to Notification
    /// Center without a banner (used for silent alerts). A toast with the same
    /// tag and group is replaced; note Windows does not re-pop the banner for
    /// a replacement once the user dismissed it, so callers that need a fresh
    /// banner remove first (see docs/spikes/windows-toast.md).
    pub fn show(id: Identity, toast: &Toast, popup: bool) -> Result<(), String> {
        ensure_com();
        let doc = XmlDocument::new().map_err(|e| e.to_string())?;
        doc.LoadXml(&HSTRING::from(toast.to_xml()))
            .map_err(|e| format!("toast XML rejected: {e}"))?;
        let notification =
            ToastNotification::CreateToastNotification(&doc).map_err(|e| e.to_string())?;
        notification
            .SetSuppressPopup(!popup)
            .map_err(|e| e.to_string())?;
        notification
            .SetTag(&HSTRING::from(toast.tag.as_str()))
            .map_err(|e| e.to_string())?;
        notification
            .SetGroup(&HSTRING::from(toast.group.as_str()))
            .map_err(|e| e.to_string())?;
        let notifier =
            ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(id.aumid))
                .map_err(|e| e.to_string())?;
        notifier
            .Show(&notification)
            .map_err(|e| format!("Show: {e}"))
    }

    /// Removes a toast from the screen and Action Center.
    pub fn remove(id: Identity, tag: &str, group: &str) -> Result<(), String> {
        ensure_com();
        ToastNotificationManager::History()
            .and_then(|h| {
                h.RemoveGroupedTagWithId(
                    &HSTRING::from(tag),
                    &HSTRING::from(group),
                    &HSTRING::from(id.aumid),
                )
            })
            .map_err(|e| format!("remove toast {group}/{tag}: {e}"))
    }

    /// `Ok(None)` when toasts are enabled, otherwise why they are not.
    pub fn blocked_reason(id: Identity) -> Result<Option<&'static str>, String> {
        ensure_com();
        let setting = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(id.aumid))
            .and_then(|n| n.Setting())
            .map_err(|e| e.to_string())?;
        Ok(match setting {
            NotificationSetting::Enabled => None,
            NotificationSetting::DisabledForApplication => {
                Some("Dun's notifications are turned off in Windows Settings")
            }
            NotificationSetting::DisabledForUser => {
                Some("Notifications are turned off for this Windows account")
            }
            NotificationSetting::DisabledByGroupPolicy => {
                Some("Notifications are disabled by group policy")
            }
            NotificationSetting::DisabledByManifest => {
                Some("Notifications are disabled by the app manifest")
            }
            _ => Some("Notifications are unavailable"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::Identity;

    #[test]
    fn clsid_formats_as_registry_guid() {
        assert_eq!(
            Identity::RELEASE.clsid_string(),
            "5E207284-DEBB-4DA4-91F5-F05AD60E7F18"
        );
        assert_eq!(
            Identity::DEV.clsid_string(),
            "D0015A10-DE2B-4104-8D25-A33092340D48"
        );
    }
}

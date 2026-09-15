//! The one-time Windows Firewall rule for sync.
//!
//! If Dun just started listening, Windows would show its own prompt — and if
//! a non-admin answers it, or anyone cancels, Windows silently records *block*
//! rules for the exe that are painful to find later. So Dun adds the allow
//! rule itself first, through one elevated helper run, and only then listens.

use std::process::Command;

pub const RULE_NAME: &str = "Dun Sync (TCP-In)";
/// Launch argument that runs the elevated helper.
pub const SETUP_ARG: &str = "--setup-firewall";

/// Don't flash a console window for netsh.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn netsh(args: &[&str]) -> Result<std::process::Output, String> {
    let mut command = Command::new("netsh");
    command.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
        .output()
        .map_err(|e| format!("couldn't run netsh: {e}"))
}

/// Whether our allow rule is already there.
pub fn rule_exists() -> bool {
    netsh(&[
        "advfirewall",
        "firewall",
        "show",
        "rule",
        &format!("name={RULE_NAME}"),
    ])
    .is_ok_and(|out| out.status.success())
}

/// Runs inside the elevated helper: clear stale block rules for this exe, then
/// allow the port on private and domain networks, from the local subnet and
/// the Tailscale range only.
pub fn apply(port: u16) -> Result<(), String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("couldn't find Dun's path: {e}"))?
        .display()
        .to_string();

    // Windows may have recorded blocks from an earlier prompt; they win over
    // allows, so they have to go first. Missing rules are not an error.
    let _ = netsh(&[
        "advfirewall",
        "firewall",
        "delete",
        "rule",
        "name=all",
        &format!("program={exe}"),
    ]);
    let _ = netsh(&[
        "advfirewall",
        "firewall",
        "delete",
        "rule",
        &format!("name={RULE_NAME}"),
    ]);

    let output = netsh(&[
        "advfirewall",
        "firewall",
        "add",
        "rule",
        &format!("name={RULE_NAME}"),
        "dir=in",
        "action=allow",
        &format!("program={exe}"),
        "enable=yes",
        "profile=private,domain",
        "protocol=tcp",
        &format!("localport={port}"),
        "remoteip=localsubnet,100.64.0.0/10",
    ])?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}

/// Makes sure the rule exists, asking for elevation once if it doesn't.
/// Returns `Ok(false)` when the rule was already there.
#[cfg(windows)]
pub fn ensure(port: u16) -> Result<bool, String> {
    if rule_exists() {
        return Ok(false);
    }
    elevate_self(port)?;
    if rule_exists() {
        Ok(true)
    } else {
        Err("Windows Firewall wasn't updated, so the phone probably can't reach this PC".into())
    }
}

#[cfg(not(windows))]
pub fn ensure(_port: u16) -> Result<bool, String> {
    Ok(false)
}

/// Re-runs this exe as administrator with `--setup-firewall <port>` and waits.
#[cfg(windows)]
fn elevate_self(port: u16) -> Result<(), String> {
    use windows::core::{HSTRING, PCWSTR};
    use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{WaitForSingleObject, INFINITE};
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = HSTRING::from(exe.as_os_str());
    let verb = HSTRING::from("runas");
    let params = HSTRING::from(format!("{SETUP_ARG} {port}"));

    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(exe.as_ptr()),
        lpParameters: PCWSTR(params.as_ptr()),
        nShow: SW_HIDE.0,
        ..Default::default()
    };

    unsafe { ShellExecuteExW(&mut info) }
        .map_err(|_| "You didn't allow the change, so sync stays off".to_string())?;

    let process = HANDLE(info.hProcess.0);
    if process.is_invalid() {
        return Ok(());
    }
    let waited = unsafe { WaitForSingleObject(process, INFINITE) };
    unsafe {
        let _ = CloseHandle(process);
    }
    if waited == WAIT_OBJECT_0 {
        Ok(())
    } else {
        Err("the firewall helper didn't finish".into())
    }
}

/// `--setup-firewall <port>`: the port to allow, if this is the helper run.
pub fn setup_request(args: &[String]) -> Option<u16> {
    let index = args
        .iter()
        .position(|a| a.eq_ignore_ascii_case(SETUP_ARG))?;
    Some(
        args.get(index + 1)
            .and_then(|p| p.parse().ok())
            .unwrap_or(dun_core::sync::protocol::PORT),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_the_helper_invocation() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            setup_request(&args(&["dun.exe", "--setup-firewall", "47823"])),
            Some(47823)
        );
        assert_eq!(
            setup_request(&args(&["dun.exe", "--setup-firewall"])),
            Some(dun_core::sync::protocol::PORT),
            "a missing port falls back to the default"
        );
        assert_eq!(setup_request(&args(&["dun.exe", "--hidden"])), None);
    }

    #[cfg(windows)]
    #[test]
    fn checking_for_the_rule_doesnt_need_admin() {
        // Either answer is fine; it must not hang or panic.
        let _ = rule_exists();
    }
}

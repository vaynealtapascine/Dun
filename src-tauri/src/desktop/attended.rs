//! Whether someone is at this PC: the session is unlocked and there was
//! keyboard or mouse input recently. Polled by the scheduler loop; both calls
//! are cheap and need no window or message loop.

/// `true` when unlocked and input happened within `idle_threshold_s`.
/// Errs toward "attended" when Windows won't say, so a failed probe never
/// hands a ring to the phone by mistake.
pub fn is_attended(idle_threshold_s: u32) -> bool {
    let locked = is_locked().unwrap_or(false);
    let idle_ms = idle_ms().unwrap_or(0);
    !locked && idle_ms < u64::from(idle_threshold_s) * 1_000
}

#[cfg(windows)]
pub fn idle_ms() -> Option<u64> {
    use windows::Win32::System::SystemInformation::GetTickCount;
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    let mut info = LASTINPUTINFO {
        cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32,
        dwTime: 0,
    };
    if !unsafe { GetLastInputInfo(&mut info) }.as_bool() {
        return None;
    }
    // Both are 32-bit millisecond tick counts that wrap every ~49.7 days.
    let now = unsafe { GetTickCount() };
    Some(u64::from(now.wrapping_sub(info.dwTime)))
}

#[cfg(windows)]
pub fn is_locked() -> Option<bool> {
    use windows::core::PWSTR;
    use windows::Win32::System::RemoteDesktop::{
        WTSFreeMemory, WTSQuerySessionInformationW, WTSSessionInfoEx, WTSINFOEXW,
        WTS_CURRENT_SERVER_HANDLE, WTS_CURRENT_SESSION, WTS_SESSIONSTATE_LOCK,
    };

    let mut buffer = PWSTR::null();
    let mut bytes = 0u32;
    unsafe {
        WTSQuerySessionInformationW(
            Some(WTS_CURRENT_SERVER_HANDLE),
            WTS_CURRENT_SESSION,
            WTSSessionInfoEx,
            &mut buffer,
            &mut bytes,
        )
        .ok()?;
    }
    if buffer.is_null() || (bytes as usize) < std::mem::size_of::<WTSINFOEXW>() {
        return None;
    }
    let info = unsafe { &*(buffer.0 as *const WTSINFOEXW) };
    let flags = if info.Level == 1 {
        Some(unsafe { info.Data.WTSInfoExLevel1.SessionFlags } as u32)
    } else {
        None
    };
    unsafe { WTSFreeMemory(buffer.0 as *mut core::ffi::c_void) };
    flags.map(|f| f == WTS_SESSIONSTATE_LOCK)
}

#[cfg(not(windows))]
pub fn idle_ms() -> Option<u64> {
    None
}

#[cfg(not(windows))]
pub fn is_locked() -> Option<bool> {
    None
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    fn probes_answer_on_a_desktop_session() {
        // The test runner has a session; the probes must return something.
        assert!(super::idle_ms().is_some());
        assert!(super::is_locked().is_some());
    }
}

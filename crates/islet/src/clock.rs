//! Local wall-clock time from the OS (time zone and DST included).

use islet_core::clock::LocalTime;

#[cfg(unix)]
pub fn now() -> LocalTime {
    // SAFETY: time() and localtime_r() with valid out-pointers; `tm` is
    // fully initialised by localtime_r before it is read.
    unsafe {
        let secs = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&secs, &mut tm);
        LocalTime {
            year: tm.tm_year + 1900,
            month: (tm.tm_mon + 1) as u8,
            day: tm.tm_mday as u8,
            weekday: tm.tm_wday as u8,
            hour: tm.tm_hour as u8,
            minute: tm.tm_min as u8,
            second: tm.tm_sec as u8,
        }
    }
}

#[cfg(windows)]
pub fn now() -> LocalTime {
    // SAFETY: GetLocalTime has no preconditions.
    let st = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    LocalTime {
        year: st.wYear as i32,
        month: st.wMonth as u8,
        day: st.wDay as u8,
        weekday: st.wDayOfWeek as u8,
        hour: st.wHour as u8,
        minute: st.wMinute as u8,
        second: st.wSecond as u8,
    }
}

/// 24-hour clock unless the user's locale conventionally uses 12-hour.
/// `ISLET_CLOCK=12` or `=24` overrides it until Settings exists.
pub fn prefers_24h() -> bool {
    match std::env::var("ISLET_CLOCK").as_deref() {
        Ok("12") => false,
        Ok("24") => true,
        _ => {
            let locale = std::env::var("LC_TIME")
                .or_else(|_| std::env::var("LC_ALL"))
                .or_else(|_| std::env::var("LANG"))
                .unwrap_or_default();
            islet_core::clock::locale_prefers_24h(&locale)
        }
    }
}

/// Seconds since the Unix epoch, now.
pub fn epoch() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64())
}

/// This computer's UTC offset now, in seconds (time zone and DST included):
/// the local wall clock minus UTC, rounded to the quarter hour.
pub fn utc_offset() -> i64 {
    let local = now();
    let wall = islet_core::alarm::minute_key(&local) * 60 + i64::from(local.second);
    let diff = wall - epoch() as i64;
    (diff as f64 / 900.0).round() as i64 * 900
}

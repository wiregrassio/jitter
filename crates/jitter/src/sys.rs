//! The few system calls Jitter makes beyond reading files. Every `unsafe` block in the daemon
//! lives here, each a single libc call on a stack value.

use std::ffi::CString;
use std::path::Path;

/// `sysconf(_SC_CLK_TCK)`: the tick unit of `/proc/stat`. Linux fixes it at 100 on every
/// mainstream architecture, but it is read, not assumed.
pub fn user_hz() -> u32 {
    // SAFETY: sysconf takes an integer and returns an integer.
    let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    u32::try_from(hz).unwrap_or(0)
}

/// `sysconf(_SC_PAGESIZE)`, for `/proc/self/statm`. 4 KiB on x86; 4, 16, or 64 KiB on arm64.
pub fn page_size() -> u64 {
    // SAFETY: sysconf takes an integer and returns an integer.
    let size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    u64::try_from(size).unwrap_or(0)
}

/// Filesystem capacity in bytes.
#[derive(Debug, Clone, Copy)]
pub struct Capacity {
    /// Size of the filesystem.
    pub total_bytes: u64,
    /// Space available to unprivileged users.
    pub available_bytes: u64,
}

/// `statvfs(path)`.
pub fn statvfs(path: &Path) -> std::io::Result<Capacity> {
    use std::os::unix::ffi::OsStrExt;
    let c_path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| std::io::Error::other("path contains a NUL byte"))?;
    // SAFETY: zeroed is a valid statvfs; the call writes it and reads only the NUL-terminated path.
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: as above.
    if unsafe { libc::statvfs(c_path.as_ptr(), &mut st) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let frsize = st.f_frsize as u64;
    Ok(Capacity {
        total_bytes: st.f_blocks as u64 * frsize,
        available_bytes: st.f_bavail as u64 * frsize,
    })
}

/// CPU time this process has used, user plus system, in microseconds.
pub fn self_cpu_us() -> std::io::Result<u64> {
    // SAFETY: zeroed is a valid rusage; the call only writes it.
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: as above.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    // rusage times are never negative; a negative field would be a kernel bug, read as zero.
    let us = |tv: libc::timeval| {
        u64::try_from(tv.tv_sec).unwrap_or(0) * 1_000_000 + u64::try_from(tv.tv_usec).unwrap_or(0)
    };
    Ok(us(ru.ru_utime) + us(ru.ru_stime))
}

/// The kernel clock discipline state, from `adjtimex` with no modes set (read only).
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy)]
pub struct ClockState {
    /// Current offset from the reference, seconds.
    pub offset_seconds: f64,
    /// Frequency correction, parts per million.
    pub frequency_ppm: f64,
    /// Kernel's maximum error bound, seconds. Grows while unsynchronized.
    pub max_error_seconds: f64,
    /// False when the kernel reports `TIME_ERROR`: no time daemon is disciplining the clock.
    pub synchronized: bool,
}

/// Read the kernel clock state. Works with any NTP or PTP client, because every client steers
/// the clock through this interface.
#[cfg(target_os = "linux")]
pub fn clock_state() -> std::io::Result<ClockState> {
    // SAFETY: zeroed is a valid timex with modes = 0, which makes adjtimex read only.
    let mut tx: libc::timex = unsafe { std::mem::zeroed() };
    // SAFETY: as above.
    let state = unsafe { libc::adjtimex(&mut tx) };
    if state < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let offset_unit = if tx.status & libc::STA_NANO != 0 {
        1e-9
    } else {
        1e-6
    };
    Ok(ClockState {
        offset_seconds: tx.offset as f64 * offset_unit,
        // `freq` is ppm with a 16-bit binary fraction.
        frequency_ppm: tx.freq as f64 / 65536.0,
        max_error_seconds: tx.maxerror as f64 * 1e-6,
        synchronized: state != libc::TIME_ERROR,
    })
}

/// Set this thread's timer slack. Applies to the calling thread only, so the driver calls it on
/// the loop thread.
#[cfg(target_os = "linux")]
pub fn set_timer_slack(ns: u64) -> std::io::Result<()> {
    let ns = libc::c_ulong::try_from(ns).map_err(|_| std::io::Error::other("slack too large"))?;
    // SAFETY: PR_SET_TIMERSLACK takes one integer argument.
    if unsafe { libc::prctl(libc::PR_SET_TIMERSLACK, ns, 0, 0, 0) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

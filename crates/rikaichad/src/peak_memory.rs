// SPDX-License-Identifier: AGPL-3.0-or-later
//! Peak resident memory of this process, for import and bench reports.

/// Peak resident set size in bytes (`VmHWM` in /proc/self/status) on
/// Linux; `None` elsewhere or if it cannot be read.
#[cfg(target_os = "linux")]
pub fn peak_rss_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    let kib: u64 = line
        .trim_start_matches("VmHWM:")
        .trim()
        .trim_end_matches("kB")
        .trim()
        .parse()
        .ok()?;
    Some(kib * 1024)
}

/// Not available on this platform.
#[cfg(not(target_os = "linux"))]
pub fn peak_rss_bytes() -> Option<u64> {
    None
}

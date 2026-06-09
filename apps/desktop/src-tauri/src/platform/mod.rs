#[cfg(target_os = "macos")]
pub const PLATFORM_NAME: &str = "macOS";

#[cfg(target_os = "windows")]
pub const PLATFORM_NAME: &str = "Windows";

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub const PLATFORM_NAME: &str = "Unknown";


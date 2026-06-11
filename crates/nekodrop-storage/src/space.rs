use std::path::Path;

use nekodrop_core::{NekoDropError, NekoDropResult};

use crate::resume::ResumePlan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceiveSpaceStatus {
    pub required_bytes: u64,
    pub available_bytes: u64,
}

pub fn remaining_receive_bytes(total_bytes: u64, resume_plan: &ResumePlan) -> u64 {
    total_bytes.saturating_sub(resume_plan.total_received_bytes())
}

pub fn check_receive_space(
    receive_dir: &Path,
    total_bytes: u64,
    resume_plan: &ResumePlan,
) -> NekoDropResult<ReceiveSpaceStatus> {
    let available_bytes = available_space(receive_dir)?;
    check_receive_space_with_available_bytes(total_bytes, resume_plan, available_bytes)
}

pub fn check_receive_space_with_available_bytes(
    total_bytes: u64,
    resume_plan: &ResumePlan,
    available_bytes: u64,
) -> NekoDropResult<ReceiveSpaceStatus> {
    let required_bytes = remaining_receive_bytes(total_bytes, resume_plan);
    if available_bytes < required_bytes {
        return Err(NekoDropError::Storage(format!(
            "insufficient receive space: need {required_bytes} bytes, available {available_bytes} bytes"
        )));
    }

    Ok(ReceiveSpaceStatus {
        required_bytes,
        available_bytes,
    })
}

#[cfg(unix)]
fn available_space(path: &Path) -> NekoDropResult<u64> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    let path_bytes = path.as_os_str().as_bytes();
    let path = CString::new(path_bytes).map_err(|_| {
        NekoDropError::Storage("failed to inspect available space: path contains NUL".into())
    })?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    let result = unsafe { libc::statvfs(path.as_ptr(), stats.as_mut_ptr()) };
    if result != 0 {
        return Err(NekoDropError::Storage(format!(
            "failed to inspect available space: {}",
            std::io::Error::last_os_error()
        )));
    }
    let stats = unsafe { stats.assume_init() };
    Ok((stats.f_bavail as u64).saturating_mul(stats.f_frsize as u64))
}

#[cfg(windows)]
fn available_space(path: &Path) -> NekoDropResult<u64> {
    use std::os::windows::ffi::OsStrExt;

    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let path = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let mut available_bytes = 0_u64;
    let success = unsafe {
        GetDiskFreeSpaceExW(
            path.as_ptr(),
            &mut available_bytes,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if success == 0 {
        return Err(NekoDropError::Storage(format!(
            "failed to inspect available space: {}",
            std::io::Error::last_os_error()
        )));
    }

    Ok(available_bytes)
}

#[cfg(not(any(unix, windows)))]
fn available_space(_path: &Path) -> NekoDropResult<u64> {
    Err(NekoDropError::Storage(
        "failed to inspect available space: unsupported platform".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resume::ResumeFileState;

    #[test]
    fn remaining_receive_bytes_subtracts_completed_and_partial_resume_bytes() {
        let resume_plan = ResumePlan {
            transfer_id: "transfer-a".to_string(),
            files: vec![
                ResumeFileState {
                    path: "drop/done.bin".to_string(),
                    received_bytes: 40,
                    expected_bytes: 40,
                    sha256: None,
                    completed: true,
                },
                ResumeFileState {
                    path: "drop/partial.bin".to_string(),
                    received_bytes: 15,
                    expected_bytes: 60,
                    sha256: None,
                    completed: false,
                },
            ],
        };

        assert_eq!(remaining_receive_bytes(120, &resume_plan), 65);
    }

    #[test]
    fn receive_space_check_rejects_when_available_bytes_are_insufficient() {
        let resume_plan = ResumePlan {
            transfer_id: "transfer-a".to_string(),
            files: vec![ResumeFileState {
                path: "drop/partial.bin".to_string(),
                received_bytes: 25,
                expected_bytes: 100,
                sha256: None,
                completed: false,
            }],
        };

        let error = check_receive_space_with_available_bytes(100, &resume_plan, 70).unwrap_err();

        assert!(error.to_string().contains("insufficient receive space"));
        assert!(error.to_string().contains("need 75 bytes"));
        assert!(error.to_string().contains("available 70 bytes"));
    }
}

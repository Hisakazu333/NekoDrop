use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use nekodrop_core::{NekoDropError, NekoDropResult};
use sha2::{Digest, Sha256};

use crate::receive_dir::safe_join_receive_path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReceivedFile {
    pub path: PathBuf,
    pub manifest_path: String,
    pub bytes_written: u64,
    pub sha256: String,
    pub verified: bool,
}

pub fn write_received_file<R: Read>(
    receive_dir: &Path,
    manifest_path: &str,
    expected_size: u64,
    expected_sha256: &str,
    reader: &mut R,
) -> NekoDropResult<ReceivedFile> {
    write_received_file_with_progress(
        receive_dir,
        manifest_path,
        expected_size,
        expected_sha256,
        reader,
        |_| {},
    )
}

pub fn write_received_file_with_progress<R, F>(
    receive_dir: &Path,
    manifest_path: &str,
    expected_size: u64,
    expected_sha256: &str,
    reader: &mut R,
    on_progress: F,
) -> NekoDropResult<ReceivedFile>
where
    R: Read,
    F: FnMut(u64),
{
    write_received_file_with_progress_and_cancel(
        receive_dir,
        manifest_path,
        expected_size,
        expected_sha256,
        reader,
        on_progress,
        || false,
    )
}

pub fn write_received_file_with_progress_and_cancel<R, F, C>(
    receive_dir: &Path,
    manifest_path: &str,
    expected_size: u64,
    expected_sha256: &str,
    reader: &mut R,
    on_progress: F,
    should_cancel: C,
) -> NekoDropResult<ReceivedFile>
where
    R: Read + ?Sized,
    F: FnMut(u64),
    C: FnMut() -> bool,
{
    write_received_file_with_resume_and_cancel(
        receive_dir,
        manifest_path,
        expected_size,
        expected_sha256,
        0,
        reader,
        on_progress,
        should_cancel,
    )
}

pub fn write_received_file_with_resume_and_cancel<R, F, C>(
    receive_dir: &Path,
    manifest_path: &str,
    expected_size: u64,
    expected_sha256: &str,
    initial_bytes: u64,
    reader: &mut R,
    mut on_progress: F,
    mut should_cancel: C,
) -> NekoDropResult<ReceivedFile>
where
    R: Read + ?Sized,
    F: FnMut(u64),
    C: FnMut() -> bool,
{
    if expected_sha256.trim().is_empty() {
        return Err(NekoDropError::Storage(
            "expected SHA-256 checksum cannot be empty".into(),
        ));
    }
    if initial_bytes > expected_size {
        return Err(NekoDropError::Storage(format!(
            "resume offset exceeds expected size for {manifest_path}: {initial_bytes} > {expected_size}"
        )));
    }

    let destination = safe_join_receive_path(receive_dir, manifest_path)?;
    let partial_path = partial_path_for(&destination)?;

    if initial_bytes == expected_size {
        return finalize_existing_resume_file(
            destination,
            partial_path,
            manifest_path,
            expected_size,
            expected_sha256,
        );
    }

    if destination.exists() {
        return Err(NekoDropError::Storage(format!(
            "destination already exists: {}",
            destination.display()
        )));
    }

    let parent = destination.parent().ok_or_else(|| {
        NekoDropError::Storage(format!(
            "destination has no parent directory: {}",
            destination.display()
        ))
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        NekoDropError::Storage(format!("failed to create {}: {error}", parent.display()))
    })?;

    if initial_bytes == 0 && partial_path.exists() {
        return Err(NekoDropError::Storage(format!(
            "partial file already exists: {}",
            partial_path.display()
        )));
    }

    let mut hasher = Sha256::new();
    let mut partial_file = if initial_bytes == 0 {
        File::create(&partial_path).map_err(|error| {
            NekoDropError::Storage(format!(
                "failed to create {}: {error}",
                partial_path.display()
            ))
        })?
    } else {
        hash_existing_partial_prefix(&partial_path, initial_bytes, &mut hasher)?;
        on_progress(initial_bytes);
        OpenOptions::new()
            .append(true)
            .open(&partial_path)
            .map_err(|error| {
                NekoDropError::Storage(format!(
                    "failed to open {} for resume append: {error}",
                    partial_path.display()
                ))
            })?
    };
    let mut remaining = expected_size.saturating_sub(initial_bytes);
    let mut buffer = [0_u8; 64 * 1024];
    let mut bytes_written = initial_bytes;

    while remaining > 0 {
        if should_cancel() {
            let _ = fs::remove_file(&partial_path);
            return Err(NekoDropError::Storage("transfer cancelled".into()));
        }

        let max_read = remaining.min(buffer.len() as u64) as usize;
        let read = reader.read(&mut buffer[..max_read]).map_err(|error| {
            NekoDropError::Storage(format!("failed to read incoming file payload: {error}"))
        })?;
        if read == 0 {
            return Err(NekoDropError::Storage(format!(
                "incoming file ended early after {bytes_written} of {expected_size} bytes"
            )));
        }

        partial_file.write_all(&buffer[..read]).map_err(|error| {
            NekoDropError::Storage(format!(
                "failed to write {}: {error}",
                partial_path.display()
            ))
        })?;
        hasher.update(&buffer[..read]);
        bytes_written += read as u64;
        remaining -= read as u64;
        on_progress(bytes_written);

        if should_cancel() {
            let _ = fs::remove_file(&partial_path);
            return Err(NekoDropError::Storage("transfer cancelled".into()));
        }
    }

    partial_file.flush().map_err(|error| {
        NekoDropError::Storage(format!(
            "failed to flush {}: {error}",
            partial_path.display()
        ))
    })?;

    let actual_sha256 = hex::encode(hasher.finalize());
    if !actual_sha256.eq_ignore_ascii_case(expected_sha256) {
        return Err(NekoDropError::Storage(format!(
            "checksum mismatch for {manifest_path}: expected {expected_sha256}, got {actual_sha256}"
        )));
    }

    fs::rename(&partial_path, &destination).map_err(|error| {
        NekoDropError::Storage(format!(
            "failed to finalize {}: {error}",
            destination.display()
        ))
    })?;

    Ok(ReceivedFile {
        path: destination,
        manifest_path: manifest_path.to_string(),
        bytes_written,
        sha256: actual_sha256,
        verified: true,
    })
}

fn finalize_existing_resume_file(
    destination: PathBuf,
    partial_path: PathBuf,
    manifest_path: &str,
    expected_size: u64,
    expected_sha256: &str,
) -> NekoDropResult<ReceivedFile> {
    if destination.exists() {
        verify_existing_complete_file(&destination, manifest_path, expected_size, expected_sha256)?;
        return Ok(ReceivedFile {
            path: destination,
            manifest_path: manifest_path.to_string(),
            bytes_written: expected_size,
            sha256: expected_sha256.to_string(),
            verified: true,
        });
    }

    if !partial_path.exists() {
        return Err(NekoDropError::Storage(format!(
            "resume offset expects existing file for {manifest_path}"
        )));
    }

    verify_existing_complete_file(&partial_path, manifest_path, expected_size, expected_sha256)?;
    fs::rename(&partial_path, &destination).map_err(|error| {
        NekoDropError::Storage(format!(
            "failed to finalize {}: {error}",
            destination.display()
        ))
    })?;

    Ok(ReceivedFile {
        path: destination,
        manifest_path: manifest_path.to_string(),
        bytes_written: expected_size,
        sha256: expected_sha256.to_string(),
        verified: true,
    })
}

fn verify_existing_complete_file(
    path: &Path,
    manifest_path: &str,
    expected_size: u64,
    expected_sha256: &str,
) -> NekoDropResult<()> {
    let metadata = fs::metadata(path).map_err(|error| {
        NekoDropError::Storage(format!(
            "failed to read metadata for {}: {error}",
            path.display()
        ))
    })?;
    if !metadata.is_file() {
        return Err(NekoDropError::Storage(format!(
            "resume path is not a file: {}",
            path.display()
        )));
    }
    if metadata.len() != expected_size {
        return Err(NekoDropError::Storage(format!(
            "resume complete file size mismatch for {manifest_path}: {} != {}",
            metadata.len(),
            expected_size
        )));
    }
    let actual_sha256 = crate::checksum::sha256_file(path)?.value;
    if !actual_sha256.eq_ignore_ascii_case(expected_sha256) {
        return Err(NekoDropError::Storage(format!(
            "checksum mismatch for {manifest_path}: expected {expected_sha256}, got {actual_sha256}"
        )));
    }
    Ok(())
}

fn hash_existing_partial_prefix(
    partial_path: &Path,
    initial_bytes: u64,
    hasher: &mut Sha256,
) -> NekoDropResult<()> {
    let metadata = fs::metadata(partial_path).map_err(|error| {
        NekoDropError::Storage(format!(
            "failed to read metadata for {}: {error}",
            partial_path.display()
        ))
    })?;
    if !metadata.is_file() {
        return Err(NekoDropError::Storage(format!(
            "resume partial is not a file: {}",
            partial_path.display()
        )));
    }
    if metadata.len() != initial_bytes {
        return Err(NekoDropError::Storage(format!(
            "resume partial size mismatch for {}: {} != {}",
            partial_path.display(),
            metadata.len(),
            initial_bytes
        )));
    }

    let mut file = File::open(partial_path).map_err(|error| {
        NekoDropError::Storage(format!(
            "failed to open {} for resume hashing: {error}",
            partial_path.display()
        ))
    })?;
    let mut remaining = initial_bytes;
    let mut buffer = [0_u8; 64 * 1024];

    while remaining > 0 {
        let max_read = remaining.min(buffer.len() as u64) as usize;
        let read = file.read(&mut buffer[..max_read]).map_err(|error| {
            NekoDropError::Storage(format!(
                "failed to read {} for resume hashing: {error}",
                partial_path.display()
            ))
        })?;
        if read == 0 {
            return Err(NekoDropError::Storage(format!(
                "resume partial ended early for {}",
                partial_path.display()
            )));
        }
        hasher.update(&buffer[..read]);
        remaining -= read as u64;
    }

    Ok(())
}

pub(crate) fn partial_path_for(destination: &Path) -> NekoDropResult<PathBuf> {
    let file_name = destination
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            NekoDropError::Storage(format!(
                "destination has no valid file name: {}",
                destination.display()
            ))
        })?;

    Ok(destination.with_file_name(format!("{file_name}.nekodrop-part")))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Cursor;

    use super::*;

    #[test]
    fn writes_received_file_after_checksum_verification() {
        let dir = unique_temp_dir("received-file");
        let payload = b"hello over network".to_vec();
        let checksum = "b0cda4b2fff9211aaa4c49df724a81dfbad65bb2d13015d22eec9fb9ab327786";
        let mut reader = Cursor::new(payload);

        let received =
            write_received_file(&dir, "folder/sample.txt", 18, checksum, &mut reader).unwrap();

        assert!(received.verified);
        assert_eq!(received.bytes_written, 18);
        assert_eq!(
            fs::read_to_string(dir.join("folder/sample.txt")).unwrap(),
            "hello over network"
        );
        assert!(!dir.join("folder/sample.txt.nekodrop-part").exists());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_checksum_mismatch() {
        let dir = unique_temp_dir("received-mismatch");
        let mut reader = Cursor::new(b"bad".to_vec());

        let result = write_received_file(
            &dir,
            "sample.txt",
            3,
            "0000000000000000000000000000000000000000000000000000000000000000",
            &mut reader,
        );

        assert!(result.is_err());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn cancelled_receive_removes_partial_file() {
        let dir = unique_temp_dir("received-cancel");
        let payload = b"hello over network".to_vec();
        let checksum = "b0cda4b2fff9211aaa4c49df724a81dfbad65bb2d13015d22eec9fb9ab327786";
        let mut reader = Cursor::new(payload);

        let result = write_received_file_with_progress_and_cancel(
            &dir,
            "folder/sample.txt",
            18,
            checksum,
            &mut reader,
            |_| {},
            || true,
        );

        assert!(result.is_err());
        assert!(!dir.join("folder/sample.txt").exists());
        assert!(!dir.join("folder/sample.txt.nekodrop-part").exists());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn resumes_receive_from_existing_partial_file() {
        let dir = unique_temp_dir("received-resume");
        fs::create_dir_all(dir.join("folder")).unwrap();
        fs::write(dir.join("folder/sample.txt.nekodrop-part"), b"hello ").unwrap();
        let checksum = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        let mut reader = Cursor::new(b"world".to_vec());
        let mut progress = Vec::new();

        let received = write_received_file_with_resume_and_cancel(
            &dir,
            "folder/sample.txt",
            11,
            checksum,
            6,
            &mut reader,
            |bytes| progress.push(bytes),
            || false,
        )
        .unwrap();

        assert!(received.verified);
        assert_eq!(received.bytes_written, 11);
        assert_eq!(
            fs::read_to_string(dir.join("folder/sample.txt")).unwrap(),
            "hello world"
        );
        assert!(!dir.join("folder/sample.txt.nekodrop-part").exists());
        assert_eq!(progress, vec![6, 11]);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn finalizes_complete_partial_file_when_resume_offset_is_complete() {
        let dir = unique_temp_dir("received-resume-complete");
        fs::write(dir.join("sample.txt.nekodrop-part"), b"hello").unwrap();
        let checksum = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824";
        let mut reader = Cursor::new(Vec::<u8>::new());

        let received = write_received_file_with_resume_and_cancel(
            &dir,
            "sample.txt",
            5,
            checksum,
            5,
            &mut reader,
            |_| {},
            || false,
        )
        .unwrap();

        assert!(received.verified);
        assert_eq!(fs::read_to_string(dir.join("sample.txt")).unwrap(), "hello");
        assert!(!dir.join("sample.txt.nekodrop-part").exists());

        fs::remove_dir_all(dir).unwrap();
    }

    fn unique_temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "nekodrop-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}

use std::path::PathBuf;

use super::path_dialog::expand_home_dir;

pub(crate) fn string_paths_to_path_bufs(paths: Vec<String>) -> Result<Vec<PathBuf>, String> {
    if paths.is_empty() {
        return Err("请至少输入一个文件或文件夹路径".into());
    }

    paths
        .into_iter()
        .map(|path| normalize_user_path(&path))
        .collect()
}

pub(crate) fn path_bufs_to_strings(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect()
}

pub(crate) fn parse_paths_text(paths_text: &str) -> Result<Vec<PathBuf>, String> {
    let paths = paths_text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.trim_matches('"').trim_matches('\'').to_string())
        .collect::<Vec<_>>();

    string_paths_to_path_bufs(paths)
}

fn normalize_user_path(path: &str) -> Result<PathBuf, String> {
    let path = strip_outer_path_quotes(path);
    validate_user_path_text(path)?;
    let expanded = expand_home_dir(path);
    if !expanded.exists() {
        return Err(format!("路径不存在：{}", expanded.display()));
    }
    Ok(expanded)
}

fn strip_outer_path_quotes(path: &str) -> &str {
    let trimmed_start = path.trim_start();
    let maybe_quoted = trimmed_start.trim_end();
    if maybe_quoted.len() >= 2 {
        let bytes = maybe_quoted.as_bytes();
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &maybe_quoted[1..maybe_quoted.len() - 1];
        }
    }
    trimmed_start
}

fn validate_user_path_text(path: &str) -> Result<(), String> {
    if path.contains('\u{fffd}') {
        return Err(
            "路径编码已经损坏，里面出现了 �。请重新用系统文件选择器选择文件，或从原始位置重新复制路径。"
                .to_string(),
        );
    }

    if let Some(reason) = windows_unsafe_user_path_reason(path) {
        return Err(format!(
            "Windows 不安全路径：{reason}。请重命名文件/文件夹后再发送，或重新选择正确路径。"
        ));
    }

    Ok(())
}

fn windows_unsafe_user_path_reason(path: &str) -> Option<String> {
    for (index, component) in path
        .split(['/', '\\'])
        .filter(|component| !component.is_empty())
        .enumerate()
    {
        if index == 0 && is_windows_drive_prefix(component) {
            continue;
        }
        if component.ends_with(' ') || component.ends_with('.') {
            return Some(format!("路径片段不能以空格或点结尾：{component}"));
        }
        if component
            .chars()
            .any(|ch| matches!(ch, '<' | '>' | '"' | '|' | '?' | '*'))
        {
            return Some(format!("路径片段包含 Windows 非法字符：{component}"));
        }
        if component.contains(':') {
            return Some(format!("路径片段包含 ADS 或非法冒号：{component}"));
        }
        if is_windows_reserved_user_path_component(component) {
            return Some(format!("路径片段使用了 Windows 保留名称：{component}"));
        }
    }
    None
}

fn is_windows_drive_prefix(component: &str) -> bool {
    let bytes = component.as_bytes();
    bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn is_windows_reserved_user_path_component(component: &str) -> bool {
    let stem = component.split('.').next().unwrap_or(component);
    let upper = stem.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_path_rejects_replacement_character_before_exists_check() {
        let error = normalize_user_path(r"I:\�ļ�\asmr\z\����\16����.m4a").unwrap_err();

        assert!(error.contains("路径编码已经损坏"));
    }

    #[test]
    fn manual_path_rejects_windows_unsafe_components_before_exists_check() {
        for path in [
            r"C:\drop\CON.txt",
            r"C:\drop\audio.m4a:Zone.Identifier",
            r"C:\drop\trailing.",
            r"C:\drop\trailing ",
        ] {
            let error = normalize_user_path(path).unwrap_err();

            assert!(
                error.contains("Windows 不安全路径"),
                "unexpected error for {path}: {error}"
            );
        }
    }
}

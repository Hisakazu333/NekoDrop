use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;

pub(crate) fn expand_home_dir(path: &str) -> PathBuf {
    if path == "~" {
        return home_dir().unwrap_or_else(|| PathBuf::from(path));
    }

    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = home_dir() {
            return home.join(rest);
        }
    }

    PathBuf::from(path)
}

pub(crate) fn default_receive_dir() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Downloads")
        .join("NekoDrop")
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

pub(crate) fn bind_available_listener(
    bind_host: &str,
    requested_port: u16,
) -> Result<TcpListener, String> {
    let mut last_error = None;

    for offset in 0..20 {
        let Some(port) = requested_port.checked_add(offset) else {
            break;
        };
        match TcpListener::bind((bind_host, port)) {
            Ok(listener) => return Ok(listener),
            Err(error) => last_error = Some(format!("{bind_host}:{port}: {error}")),
        }
    }

    Err(format!(
        "无法监听端口，从 {requested_port} 起连续尝试失败：{}",
        last_error.unwrap_or_else(|| "没有可用端口".to_string())
    ))
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum PathDialogKind {
    Files,
    Folders,
    SingleFolder,
    BundleSourceFolder,
}

pub(crate) fn parse_dialog_output(output: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(output)
        .lines()
        .map(|line| line.trim_start_matches('\u{feff}').trim())
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(any(target_os = "windows", test))]
pub(crate) fn windows_dialog_script(kind: PathDialogKind) -> String {
    let picker_script = match kind {
        PathDialogKind::Files => {
            r#"
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.OpenFileDialog
$dialog.Multiselect = $true
$dialog.Title = '选择要发送的文件'
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
  $dialog.FileNames -join "`n"
}
"#
        }
        PathDialogKind::Folders => {
            r#"
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.FolderBrowserDialog
$dialog.Description = '选择要发送的文件夹'
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
  $dialog.SelectedPath
}
"#
        }
        PathDialogKind::SingleFolder => {
            r#"
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.FolderBrowserDialog
$dialog.Description = '选择接收目录'
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
  $dialog.SelectedPath
}
"#
        }
        PathDialogKind::BundleSourceFolder => {
            r#"
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.FolderBrowserDialog
$dialog.Description = '选择资料包来源目录'
if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
  $dialog.SelectedPath
}
"#
        }
    };

    format!(
        r#"
$utf8NoBom = New-Object System.Text.UTF8Encoding -ArgumentList $false
[Console]::OutputEncoding = $utf8NoBom
$OutputEncoding = $utf8NoBom
{picker_script}
"#
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn choose_paths(kind: PathDialogKind) -> Result<Vec<String>, String> {
    let script = match kind {
        PathDialogKind::Files => {
            r#"
set pickedItems to choose file with prompt "选择要发送的文件" with multiple selections allowed
set outputText to ""
repeat with pickedItem in pickedItems
  set outputText to outputText & POSIX path of pickedItem & linefeed
end repeat
return outputText
"#
        }
        PathDialogKind::Folders => {
            r#"
set pickedItems to choose folder with prompt "选择要发送的文件夹" with multiple selections allowed
set outputText to ""
repeat with pickedItem in pickedItems
  set outputText to outputText & POSIX path of pickedItem & linefeed
end repeat
return outputText
"#
        }
        PathDialogKind::SingleFolder => {
            r#"
set pickedItem to choose folder with prompt "选择接收目录"
return POSIX path of pickedItem
"#
        }
        PathDialogKind::BundleSourceFolder => {
            r#"
set pickedItem to choose folder with prompt "选择资料包来源目录"
return POSIX path of pickedItem
"#
        }
    };

    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|error| format!("无法打开系统选择窗口：{error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("User canceled") || stderr.contains("-128") {
            return Ok(Vec::new());
        }
        return Err(format!("系统选择窗口失败：{}", stderr.trim()));
    }

    Ok(parse_dialog_output(&output.stdout))
}

#[cfg(target_os = "windows")]
pub(crate) fn choose_paths(kind: PathDialogKind) -> Result<Vec<String>, String> {
    let script = windows_dialog_script(kind);

    let output = Command::new("powershell")
        .args(["-NoProfile", "-STA", "-Command", &script])
        .output()
        .map_err(|error| format!("无法打开系统选择窗口：{error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("系统选择窗口失败：{}", stderr.trim()));
    }

    Ok(parse_dialog_output(&output.stdout))
}

#[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
pub(crate) fn choose_paths(kind: PathDialogKind) -> Result<Vec<String>, String> {
    let mut args = vec!["--file-selection".to_string()];
    match kind {
        PathDialogKind::Files => {
            args.push("--multiple".to_string());
            args.push("--separator=\n".to_string());
            args.push("--title=选择要发送的文件".to_string());
        }
        PathDialogKind::Folders => {
            args.push("--directory".to_string());
            args.push("--multiple".to_string());
            args.push("--separator=\n".to_string());
            args.push("--title=选择要发送的文件夹".to_string());
        }
        PathDialogKind::SingleFolder => {
            args.push("--directory".to_string());
            args.push("--title=选择接收目录".to_string());
        }
        PathDialogKind::BundleSourceFolder => {
            args.push("--directory".to_string());
            args.push("--title=选择资料包来源目录".to_string());
        }
    }

    let output = Command::new("zenity")
        .args(args)
        .output()
        .map_err(|error| format!("无法打开系统选择窗口：{error}"))?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    Ok(parse_dialog_output(&output.stdout))
}

pub(crate) fn open_path_with_system(path: PathBuf) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(&path);
        command
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer");
        command.arg(&path);
        command
    };

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(&path);
        command
    };

    command
        .spawn()
        .map_err(|error| format!("无法打开 {}：{error}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_dialog_output_strips_utf8_bom_from_windows_stdout() {
        let output = b"\xEF\xBB\xBFI:\\\xe6\x96\x87\xe4\xbb\xb6\\asmr\\z\\16\xe5\x88\x86\xe9\x92\x9f.m4a\r\n";

        let paths = parse_dialog_output(output);

        assert_eq!(paths, vec!["I:\\文件\\asmr\\z\\16分钟.m4a"]);
    }

    #[test]
    fn windows_dialog_script_forces_utf8_stdout_for_chinese_paths() {
        let script = windows_dialog_script(PathDialogKind::Files);

        assert!(script.contains("[Console]::OutputEncoding"));
        assert!(script.contains("UTF8Encoding"));
        assert!(script.contains("$OutputEncoding"));
    }

    #[test]
    fn windows_dialog_script_uses_bundle_source_prompt() {
        let script = windows_dialog_script(PathDialogKind::BundleSourceFolder);

        assert!(script.contains("选择资料包来源目录"));
        assert!(!script.contains("选择接收目录"));
    }
}

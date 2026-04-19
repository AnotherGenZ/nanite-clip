use super::*;

pub(super) fn pick_directory_impl(current_dir: String) -> Result<Option<String>, String> {
    let dialog_attempts = [
        ("zenity", build_zenity_args(current_dir.as_str())),
        ("qarma", build_zenity_args(current_dir.as_str())),
        ("yad", build_zenity_args(current_dir.as_str())),
        ("kdialog", build_kdialog_args(current_dir.as_str())),
    ];

    for (program, args) in dialog_attempts {
        let output = match command_runner::output(std::process::Command::new(program).args(&args)) {
            Ok(output) => output,
            Err(command_runner::CommandError::Spawn { source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                continue;
            }
            Err(error) => return Err(format!("{program} failed to start: {error}")),
        };

        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            return if path.is_empty() {
                Ok(None)
            } else {
                Ok(Some(path))
            };
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if is_dialog_cancelled(program, output.status.code()) {
            return Ok(None);
        }

        let detail = if stderr.is_empty() {
            format!("exit status {}", output.status)
        } else {
            stderr
        };
        return Err(format!("{program} failed: {detail}"));
    }

    Err(
        "No supported directory picker found. Install `zenity`, `qarma`, `yad`, or `kdialog`."
            .into(),
    )
}

#[cfg(target_os = "windows")]
pub(super) fn pick_directory_impl(current_dir: String) -> Result<Option<String>, String> {
    let initial_directory = sanitize_windows_dialog_start_dir(current_dir.as_str());
    std::thread::spawn(move || pick_directory_with_windows_shell_dialog(initial_directory))
        .join()
        .map_err(|_| "Windows folder picker thread panicked".to_string())?
}

#[cfg(not(target_os = "windows"))]
pub(super) fn pick_toml_file_impl(
    current_path: String,
    title: String,
) -> Result<Option<String>, String> {
    let result = run_path_dialog(
        [
            (
                "zenity",
                build_zenity_open_toml_file_args(current_path.as_str(), title.as_str()),
            ),
            (
                "qarma",
                build_zenity_open_toml_file_args(current_path.as_str(), title.as_str()),
            ),
            (
                "yad",
                build_zenity_open_toml_file_args(current_path.as_str(), title.as_str()),
            ),
            (
                "kdialog",
                build_kdialog_open_toml_file_args(current_path.as_str(), title.as_str()),
            ),
        ],
        "No supported file picker found. Install `zenity`, `qarma`, `yad`, or `kdialog`.",
    )?;

    validate_toml_file_selection(result)
}

#[cfg(target_os = "windows")]
pub(super) fn pick_toml_file_impl(
    current_path: String,
    title: String,
) -> Result<Option<String>, String> {
    let (initial_directory, _) = sanitize_windows_file_dialog_target(current_path.as_str());
    let result = std::thread::spawn(move || {
        pick_toml_file_with_windows_shell_dialog(initial_directory, title)
    })
    .join()
    .map_err(|_| "Windows file picker thread panicked".to_string())??;

    validate_toml_file_selection(result)
}

#[cfg(not(target_os = "windows"))]
pub(super) fn save_file_impl(
    initial_path: String,
    title: String,
) -> Result<Option<String>, String> {
    run_path_dialog(
        [
            (
                "zenity",
                build_zenity_save_file_args(initial_path.as_str(), title.as_str()),
            ),
            (
                "qarma",
                build_zenity_save_file_args(initial_path.as_str(), title.as_str()),
            ),
            (
                "yad",
                build_zenity_save_file_args(initial_path.as_str(), title.as_str()),
            ),
            (
                "kdialog",
                build_kdialog_save_file_args(initial_path.as_str(), title.as_str()),
            ),
        ],
        "No supported save-file picker found. Install `zenity`, `qarma`, `yad`, or `kdialog`.",
    )
}

#[cfg(target_os = "windows")]
pub(super) fn save_file_impl(
    initial_path: String,
    title: String,
) -> Result<Option<String>, String> {
    let (initial_directory, suggested_name) =
        sanitize_windows_file_dialog_target(initial_path.as_str());
    std::thread::spawn(move || {
        save_file_with_windows_shell_dialog(initial_directory, suggested_name, title)
    })
    .join()
    .map_err(|_| "Windows save-file picker thread panicked".to_string())?
}

#[cfg(not(target_os = "windows"))]
fn build_zenity_args(current_dir: &str) -> Vec<String> {
    let mut args = vec!["--file-selection".into(), "--directory".into()];
    if let Some(initial) = sanitize_dialog_start_dir(current_dir) {
        args.push("--filename".into());
        args.push(initial);
    }
    args
}

#[cfg(not(target_os = "windows"))]
fn build_kdialog_args(current_dir: &str) -> Vec<String> {
    let mut args = vec!["--getexistingdirectory".into()];
    if let Some(initial) = sanitize_dialog_start_dir(current_dir) {
        args.push(initial);
    }
    args
}

#[cfg(not(target_os = "windows"))]
fn build_zenity_open_file_args(current_path: &str, title: &str) -> Vec<String> {
    let mut args = vec!["--file-selection".into(), "--title".into(), title.into()];
    if let Some(initial) = sanitize_dialog_file_path(current_path, false) {
        args.push("--filename".into());
        args.push(initial);
    }
    args
}

#[cfg(not(target_os = "windows"))]
fn build_zenity_open_toml_file_args(current_path: &str, title: &str) -> Vec<String> {
    let mut args = build_zenity_open_file_args(current_path, title);
    args.push("--file-filter".into());
    args.push("TOML files | *.toml".into());
    args
}

#[cfg(not(target_os = "windows"))]
fn build_zenity_save_file_args(initial_path: &str, title: &str) -> Vec<String> {
    let mut args = vec![
        "--file-selection".into(),
        "--save".into(),
        "--confirm-overwrite".into(),
        "--title".into(),
        title.into(),
    ];
    if let Some(initial) = sanitize_dialog_file_path(initial_path, true) {
        args.push("--filename".into());
        args.push(initial);
    }
    args
}

#[cfg(not(target_os = "windows"))]
fn build_kdialog_open_file_args(current_path: &str, title: &str) -> Vec<String> {
    let mut args = vec!["--title".into(), title.into(), "--getopenfilename".into()];
    if let Some(initial) = sanitize_dialog_file_path(current_path, false) {
        args.push(initial);
    }
    args
}

#[cfg(not(target_os = "windows"))]
fn build_kdialog_open_toml_file_args(current_path: &str, title: &str) -> Vec<String> {
    let mut args = build_kdialog_open_file_args(current_path, title);
    args.push("TOML files (*.toml)".into());
    args
}

#[cfg(not(target_os = "windows"))]
fn build_kdialog_save_file_args(initial_path: &str, title: &str) -> Vec<String> {
    let mut args = vec!["--title".into(), title.into(), "--getsavefilename".into()];
    if let Some(initial) = sanitize_dialog_file_path(initial_path, true) {
        args.push(initial);
    }
    args
}

#[cfg(not(target_os = "windows"))]
fn run_path_dialog<const N: usize>(
    dialog_attempts: [(&str, Vec<String>); N],
    missing_dialog_error: &str,
) -> Result<Option<String>, String> {
    for (program, args) in dialog_attempts {
        let output = match command_runner::output(std::process::Command::new(program).args(&args)) {
            Ok(output) => output,
            Err(command_runner::CommandError::Spawn { source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                continue;
            }
            Err(error) => return Err(format!("{program} failed to start: {error}")),
        };

        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            return if path.is_empty() {
                Ok(None)
            } else {
                Ok(Some(path))
            };
        }

        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if is_dialog_cancelled(program, output.status.code()) {
            return Ok(None);
        }

        let detail = if stderr.is_empty() {
            format!("exit status {}", output.status)
        } else {
            stderr
        };
        return Err(format!("{program} failed: {detail}"));
    }

    Err(missing_dialog_error.into())
}

fn validate_toml_file_selection(selection: Option<String>) -> Result<Option<String>, String> {
    let Some(path) = selection else {
        return Ok(None);
    };

    if std::path::Path::new(&path)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))
    {
        Ok(Some(path))
    } else {
        Err("Select a `.toml` file to import profiles.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ApplyDiscoveredAudioSource, apply_discovered_audio_source, audio_source_draft_is_blank,
        manual_clip_settings_dirty_values, validate_toml_file_selection,
    };
    use crate::app::AudioSourceDraft;
    use crate::capture::{DiscoveredAudioKind, DiscoveredAudioSource};
    use crate::config::{AudioSourceKind, ManualClipConfig};

    #[test]
    fn discovered_audio_source_replaces_blank_placeholder_row() {
        let mut drafts = vec![AudioSourceDraft::default()];
        let discovered = DiscoveredAudioSource {
            kind_hint: AudioSourceKind::DefaultOutput,
            display_label: "Default output".into(),
            kind: DiscoveredAudioKind::Device,
            available: true,
        };

        let outcome = apply_discovered_audio_source(&mut drafts, &discovered);

        assert_eq!(outcome, ApplyDiscoveredAudioSource::Added);
        assert_eq!(
            drafts,
            vec![AudioSourceDraft {
                label: "Default output".into(),
                source: "default_output".into(),
                ..AudioSourceDraft::default()
            }]
        );
    }

    #[test]
    fn discovered_audio_source_does_not_duplicate_existing_source() {
        let mut drafts = vec![AudioSourceDraft {
            label: "Game audio".into(),
            source: "default_output".into(),
            ..AudioSourceDraft::default()
        }];
        let discovered = DiscoveredAudioSource {
            kind_hint: AudioSourceKind::DefaultOutput,
            display_label: "Default output".into(),
            kind: DiscoveredAudioKind::Device,
            available: true,
        };

        let outcome = apply_discovered_audio_source(&mut drafts, &discovered);

        assert_eq!(outcome, ApplyDiscoveredAudioSource::Unchanged);
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].label, "Game audio");
    }

    #[test]
    fn blank_audio_source_helper_requires_empty_label_and_source() {
        assert!(audio_source_draft_is_blank(&AudioSourceDraft::default()));
        assert!(!audio_source_draft_is_blank(&AudioSourceDraft {
            label: "Game audio".into(),
            source: String::new(),
            ..AudioSourceDraft::default()
        }));
    }

    #[test]
    fn manual_clip_settings_dirty_helper_only_flags_real_changes() {
        let config = ManualClipConfig {
            enabled: true,
            hotkey: "Ctrl+Shift+F8".into(),
            duration_secs: 30,
        };

        assert!(!manual_clip_settings_dirty_values(
            true,
            "Ctrl+Shift+F8",
            "30",
            &config,
        ));
        assert!(manual_clip_settings_dirty_values(
            false,
            "Ctrl+Shift+F8",
            "30",
            &config,
        ));
        assert!(manual_clip_settings_dirty_values(
            true, "Alt+F8", "30", &config,
        ));
        assert!(manual_clip_settings_dirty_values(
            true,
            "Ctrl+Shift+F8",
            "45",
            &config,
        ));
    }

    #[test]
    fn toml_picker_validation_accepts_toml_extensions_case_insensitively() {
        assert_eq!(
            validate_toml_file_selection(Some("/tmp/profile-export.TOML".into())).unwrap(),
            Some("/tmp/profile-export.TOML".into())
        );
    }

    #[test]
    fn toml_picker_validation_rejects_non_toml_extensions() {
        let error =
            validate_toml_file_selection(Some("/tmp/profile-export.json".into())).unwrap_err();

        assert!(error.contains("`.toml`"));
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn zenity_toml_picker_args_include_file_filter() {
        let args = super::build_zenity_open_toml_file_args("/tmp", "Import");

        assert!(args.iter().any(|arg| arg == "--file-filter"));
        assert!(args.iter().any(|arg| arg == "TOML files | *.toml"));
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn kdialog_toml_picker_args_include_file_filter() {
        let args = super::build_kdialog_open_toml_file_args("/tmp", "Import");

        assert!(args.iter().any(|arg| arg == "TOML files (*.toml)"));
    }
}

#[cfg(not(target_os = "windows"))]
fn sanitize_dialog_start_dir(current_dir: &str) -> Option<String> {
    let trimmed = current_dir.trim();
    if trimmed.is_empty() {
        return None;
    }

    let path = std::path::Path::new(trimmed);
    if path.exists() {
        Some(with_trailing_slash(trimmed))
    } else {
        path.parent()
            .map(|parent| with_trailing_slash(parent.to_string_lossy().as_ref()))
    }
}

#[cfg(not(target_os = "windows"))]
fn sanitize_dialog_file_path(path_hint: &str, allow_nonexistent_file_name: bool) -> Option<String> {
    let trimmed = path_hint.trim();
    if trimmed.is_empty() {
        return None;
    }

    let path = std::path::Path::new(trimmed);
    if path.exists() {
        return if path.is_dir() {
            Some(with_trailing_slash(trimmed))
        } else {
            Some(trimmed.to_string())
        };
    }

    if allow_nonexistent_file_name && path.parent().is_some_and(std::path::Path::exists) {
        return Some(path.to_string_lossy().into_owned());
    }

    path.parent()
        .filter(|parent| parent.exists())
        .map(|parent| with_trailing_slash(parent.to_string_lossy().as_ref()))
}

#[cfg(target_os = "windows")]
fn sanitize_windows_dialog_start_dir(current_dir: &str) -> Option<String> {
    let trimmed = current_dir.trim();
    if trimmed.is_empty() {
        return None;
    }

    let path = std::path::Path::new(trimmed);
    if path.exists() {
        Some(trimmed.to_string())
    } else {
        path.parent()
            .map(|parent| parent.to_string_lossy().into_owned())
    }
}

#[cfg(target_os = "windows")]
fn sanitize_windows_file_dialog_target(path_hint: &str) -> (Option<String>, Option<String>) {
    let trimmed = path_hint.trim();
    if trimmed.is_empty() {
        return (None, None);
    }

    let path = std::path::Path::new(trimmed);
    if path.exists() {
        if path.is_dir() {
            return (Some(trimmed.to_string()), None);
        }

        return (
            path.parent()
                .map(|parent| parent.to_string_lossy().into_owned()),
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned()),
        );
    }

    (
        path.parent()
            .map(|parent| parent.to_string_lossy().into_owned()),
        path.file_name()
            .map(|name| name.to_string_lossy().into_owned()),
    )
}

#[cfg(not(target_os = "windows"))]
fn with_trailing_slash(path: &str) -> String {
    let mut value = path.to_string();
    if !value.ends_with('/') {
        value.push('/');
    }
    value
}

#[cfg(not(target_os = "windows"))]
fn is_dialog_cancelled(program: &str, code: Option<i32>) -> bool {
    match program {
        "zenity" | "qarma" | "yad" | "kdialog" => code == Some(1),
        _ => false,
    }
}

#[cfg(target_os = "windows")]
fn pick_directory_with_windows_shell_dialog(
    initial_directory: Option<String>,
) -> Result<Option<String>, String> {
    use windows::Win32::Foundation::ERROR_CANCELLED;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance,
        CoInitializeEx, CoTaskMemFree, CoUninitialize,
    };
    use windows::Win32::UI::Shell::{
        FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog,
        IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
    };
    use windows::core::{HRESULT, HSTRING};

    struct ComApartment;

    impl Drop for ComApartment {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }

    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE)
            .ok()
            .map_err(|error| format!("failed to initialize the Windows file picker: {error}"))?;
        let _com_apartment = ComApartment;

        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|error| format!("failed to create the Windows file picker: {error}"))?;

        dialog
            .SetTitle(&HSTRING::from("Select a folder for NaniteClip"))
            .map_err(|error| format!("failed to set the Windows file picker title: {error}"))?;

        let options = dialog
            .GetOptions()
            .map_err(|error| format!("failed to read Windows file picker options: {error}"))?;
        dialog
            .SetOptions(options | FOS_PICKFOLDERS | FOS_PATHMUSTEXIST | FOS_FORCEFILESYSTEM)
            .map_err(|error| format!("failed to configure the Windows file picker: {error}"))?;

        if let Some(initial_directory) = initial_directory.as_ref() {
            let shell_item: IShellItem =
                SHCreateItemFromParsingName(&HSTRING::from(initial_directory), None).map_err(
                    |error| {
                        format!("failed to resolve the initial directory for the picker: {error}")
                    },
                )?;
            dialog
                .SetFolder(&shell_item)
                .map_err(|error| format!("failed to set the initial picker directory: {error}"))?;
            dialog
                .SetDefaultFolder(&shell_item)
                .map_err(|error| format!("failed to set the default picker directory: {error}"))?;
        }

        match dialog.Show(None) {
            Ok(()) => {}
            Err(error) if error.code() == HRESULT::from_win32(ERROR_CANCELLED.0) => {
                return Ok(None);
            }
            Err(error) => {
                return Err(format!("Windows folder picker failed: {error}"));
            }
        }

        let selected_item = dialog
            .GetResult()
            .map_err(|error| format!("failed to read the selected folder: {error}"))?;
        let selected_path = selected_item
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|error| format!("failed to resolve the selected folder path: {error}"))?;
        let path = selected_path
            .to_string()
            .map_err(|error| format!("selected folder path was not valid UTF-16: {error}"))?;
        CoTaskMemFree(Some(selected_path.0.cast()));

        if path.trim().is_empty() {
            Ok(None)
        } else {
            Ok(Some(path))
        }
    }
}

#[cfg(target_os = "windows")]
fn pick_toml_file_with_windows_shell_dialog(
    initial_directory: Option<String>,
    title: String,
) -> Result<Option<String>, String> {
    use windows::Win32::Foundation::ERROR_CANCELLED;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance,
        CoInitializeEx, CoTaskMemFree, CoUninitialize,
    };
    use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
    use windows::Win32::UI::Shell::{
        FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM, FOS_PATHMUSTEXIST, FileOpenDialog, IFileOpenDialog,
        IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
    };
    use windows::core::{HRESULT, HSTRING, PCWSTR};

    struct ComApartment;

    impl Drop for ComApartment {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }

    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE)
            .ok()
            .map_err(|error| format!("failed to initialize the Windows file picker: {error}"))?;
        let _com_apartment = ComApartment;

        let dialog: IFileOpenDialog = CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|error| format!("failed to create the Windows file picker: {error}"))?;

        dialog
            .SetTitle(&HSTRING::from(title))
            .map_err(|error| format!("failed to set the Windows file picker title: {error}"))?;

        let filter_name = HSTRING::from("TOML files");
        let filter_spec = HSTRING::from("*.toml");
        let file_types = [COMDLG_FILTERSPEC {
            pszName: PCWSTR(filter_name.as_ptr()),
            pszSpec: PCWSTR(filter_spec.as_ptr()),
        }];
        dialog
            .SetFileTypes(&file_types)
            .map_err(|error| format!("failed to set the Windows file picker filter: {error}"))?;

        let options = dialog
            .GetOptions()
            .map_err(|error| format!("failed to read Windows file picker options: {error}"))?;
        dialog
            .SetOptions(options | FOS_FILEMUSTEXIST | FOS_PATHMUSTEXIST | FOS_FORCEFILESYSTEM)
            .map_err(|error| format!("failed to configure the Windows file picker: {error}"))?;

        if let Some(initial_directory) = initial_directory.as_ref() {
            let shell_item: IShellItem =
                SHCreateItemFromParsingName(&HSTRING::from(initial_directory), None).map_err(
                    |error| {
                        format!("failed to resolve the initial directory for the picker: {error}")
                    },
                )?;
            dialog
                .SetFolder(&shell_item)
                .map_err(|error| format!("failed to set the initial picker directory: {error}"))?;
            dialog
                .SetDefaultFolder(&shell_item)
                .map_err(|error| format!("failed to set the default picker directory: {error}"))?;
        }

        match dialog.Show(None) {
            Ok(()) => {}
            Err(error) if error.code() == HRESULT::from_win32(ERROR_CANCELLED.0) => {
                return Ok(None);
            }
            Err(error) => {
                return Err(format!("Windows file picker failed: {error}"));
            }
        }

        let selected_item = dialog
            .GetResult()
            .map_err(|error| format!("failed to read the selected file: {error}"))?;
        let selected_path = selected_item
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|error| format!("failed to resolve the selected file path: {error}"))?;
        let path = selected_path
            .to_string()
            .map_err(|error| format!("selected file path was not valid UTF-16: {error}"))?;
        CoTaskMemFree(Some(selected_path.0.cast()));

        if path.trim().is_empty() {
            Ok(None)
        } else {
            Ok(Some(path))
        }
    }
}

#[cfg(target_os = "windows")]
fn save_file_with_windows_shell_dialog(
    initial_directory: Option<String>,
    suggested_name: Option<String>,
    title: String,
) -> Result<Option<String>, String> {
    use windows::Win32::Foundation::ERROR_CANCELLED;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance,
        CoInitializeEx, CoTaskMemFree, CoUninitialize,
    };
    use windows::Win32::UI::Shell::{
        FOS_FORCEFILESYSTEM, FOS_OVERWRITEPROMPT, FOS_PATHMUSTEXIST, FileSaveDialog,
        IFileSaveDialog, IShellItem, SHCreateItemFromParsingName, SIGDN_FILESYSPATH,
    };
    use windows::core::{HRESULT, HSTRING};

    struct ComApartment;

    impl Drop for ComApartment {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }

    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE)
            .ok()
            .map_err(|error| format!("failed to initialize the Windows save picker: {error}"))?;
        let _com_apartment = ComApartment;

        let dialog: IFileSaveDialog = CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER)
            .map_err(|error| format!("failed to create the Windows save picker: {error}"))?;

        dialog
            .SetTitle(&HSTRING::from(title))
            .map_err(|error| format!("failed to set the Windows save picker title: {error}"))?;

        let options = dialog
            .GetOptions()
            .map_err(|error| format!("failed to read Windows save picker options: {error}"))?;
        dialog
            .SetOptions(options | FOS_PATHMUSTEXIST | FOS_FORCEFILESYSTEM | FOS_OVERWRITEPROMPT)
            .map_err(|error| format!("failed to configure the Windows save picker: {error}"))?;

        if let Some(initial_directory) = initial_directory.as_ref() {
            let shell_item: IShellItem =
                SHCreateItemFromParsingName(&HSTRING::from(initial_directory), None).map_err(
                    |error| {
                        format!("failed to resolve the initial directory for the picker: {error}")
                    },
                )?;
            dialog
                .SetFolder(&shell_item)
                .map_err(|error| format!("failed to set the initial picker directory: {error}"))?;
            dialog
                .SetDefaultFolder(&shell_item)
                .map_err(|error| format!("failed to set the default picker directory: {error}"))?;
        }

        if let Some(suggested_name) = suggested_name.as_ref() {
            dialog
                .SetFileName(&HSTRING::from(suggested_name))
                .map_err(|error| format!("failed to set the suggested file name: {error}"))?;
        }

        dialog
            .SetDefaultExtension(&HSTRING::from("toml"))
            .map_err(|error| format!("failed to set the save picker extension: {error}"))?;

        match dialog.Show(None) {
            Ok(()) => {}
            Err(error) if error.code() == HRESULT::from_win32(ERROR_CANCELLED.0) => {
                return Ok(None);
            }
            Err(error) => {
                return Err(format!("Windows save picker failed: {error}"));
            }
        }

        let selected_item = dialog
            .GetResult()
            .map_err(|error| format!("failed to read the selected save path: {error}"))?;
        let selected_path = selected_item
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|error| format!("failed to resolve the selected save path: {error}"))?;
        let path = selected_path
            .to_string()
            .map_err(|error| format!("selected save path was not valid UTF-16: {error}"))?;
        CoTaskMemFree(Some(selected_path.0.cast()));

        if path.trim().is_empty() {
            Ok(None)
        } else {
            Ok(Some(path))
        }
    }
}

pub(super) trait PresetValue: Sized {
    fn from_value(value: &str) -> Self;
    fn config_value(self) -> Option<&'static str>;
}

pub(super) fn apply_preset_string_selection<T>(current_value: &str, preset: T) -> String
where
    T: PresetValue + PartialEq + Copy,
{
    match preset.config_value() {
        Some(value) => value.to_string(),
        None if T::from_value(current_value) == preset => current_value.to_string(),
        None => String::new(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureBackendPreset {
    Gsr,
    Obs,
}

impl CaptureBackendPreset {
    #[cfg(target_os = "linux")]
    pub(super) const ALL: [Self; 2] = [Self::Gsr, Self::Obs];
    #[cfg(not(target_os = "linux"))]
    pub(super) const ALL: [Self; 1] = [Self::Obs];

    pub(super) fn all() -> &'static [Self] {
        &Self::ALL
    }

    pub(super) fn from_config(value: CaptureBackend) -> Self {
        match value {
            CaptureBackend::Gsr => Self::Gsr,
            CaptureBackend::Obs => Self::Obs,
        }
    }

    pub(super) fn into_config_backend(self) -> CaptureBackend {
        match self {
            Self::Gsr => CaptureBackend::Gsr,
            Self::Obs => CaptureBackend::Obs,
        }
    }
}

impl PresetValue for CaptureBackendPreset {
    fn from_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "obs" => Self::Obs,
            _ => Self::Gsr,
        }
    }

    fn config_value(self) -> Option<&'static str> {
        Some(self.into_config_backend().as_str())
    }
}

impl std::fmt::Display for CaptureBackendPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Gsr => "gpu-screen-recorder",
            Self::Obs => "OBS Studio",
        })
    }
}

impl std::fmt::Display for ObsManagementMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(obs_management_mode_label(*self))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureSourcePreset {
    Automatic,
    Portal,
    Custom,
}

impl CaptureSourcePreset {
    pub(super) const ALL: [Self; 3] = [Self::Automatic, Self::Portal, Self::Custom];
}

impl PresetValue for CaptureSourcePreset {
    fn from_value(value: &str) -> Self {
        let value = value.trim();
        if value.is_empty()
            || value.eq_ignore_ascii_case("planetside2")
            || value.eq_ignore_ascii_case("ps2")
            || value.eq_ignore_ascii_case("auto")
            || value.eq_ignore_ascii_case("screen")
        {
            Self::Automatic
        } else if value.eq_ignore_ascii_case("portal") {
            Self::Portal
        } else {
            Self::Custom
        }
    }

    fn config_value(self) -> Option<&'static str> {
        match self {
            Self::Automatic => Some("planetside2"),
            Self::Portal => Some("portal"),
            Self::Custom => None,
        }
    }
}

impl std::fmt::Display for CaptureSourcePreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Automatic => "Automatic (PlanetSide 2)",
            Self::Portal => "Portal/Desktop Picker",
            Self::Custom => "Custom",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecPreset {
    Auto,
    H264,
    Hevc,
    Av1,
    Vp8,
    Vp9,
    HevcHdr,
    Hevc10Bit,
    Av1Hdr,
    Av110Bit,
    Custom,
}

impl CodecPreset {
    pub(super) const ALL: [Self; 11] = [
        Self::Auto,
        Self::H264,
        Self::Hevc,
        Self::Av1,
        Self::Vp8,
        Self::Vp9,
        Self::HevcHdr,
        Self::Hevc10Bit,
        Self::Av1Hdr,
        Self::Av110Bit,
        Self::Custom,
    ];
}

impl PresetValue for CodecPreset {
    fn from_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Self::Auto,
            "h264" => Self::H264,
            "h265" | "hevc" => Self::Hevc,
            "av1" => Self::Av1,
            "vp8" => Self::Vp8,
            "vp9" => Self::Vp9,
            "hevc_hdr" => Self::HevcHdr,
            "hevc_10bit" => Self::Hevc10Bit,
            "av1_hdr" => Self::Av1Hdr,
            "av1_10bit" => Self::Av110Bit,
            _ => Self::Custom,
        }
    }

    fn config_value(self) -> Option<&'static str> {
        match self {
            Self::Auto => Some("auto"),
            Self::H264 => Some("h264"),
            Self::Hevc => Some("hevc"),
            Self::Av1 => Some("av1"),
            Self::Vp8 => Some("vp8"),
            Self::Vp9 => Some("vp9"),
            Self::HevcHdr => Some("hevc_hdr"),
            Self::Hevc10Bit => Some("hevc_10bit"),
            Self::Av1Hdr => Some("av1_hdr"),
            Self::Av110Bit => Some("av1_10bit"),
            Self::Custom => None,
        }
    }
}

impl std::fmt::Display for CodecPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Auto => "Auto",
            Self::H264 => "H.264",
            Self::Hevc => "HEVC / H.265",
            Self::Av1 => "AV1",
            Self::Vp8 => "VP8",
            Self::Vp9 => "VP9",
            Self::HevcHdr => "HEVC HDR",
            Self::Hevc10Bit => "HEVC 10-bit",
            Self::Av1Hdr => "AV1 HDR",
            Self::Av110Bit => "AV1 10-bit",
            Self::Custom => "Custom",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerPreset {
    Mkv,
    Mp4,
    Mov,
    Custom,
}

impl ContainerPreset {
    pub(super) const ALL: [Self; 4] = [Self::Mkv, Self::Mp4, Self::Mov, Self::Custom];
}

impl PresetValue for ContainerPreset {
    fn from_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "mkv" => Self::Mkv,
            "mp4" => Self::Mp4,
            "mov" => Self::Mov,
            _ => Self::Custom,
        }
    }

    fn config_value(self) -> Option<&'static str> {
        match self {
            Self::Mkv => Some("mkv"),
            Self::Mp4 => Some("mp4"),
            Self::Mov => Some("mov"),
            Self::Custom => None,
        }
    }
}

impl std::fmt::Display for ContainerPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Mkv => "MKV",
            Self::Mp4 => "MP4",
            Self::Mov => "MOV",
            Self::Custom => "Custom",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ObsContainerPreset {
    Mkv,
    Mp4,
    Mov,
    Flv,
    Ts,
}

impl ObsContainerPreset {
    pub(super) const ALL: [Self; 5] = [Self::Mkv, Self::Mp4, Self::Mov, Self::Flv, Self::Ts];

    pub(super) fn from_value(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "mp4" => Self::Mp4,
            "mov" => Self::Mov,
            "flv" => Self::Flv,
            "ts" => Self::Ts,
            _ => Self::Mkv,
        }
    }

    pub(super) fn config_value(self) -> &'static str {
        match self {
            Self::Mkv => "mkv",
            Self::Mp4 => "mp4",
            Self::Mov => "mov",
            Self::Flv => "flv",
            Self::Ts => "ts",
        }
    }
}

impl std::fmt::Display for ObsContainerPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Mkv => "MKV",
            Self::Mp4 => "MP4",
            Self::Mov => "MOV",
            Self::Flv => "FLV",
            Self::Ts => "TS",
        })
    }
}

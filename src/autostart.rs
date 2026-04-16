use std::path::{Path, PathBuf};

use crate::config::{LaunchAtLoginConfig, LaunchAtLoginProvider};

const AUTOSTART_DESKTOP_FILE_NAME: &str = "nanite-clip.desktop";
#[cfg(target_os = "windows")]
const AUTOSTART_SHORTCUT_FILE_NAME: &str = "nanite-clip.lnk";
const APPLICATION_NAME: &str = "nanite-clip";
const APPLICATION_COMMENT: &str =
    "PlanetSide 2 desktop companion for automatic and manual clip capture";
const APPLICATION_ICON_NAME: &str = "nanite-clip";
#[cfg(target_os = "windows")]
const WINDOWS_RUN_KEY_PATH: &str = r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(target_os = "windows")]
const WINDOWS_RUN_VALUE_NAME: &str = "nanite-clip";

#[derive(Debug, Clone, PartialEq, Eq)]
struct AutostartCommand {
    executable: PathBuf,
    args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AutostartEntry {
    desktop_file_name: &'static str,
    application_name: &'static str,
    comment: &'static str,
    icon_name: &'static str,
    command: AutostartCommand,
}

pub fn sync_launch_at_login(config: &LaunchAtLoginConfig) -> Result<(), String> {
    let entry = AutostartEntry::for_current_binary()?;

    match selected_provider(config) {
        SelectedLaunchAtLoginProvider::Disabled => {
            uninstall_all_known_entries(&entry)?;
            Ok(())
        }
        SelectedLaunchAtLoginProvider::XdgAutostart => {
            let autostart_dir = autostart_dir()?;
            entry.install(&autostart_dir).map(|_| ())
        }
        #[cfg(target_os = "windows")]
        SelectedLaunchAtLoginProvider::WindowsStartupFolder => {
            let startup_dir = windows_startup_dir()?;
            entry.install_windows_shortcut(&startup_dir)
        }
        #[cfg(target_os = "windows")]
        SelectedLaunchAtLoginProvider::WindowsRegistryRun => entry.install_registry_run(),
        #[cfg(not(target_os = "windows"))]
        SelectedLaunchAtLoginProvider::WindowsStartupFolder
        | SelectedLaunchAtLoginProvider::WindowsRegistryRun => {
            Err("Windows launch-at-login integration is not available on this platform".into())
        }
        SelectedLaunchAtLoginProvider::Unsupported(provider) => Err(format!(
            "launch-at-login provider `{}` is not supported on this platform yet",
            provider_label(provider)
        )),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectedLaunchAtLoginProvider {
    Disabled,
    XdgAutostart,
    WindowsStartupFolder,
    WindowsRegistryRun,
    Unsupported(LaunchAtLoginProvider),
}

fn autostart_dir() -> Result<PathBuf, String> {
    directories::BaseDirs::new()
        .map(|dirs| dirs.config_dir().join("autostart"))
        .ok_or_else(|| "failed to resolve the user configuration directory".to_string())
}

fn selected_provider(config: &LaunchAtLoginConfig) -> SelectedLaunchAtLoginProvider {
    if !config.enabled {
        return SelectedLaunchAtLoginProvider::Disabled;
    }

    match config.provider {
        LaunchAtLoginProvider::Auto => {
            if cfg!(target_os = "linux") {
                SelectedLaunchAtLoginProvider::XdgAutostart
            } else if cfg!(target_os = "windows") {
                SelectedLaunchAtLoginProvider::WindowsStartupFolder
            } else {
                SelectedLaunchAtLoginProvider::Unsupported(LaunchAtLoginProvider::Auto)
            }
        }
        LaunchAtLoginProvider::XdgAutostart => {
            if cfg!(target_os = "linux") {
                SelectedLaunchAtLoginProvider::XdgAutostart
            } else {
                SelectedLaunchAtLoginProvider::Unsupported(LaunchAtLoginProvider::XdgAutostart)
            }
        }
        LaunchAtLoginProvider::WindowsStartupFolder => {
            if cfg!(target_os = "windows") {
                SelectedLaunchAtLoginProvider::WindowsStartupFolder
            } else {
                SelectedLaunchAtLoginProvider::Unsupported(
                    LaunchAtLoginProvider::WindowsStartupFolder,
                )
            }
        }
        LaunchAtLoginProvider::WindowsRegistryRun => {
            if cfg!(target_os = "windows") {
                SelectedLaunchAtLoginProvider::WindowsRegistryRun
            } else {
                SelectedLaunchAtLoginProvider::Unsupported(
                    LaunchAtLoginProvider::WindowsRegistryRun,
                )
            }
        }
        provider => SelectedLaunchAtLoginProvider::Unsupported(provider),
    }
}

fn provider_label(provider: LaunchAtLoginProvider) -> &'static str {
    match provider {
        LaunchAtLoginProvider::Auto => "auto",
        LaunchAtLoginProvider::XdgAutostart => "xdg_autostart",
        LaunchAtLoginProvider::SystemdUser => "systemd_user",
        LaunchAtLoginProvider::WindowsStartupFolder => "windows_startup_folder",
        LaunchAtLoginProvider::WindowsRegistryRun => "windows_registry_run",
    }
}

fn uninstall_all_known_entries(entry: &AutostartEntry) -> Result<(), String> {
    if cfg!(target_os = "linux") {
        let autostart_dir = autostart_dir()?;
        entry.uninstall(&autostart_dir)?;
    }

    #[cfg(target_os = "windows")]
    {
        let startup_dir = windows_startup_dir()?;
        entry.uninstall_windows_shortcut(&startup_dir)?;
        entry.uninstall_registry_run()?;
    }

    Ok(())
}

impl AutostartEntry {
    fn for_current_binary() -> Result<Self, String> {
        let executable = std::env::current_exe()
            .map_err(|error| format!("failed to resolve the current executable path: {error}"))?;

        Ok(Self {
            desktop_file_name: AUTOSTART_DESKTOP_FILE_NAME,
            application_name: APPLICATION_NAME,
            comment: APPLICATION_COMMENT,
            icon_name: APPLICATION_ICON_NAME,
            command: AutostartCommand {
                executable,
                args: Vec::new(),
            },
        })
    }

    fn install(&self, autostart_dir: &Path) -> Result<PathBuf, String> {
        std::fs::create_dir_all(autostart_dir).map_err(|error| {
            format!(
                "failed to create the autostart directory `{}`: {error}",
                autostart_dir.display()
            )
        })?;

        let desktop_file_path = self.desktop_file_path(autostart_dir);
        std::fs::write(&desktop_file_path, self.desktop_file_contents()).map_err(|error| {
            format!(
                "failed to write the autostart entry `{}`: {error}",
                desktop_file_path.display()
            )
        })?;

        Ok(desktop_file_path)
    }

    fn uninstall(&self, autostart_dir: &Path) -> Result<(), String> {
        let desktop_file_path = self.desktop_file_path(autostart_dir);
        match std::fs::remove_file(&desktop_file_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!(
                "failed to remove the autostart entry `{}`: {error}",
                desktop_file_path.display()
            )),
        }
    }

    fn desktop_file_path(&self, autostart_dir: &Path) -> PathBuf {
        autostart_dir.join(self.desktop_file_name)
    }

    fn desktop_file_contents(&self) -> String {
        format!(
            concat!(
                "[Desktop Entry]\n",
                "Type=Application\n",
                "Version=1.0\n",
                "Name={name}\n",
                "Comment={comment}\n",
                "Icon={icon}\n",
                "Exec={exec}\n",
                "Terminal=false\n",
                "StartupWMClass={icon}\n",
                "X-GNOME-Autostart-enabled=true\n",
            ),
            name = self.application_name,
            comment = self.comment,
            icon = self.icon_name,
            exec = self.command.render_exec(),
        )
    }

    #[cfg(target_os = "windows")]
    fn install_windows_shortcut(&self, startup_dir: &Path) -> Result<(), String> {
        std::fs::create_dir_all(startup_dir).map_err(|error| {
            format!(
                "failed to create the Windows startup directory `{}`: {error}",
                startup_dir.display()
            )
        })?;

        run_windows_powershell(
            windows_shortcut_script(
                startup_dir.join(AUTOSTART_SHORTCUT_FILE_NAME).as_path(),
                self.command.executable.as_path(),
                self.command.executable.parent(),
            )
            .as_str(),
        )
    }

    #[cfg(target_os = "windows")]
    fn uninstall_windows_shortcut(&self, startup_dir: &Path) -> Result<(), String> {
        let shortcut_path = startup_dir.join(AUTOSTART_SHORTCUT_FILE_NAME);
        match std::fs::remove_file(&shortcut_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!(
                "failed to remove the Windows startup shortcut `{}`: {error}",
                shortcut_path.display()
            )),
        }
    }

    #[cfg(target_os = "windows")]
    fn install_registry_run(&self) -> Result<(), String> {
        run_windows_powershell(
            windows_registry_set_script(
                WINDOWS_RUN_KEY_PATH,
                WINDOWS_RUN_VALUE_NAME,
                self.command.render_windows_command().as_str(),
            )
            .as_str(),
        )
    }

    #[cfg(target_os = "windows")]
    fn uninstall_registry_run(&self) -> Result<(), String> {
        run_windows_powershell(
            windows_registry_remove_script(WINDOWS_RUN_KEY_PATH, WINDOWS_RUN_VALUE_NAME).as_str(),
        )
    }
}

impl AutostartCommand {
    fn render_exec(&self) -> String {
        let mut parts = Vec::with_capacity(1 + self.args.len());
        parts.push(quote_desktop_exec_arg(
            self.executable.to_string_lossy().as_ref(),
        ));
        parts.extend(self.args.iter().map(|arg| quote_desktop_exec_arg(arg)));
        parts.join(" ")
    }

    #[cfg(target_os = "windows")]
    fn render_windows_command(&self) -> String {
        let mut parts = Vec::with_capacity(1 + self.args.len());
        parts.push(quote_windows_arg(
            self.executable.to_string_lossy().as_ref(),
        ));
        parts.extend(self.args.iter().map(|arg| quote_windows_arg(arg)));
        parts.join(" ")
    }
}

fn quote_desktop_exec_arg(value: &str) -> String {
    let needs_quotes = value.is_empty()
        || value.chars().any(|character| {
            matches!(character, ' ' | '\t' | '\n' | '"' | '\'' | '\\' | '$' | '`')
        });
    if !needs_quotes {
        return value.to_string();
    }

    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for character in value.chars() {
        match character {
            '"' | '\\' | '$' | '`' => {
                quoted.push('\\');
                quoted.push(character);
            }
            _ => quoted.push(character),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(target_os = "windows")]
fn windows_startup_dir() -> Result<PathBuf, String> {
    directories::BaseDirs::new()
        .map(|dirs| {
            dirs.config_dir()
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join("Startup")
        })
        .ok_or_else(|| "failed to resolve the Windows startup directory".to_string())
}

#[cfg(target_os = "windows")]
fn run_windows_powershell(script: &str) -> Result<(), String> {
    let output = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-STA",
            "-Command",
        ])
        .arg(script)
        .output()
        .map_err(|error| format!("failed to start PowerShell: {error}"))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let detail = if stderr.is_empty() {
        format!("exit status {}", output.status)
    } else {
        stderr
    };
    Err(format!(
        "PowerShell launch-at-login integration failed: {detail}"
    ))
}

#[cfg(target_os = "windows")]
fn windows_shortcut_script(
    shortcut_path: &Path,
    target_path: &Path,
    working_directory: Option<&Path>,
) -> String {
    let shortcut_path = powershell_single_quoted(shortcut_path.to_string_lossy().as_ref());
    let target_path = powershell_single_quoted(target_path.to_string_lossy().as_ref());
    let working_directory = working_directory
        .map(|path| powershell_single_quoted(path.to_string_lossy().as_ref()))
        .unwrap_or_default();

    format!(
        concat!(
            "$shell = New-Object -ComObject WScript.Shell;",
            "$shortcut = $shell.CreateShortcut('{shortcut_path}');",
            "$shortcut.TargetPath = '{target_path}';",
            "$shortcut.WorkingDirectory = '{working_directory}';",
            "$shortcut.IconLocation = '{target_path},0';",
            "$shortcut.Save()"
        ),
        shortcut_path = shortcut_path,
        target_path = target_path,
        working_directory = working_directory,
    )
}

#[cfg(target_os = "windows")]
fn windows_registry_set_script(key_path: &str, value_name: &str, command_line: &str) -> String {
    format!(
        concat!(
            "$key = '{key_path}';",
            "if (-not (Test-Path $key)) {{ New-Item -Path $key -Force | Out-Null }};",
            "New-ItemProperty -Path $key -Name '{value_name}' -Value '{command_line}' -PropertyType String -Force | Out-Null"
        ),
        key_path = powershell_single_quoted(key_path),
        value_name = powershell_single_quoted(value_name),
        command_line = powershell_single_quoted(command_line),
    )
}

#[cfg(target_os = "windows")]
fn windows_registry_remove_script(key_path: &str, value_name: &str) -> String {
    format!(
        concat!(
            "$key = '{key_path}';",
            "if (Test-Path $key) {{",
            "  $existing = Get-ItemProperty -Path $key -Name '{value_name}' -ErrorAction SilentlyContinue;",
            "  if ($null -ne $existing) {{",
            "    Remove-ItemProperty -Path $key -Name '{value_name}' -ErrorAction Stop | Out-Null;",
            "  }}",
            "}};",
            "exit 0"
        ),
        key_path = powershell_single_quoted(key_path),
        value_name = powershell_single_quoted(value_name),
    )
}

#[cfg(target_os = "windows")]
fn powershell_single_quoted(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(target_os = "windows")]
fn quote_windows_arg(value: &str) -> String {
    let escaped = value.replace('"', "\"\"");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn desktop_file_contents_include_expected_exec_line() {
        let entry = AutostartEntry {
            desktop_file_name: AUTOSTART_DESKTOP_FILE_NAME,
            application_name: APPLICATION_NAME,
            comment: APPLICATION_COMMENT,
            icon_name: APPLICATION_ICON_NAME,
            command: AutostartCommand {
                executable: PathBuf::from("/tmp/nanite clip"),
                args: vec!["--example".into(), "value with spaces".into()],
            },
        };

        let contents = entry.desktop_file_contents();

        assert!(contents.contains("Name=nanite-clip"));
        assert!(contents.contains("Icon=nanite-clip"));
        assert!(contents.contains("StartupWMClass=nanite-clip"));
        assert!(
            contents.contains("Exec=\"/tmp/nanite clip\" --example \"value with spaces\""),
            "unexpected desktop file contents: {contents}"
        );
    }

    #[test]
    fn install_and_uninstall_manage_desktop_file() {
        let test_dir = unique_test_dir();
        let entry = AutostartEntry {
            desktop_file_name: AUTOSTART_DESKTOP_FILE_NAME,
            application_name: APPLICATION_NAME,
            comment: APPLICATION_COMMENT,
            icon_name: APPLICATION_ICON_NAME,
            command: AutostartCommand {
                executable: PathBuf::from("/tmp/nanite-clip"),
                args: Vec::new(),
            },
        };

        let desktop_file_path = entry.install(&test_dir).unwrap();
        assert!(desktop_file_path.exists());

        entry.uninstall(&test_dir).unwrap();
        assert!(!desktop_file_path.exists());

        let _ = std::fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn auto_provider_resolves_to_xdg_on_linux() {
        let config = LaunchAtLoginConfig {
            enabled: true,
            provider: LaunchAtLoginProvider::Auto,
        };

        if cfg!(target_os = "linux") {
            assert_eq!(
                selected_provider(&config),
                SelectedLaunchAtLoginProvider::XdgAutostart
            );
        } else if cfg!(target_os = "windows") {
            assert_eq!(
                selected_provider(&config),
                SelectedLaunchAtLoginProvider::WindowsStartupFolder
            );
        } else {
            assert_eq!(
                selected_provider(&config),
                SelectedLaunchAtLoginProvider::Unsupported(LaunchAtLoginProvider::Auto)
            );
        }
    }

    #[test]
    fn disabled_config_does_not_select_provider() {
        let config = LaunchAtLoginConfig {
            enabled: false,
            provider: LaunchAtLoginProvider::SystemdUser,
        };

        assert_eq!(
            selected_provider(&config),
            SelectedLaunchAtLoginProvider::Disabled
        );
    }

    fn unique_test_dir() -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("nanite-clip-autostart-test-{timestamp}"))
    }
}

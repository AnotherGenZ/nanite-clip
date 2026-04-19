use std::path::Path;
use std::process::Command;

pub fn open_path(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err(format!("Clip file does not exist: {}", path.display()));
    }

    spawn_platform_opener(path)
}

pub fn open_url(url: &str) -> Result<(), String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("URL is empty".into());
    }

    spawn_platform_url_opener(url)
}

pub fn launch_command(program: &str, args: &[String], display: &str) -> Result<(), String> {
    if program.trim().is_empty() {
        return Err("Command program is empty".into());
    }

    spawn_platform_command(program, args, display)
}

#[cfg(target_os = "linux")]
fn spawn_platform_opener(path: &Path) -> Result<(), String> {
    let mut last_error = None;

    for (program, extra_args) in [("xdg-open", &[][..]), ("gio", &["open"][..])] {
        let mut command = Command::new(program);
        command.args(extra_args).arg(path);

        match command.spawn() {
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                last_error = Some(format!("{program} is not installed"));
            }
            Err(error) => {
                return Err(format!(
                    "Failed to launch {program} for {}: {error}",
                    path.display()
                ));
            }
        }
    }

    Err(last_error
        .unwrap_or_else(|| format!("No desktop opener is available for {}", path.display())))
}

#[cfg(target_os = "linux")]
fn spawn_platform_url_opener(url: &str) -> Result<(), String> {
    let mut last_error = None;

    for (program, extra_args) in [("xdg-open", &[][..]), ("gio", &["open"][..])] {
        let mut command = Command::new(program);
        command.args(extra_args).arg(url);

        match command.spawn() {
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                last_error = Some(format!("{program} is not installed"));
            }
            Err(error) => {
                return Err(format!("Failed to launch {program} for {url}: {error}"));
            }
        }
    }

    Err(last_error.unwrap_or_else(|| format!("No desktop opener is available for {url}")))
}

#[cfg(target_os = "linux")]
fn spawn_platform_command(program: &str, args: &[String], display: &str) -> Result<(), String> {
    let mut last_error = None;

    for (terminal, terminal_args) in [
        ("kgx", &["--"][..]),
        ("gnome-terminal", &["--"][..]),
        ("konsole", &["-e"][..]),
        ("xfce4-terminal", &["-x"][..]),
        ("mate-terminal", &["-x"][..]),
        ("tilix", &["-e"][..]),
        ("xterm", &["-e"][..]),
        ("kitty", &["sh", "-lc"][..]),
        ("alacritty", &["-e", "sh", "-lc"][..]),
        ("wezterm", &["start", "--always-new-process", "--"][..]),
        ("footclient", &["sh", "-lc"][..]),
        ("foot", &["sh", "-lc"][..]),
        ("lxterminal", &["-e"][..]),
    ] {
        let mut command = Command::new(terminal);
        command.args(terminal_args);
        if matches!(terminal, "kitty" | "footclient" | "foot") {
            command.arg(display);
        } else {
            command.args(["sh", "-lc", display]);
        }

        match command.spawn() {
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                last_error = Some(format!("{terminal} is not installed"));
            }
            Err(error) => {
                return Err(format!(
                    "Failed to launch {terminal} for `{display}`: {error}"
                ));
            }
        }
    }

    Command::new(program)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|error| {
            let launcher_error =
                last_error.unwrap_or_else(|| "no terminal launcher was available".into());
            format!("Failed to run `{display}` directly after {launcher_error}: {error}")
        })
}

#[cfg(target_os = "macos")]
fn spawn_platform_opener(path: &Path) -> Result<(), String> {
    Command::new("open")
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Failed to launch open for {}: {error}", path.display()))
}

#[cfg(target_os = "macos")]
fn spawn_platform_url_opener(url: &str) -> Result<(), String> {
    Command::new("open")
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Failed to launch open for {url}: {error}"))
}

#[cfg(target_os = "macos")]
fn spawn_platform_command(program: &str, args: &[String], display: &str) -> Result<(), String> {
    Command::new(program)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Failed to launch `{display}`: {error}"))
}

#[cfg(target_os = "windows")]
fn spawn_platform_opener(path: &Path) -> Result<(), String> {
    Command::new("cmd")
        .args(["/C", "start", ""])
        .arg(path)
        .spawn()
        .map(|_| ())
        .map_err(|error| {
            format!(
                "Failed to launch the default app for {}: {error}",
                path.display()
            )
        })
}

#[cfg(target_os = "windows")]
fn spawn_platform_url_opener(url: &str) -> Result<(), String> {
    Command::new("cmd")
        .args(["/C", "start", ""])
        .arg(url)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Failed to launch the default app for {url}: {error}"))
}

#[cfg(target_os = "windows")]
fn spawn_platform_command(program: &str, args: &[String], display: &str) -> Result<(), String> {
    Command::new(program)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Failed to launch `{display}`: {error}"))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn spawn_platform_opener(path: &Path) -> Result<(), String> {
    Err(format!(
        "Opening clips is not supported on this platform for {}",
        path.display()
    ))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn spawn_platform_url_opener(url: &str) -> Result<(), String> {
    Err(format!(
        "Opening URLs is not supported on this platform for {url}"
    ))
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
fn spawn_platform_command(_program: &str, _args: &[String], display: &str) -> Result<(), String> {
    Err(format!(
        "Launching commands is not supported on this platform for `{display}`"
    ))
}

use std::ffi::OsStr;
use std::io;
use std::process::{Child, Command, ExitStatus, Output};

#[derive(Debug, thiserror::Error)]
#[allow(dead_code)]
pub enum CommandError {
    #[error("failed to start `{command}`: {source}")]
    Spawn {
        command: String,
        #[source]
        source: io::Error,
    },
    #[error("`{command}` exited with {status}{stderr}")]
    Failed {
        command: String,
        status: ExitStatus,
        stderr: String,
    },
}

pub fn command_available(program: &str) -> bool {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path).any(|entry| {
        let candidate = entry.join(program);
        if candidate.is_file() {
            return true;
        }

        #[cfg(target_os = "windows")]
        {
            for ext in ["exe", "cmd", "bat"] {
                if entry.join(format!("{program}.{ext}")).is_file() {
                    return true;
                }
            }
        }

        false
    })
}

pub fn spawn(command: &mut Command) -> Result<Child, CommandError> {
    let display = describe(command);
    command.spawn().map_err(|source| CommandError::Spawn {
        command: display,
        source,
    })
}

pub fn output(command: &mut Command) -> Result<Output, CommandError> {
    let display = describe(command);
    command.output().map_err(|source| CommandError::Spawn {
        command: display,
        source,
    })
}

pub fn status(command: &mut Command) -> Result<ExitStatus, CommandError> {
    let display = describe(command);
    command.status().map_err(|source| CommandError::Spawn {
        command: display,
        source,
    })
}

#[allow(dead_code)]
pub fn check_output(command: &mut Command) -> Result<Output, CommandError> {
    let display = describe(command);
    let output = command.output().map_err(|source| CommandError::Spawn {
        command: display.clone(),
        source,
    })?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(CommandError::Failed {
            command: display,
            status: output.status,
            stderr: stderr_summary(&output.stderr),
        })
    }
}

#[allow(dead_code)]
pub fn check_status(command: &mut Command) -> Result<(), CommandError> {
    let display = describe(command);
    let output = command.output().map_err(|source| CommandError::Spawn {
        command: display.clone(),
        source,
    })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(CommandError::Failed {
            command: display,
            status: output.status,
            stderr: stderr_summary(&output.stderr),
        })
    }
}

fn describe(command: &Command) -> String {
    let program = command.get_program().to_string_lossy();
    let args = command
        .get_args()
        .map(quote_os_str)
        .collect::<Vec<_>>()
        .join(" ");
    if args.is_empty() {
        program.into_owned()
    } else {
        format!("{program} {args}")
    }
}

fn quote_os_str(value: &OsStr) -> String {
    let text = value.to_string_lossy();
    if text.contains(' ') {
        format!("\"{text}\"")
    } else {
        text.into_owned()
    }
}

#[allow(dead_code)]
fn stderr_summary(stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();
    if stderr.is_empty() {
        String::new()
    } else {
        format!(": {stderr}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_quotes_arguments_with_spaces() {
        let mut command = Command::new("echo");
        command.arg("hello world").arg("plain");
        assert_eq!(describe(&command), "echo \"hello world\" plain");
    }

    #[test]
    fn check_output_reports_missing_binaries() {
        let mut command = Command::new("nanite-clip-test-missing-binary");
        let error = check_output(&mut command).unwrap_err();
        assert!(matches!(error, CommandError::Spawn { .. }));
    }

    #[cfg(unix)]
    #[test]
    fn check_output_captures_stderr_from_failed_commands() {
        let mut command = Command::new("sh");
        command.args(["-c", "printf 'boom' >&2; exit 7"]);
        let error = check_output(&mut command).unwrap_err();
        match error {
            CommandError::Failed { stderr, .. } => assert_eq!(stderr, ": boom"),
            other => panic!("expected failed command error, got {other:?}"),
        }
    }

    #[cfg(windows)]
    #[test]
    fn check_output_captures_stderr_from_failed_commands() {
        let mut command = Command::new("cmd");
        command.args(["/C", "echo boom 1>&2 && exit /b 7"]);
        let error = check_output(&mut command).unwrap_err();
        match error {
            CommandError::Failed { stderr, .. } => assert!(stderr.contains("boom")),
            other => panic!("expected failed command error, got {other:?}"),
        }
    }
}

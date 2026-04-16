use std::path::{Path, PathBuf};
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdout, Command};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{Duration, timeout};

const PLATFORM_SERVICE_PROTOCOL_VERSION: u32 = 1;
const PLATFORM_SERVICE_PATH: &str = env!("NANITE_CLIP_PLATFORM_SERVICE_PATH");
const PLATFORM_SERVICE_BINARY_NAME: &str = "nanite-clip-platform-service";

pub struct PlatformHotkeyServiceHandle {
    binding_label: String,
    configuration_note: Option<String>,
    receiver: mpsc::UnboundedReceiver<PlatformHotkeyEvent>,
    shutdown: Option<oneshot::Sender<()>>,
    task: JoinHandle<()>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformHotkeyEvent {
    Activated,
}

#[derive(Debug)]
struct PlatformHotkeyStartup {
    binding_label: String,
    configuration_note: Option<String>,
}

#[derive(Serialize)]
struct PlatformServiceRequestEnvelope {
    protocol_version: u32,
    #[serde(flatten)]
    request: PlatformServiceRequest,
}

#[derive(Serialize)]
#[serde(tag = "service", rename_all = "snake_case")]
enum PlatformServiceRequest {
    PlasmaManualClipHotkey {
        combined_key: i32,
        action_id: String,
        description: String,
    },
}

#[derive(Deserialize)]
struct PlatformServiceMessageEnvelope {
    protocol_version: u32,
    #[serde(flatten)]
    event: PlatformServiceMessage,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum PlatformServiceMessage {
    Ready {
        binding_label: String,
        #[serde(default)]
        configuration_note: Option<String>,
    },
    Note {
        message: String,
    },
    Error {
        message: String,
    },
    Activated,
}

impl PlatformHotkeyServiceHandle {
    pub fn binding_label(&self) -> &str {
        &self.binding_label
    }

    pub fn configuration_note(&self) -> Option<&str> {
        self.configuration_note.as_deref()
    }

    pub fn drain_events(&mut self) -> Vec<PlatformHotkeyEvent> {
        self.receiver.try_recv().into_iter().collect()
    }
}

impl Drop for PlatformHotkeyServiceHandle {
    fn drop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
        self.task.abort();
    }
}

pub async fn start_plasma_manual_clip_hotkey(
    combined_key: i32,
    action_id: &str,
    description: &str,
) -> Result<PlatformHotkeyServiceHandle, String> {
    let platform_service_path = platform_service_path()
        .ok_or_else(|| "the platform service backend is not available in this build".to_string())?;
    let request = PlatformServiceRequestEnvelope {
        protocol_version: PLATFORM_SERVICE_PROTOCOL_VERSION,
        request: PlatformServiceRequest::PlasmaManualClipHotkey {
            combined_key,
            action_id: action_id.to_string(),
            description: description.to_string(),
        },
    };
    let request = serde_json::to_vec(&request)
        .map_err(|error| format!("failed to serialize the platform service request: {error}"))?;

    let mut child = Command::new(&platform_service_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| {
            format!(
                "failed to start the platform service `{}`: {error}",
                platform_service_path.display()
            )
        })?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "failed to capture the platform service stdin".to_string())?;
    stdin
        .write_all(&request)
        .await
        .map_err(|error| format!("failed to write the platform service request: {error}"))?;
    stdin.write_all(b"\n").await.map_err(|error| {
        format!("failed to terminate the platform service request line: {error}")
    })?;
    stdin
        .flush()
        .await
        .map_err(|error| format!("failed to flush the platform service request: {error}"))?;
    drop(stdin);

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "failed to capture the platform service stdout".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "failed to capture the platform service stderr".to_string())?;

    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let (startup_tx, startup_rx) = oneshot::channel();
    let task = tokio::spawn(run_platform_service_session(
        child,
        stdout,
        stderr,
        event_tx,
        startup_tx,
        shutdown_rx,
    ));

    let startup = match timeout(Duration::from_secs(5), startup_rx).await {
        Ok(Ok(Ok(startup))) => startup,
        Ok(Ok(Err(error))) => {
            let _ = shutdown_tx.send(());
            task.abort();
            return Err(error);
        }
        Ok(Err(_)) => {
            let _ = shutdown_tx.send(());
            task.abort();
            return Err("the platform service stopped before becoming ready".into());
        }
        Err(_) => {
            let _ = shutdown_tx.send(());
            task.abort();
            return Err("the platform service did not become ready in time".into());
        }
    };

    Ok(PlatformHotkeyServiceHandle {
        binding_label: startup.binding_label,
        configuration_note: startup.configuration_note,
        receiver: event_rx,
        shutdown: Some(shutdown_tx),
        task,
    })
}

async fn run_platform_service_session(
    mut child: Child,
    stdout: ChildStdout,
    stderr: ChildStderr,
    event_tx: mpsc::UnboundedSender<PlatformHotkeyEvent>,
    startup_tx: oneshot::Sender<Result<PlatformHotkeyStartup, String>>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let mut stdout_lines = BufReader::new(stdout).lines();
    let mut stderr_lines = BufReader::new(stderr).lines();
    let mut startup_tx = Some(startup_tx);
    let mut configuration_note = None;

    loop {
        tokio::select! {
            _ = &mut shutdown_rx => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                return;
            }
            status = child.wait() => {
                if let Some(startup_tx) = startup_tx.take() {
                    let message = match status {
                        Ok(status) => format!("the platform service exited before becoming ready with status {status}"),
                        Err(error) => format!("failed while waiting for the platform service: {error}"),
                    };
                    let _ = startup_tx.send(Err(message));
                }
                return;
            }
            line = stdout_lines.next_line() => {
                match line {
                    Ok(Some(line)) => {
                        if line.trim().is_empty() {
                            continue;
                        }

                        match parse_platform_service_message(&line) {
                            Ok(PlatformServiceMessage::Ready { binding_label, configuration_note: ready_note }) => {
                                let configuration_note = ready_note.or_else(|| configuration_note.take());
                                if let Some(startup_tx) = startup_tx.take() {
                                    let _ = startup_tx.send(Ok(PlatformHotkeyStartup {
                                        binding_label,
                                        configuration_note,
                                    }));
                                }
                            }
                            Ok(PlatformServiceMessage::Note { message }) => {
                                if startup_tx.is_some() {
                                    configuration_note = Some(message);
                                } else {
                                    tracing::info!("platform service note: {message}");
                                }
                            }
                            Ok(PlatformServiceMessage::Error { message }) => {
                                if let Some(startup_tx) = startup_tx.take() {
                                    let _ = startup_tx.send(Err(message.trim().to_string()));
                                } else {
                                    tracing::warn!("platform service error: {message}");
                                }
                                let _ = child.start_kill();
                                let _ = child.wait().await;
                                return;
                            }
                            Ok(PlatformServiceMessage::Activated) => {
                                let _ = event_tx.send(PlatformHotkeyEvent::Activated);
                            }
                            Err(error) => {
                                if let Some(startup_tx) = startup_tx.take() {
                                    let _ = startup_tx.send(Err(error.clone()));
                                } else {
                                    tracing::warn!("failed to parse the platform service output: {error}");
                                }
                                let _ = child.start_kill();
                                let _ = child.wait().await;
                                return;
                            }
                        }
                    }
                    Ok(None) => {
                        if let Some(startup_tx) = startup_tx.take() {
                            let _ = startup_tx.send(Err(
                                "the platform service closed its stdout before becoming ready"
                                    .to_string(),
                            ));
                        }
                        return;
                    }
                    Err(error) => {
                        if let Some(startup_tx) = startup_tx.take() {
                            let _ = startup_tx.send(Err(format!(
                                "failed to read the platform service output: {error}"
                            )));
                        } else {
                            tracing::warn!("failed to read the platform service output: {error}");
                        }
                        return;
                    }
                }
            }
            line = stderr_lines.next_line() => {
                match line {
                    Ok(Some(line)) if !line.trim().is_empty() => {
                        tracing::info!("platform service: {line}");
                    }
                    Ok(Some(_)) => {}
                    Ok(None) => {}
                    Err(error) => {
                        tracing::warn!("failed to read the platform service stderr: {error}");
                    }
                }
            }
        }
    }
}

fn parse_platform_service_message(line: &str) -> Result<PlatformServiceMessage, String> {
    let message: PlatformServiceMessageEnvelope = serde_json::from_str(line)
        .map_err(|error| format!("invalid platform service message `{line}`: {error}"))?;
    if message.protocol_version != PLATFORM_SERVICE_PROTOCOL_VERSION {
        return Err(format!(
            "unsupported platform service protocol version `{}`",
            message.protocol_version
        ));
    }

    Ok(message.event)
}

fn platform_service_path() -> Option<PathBuf> {
    bundled_platform_service_paths()
        .into_iter()
        .find(|path| path.is_file())
        .or_else(|| {
            let path = PLATFORM_SERVICE_PATH.trim();
            (!path.is_empty())
                .then(|| Path::new(path).to_path_buf())
                .filter(|path| path.is_file())
        })
}

fn bundled_platform_service_paths() -> Vec<PathBuf> {
    let Ok(current_exe) = std::env::current_exe() else {
        return Vec::new();
    };
    let Some(exe_dir) = current_exe.parent() else {
        return Vec::new();
    };

    vec![
        exe_dir.join(PLATFORM_SERVICE_BINARY_NAME),
        exe_dir
            .parent()
            .map(|prefix| {
                prefix
                    .join("lib")
                    .join("nanite-clip")
                    .join(PLATFORM_SERVICE_BINARY_NAME)
            })
            .unwrap_or_else(|| exe_dir.join(PLATFORM_SERVICE_BINARY_NAME)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ready_message() {
        let message = parse_platform_service_message(
            r#"{"protocol_version":1,"event":"ready","binding_label":"Num+Enter"}"#,
        )
        .unwrap();

        match message {
            PlatformServiceMessage::Ready {
                binding_label,
                configuration_note,
            } => {
                assert_eq!(binding_label, "Num+Enter");
                assert_eq!(configuration_note, None);
            }
            other => panic!("unexpected message: {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_protocol_version() {
        let error = parse_platform_service_message(r#"{"protocol_version":2,"event":"activated"}"#)
            .unwrap_err();
        assert!(error.contains("unsupported platform service protocol version"));
    }

    #[test]
    fn bundled_paths_include_sibling_and_lib_locations() {
        let paths = bundled_platform_service_paths();
        let expected_lib_path = std::path::Path::new("lib")
            .join("nanite-clip")
            .join(PLATFORM_SERVICE_BINARY_NAME);

        assert!(paths.len() >= 2);
        assert!(
            paths
                .iter()
                .any(|path| path.ends_with("nanite-clip-platform-service"))
        );
        assert!(paths.iter().any(|path| path.ends_with(&expected_lib_path)));
    }
}

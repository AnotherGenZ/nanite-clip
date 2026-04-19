use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Command;

#[cfg(target_os = "windows")]
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{HLOCAL, LocalFree};
#[cfg(target_os = "windows")]
use windows::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};

use crate::command_runner;

const SERVICE_LABEL: &str = "nanite-clip";
#[cfg(target_os = "windows")]
const WINDOWS_DPAPI_PREFIX: &str = "dpapi:";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SecretKey {
    CopypartyPassword,
    YoutubeClientSecret,
    YoutubeRefreshToken,
    DiscordWebhookUrl,
    ObsWebsocketPassword,
}

impl SecretKey {
    fn label(self) -> &'static str {
        match self {
            Self::CopypartyPassword => "Copyparty password",
            Self::YoutubeClientSecret => "YouTube client secret",
            Self::YoutubeRefreshToken => "YouTube refresh token",
            Self::DiscordWebhookUrl => "Discord webhook URL",
            Self::ObsWebsocketPassword => "OBS websocket password",
        }
    }

    fn entry_name(self) -> &'static str {
        match self {
            Self::CopypartyPassword => "copyparty_password",
            Self::YoutubeClientSecret => "youtube_client_secret",
            Self::YoutubeRefreshToken => "youtube_refresh_token",
            Self::DiscordWebhookUrl => "discord_webhook_url",
            Self::ObsWebsocketPassword => "obs_websocket_password",
        }
    }

    fn legacy_entry_names(self) -> &'static [&'static str] {
        match self {
            Self::CopypartyPassword => &["streamable_secret"],
            Self::YoutubeClientSecret
            | Self::YoutubeRefreshToken
            | Self::DiscordWebhookUrl
            | Self::ObsWebsocketPassword => &[],
        }
    }

    fn attribute_name(self) -> &'static str {
        self.entry_name()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecureStoreBackend {
    SecretTool,
    WindowsDpapi,
    LocalFile,
}

impl SecureStoreBackend {
    pub fn label(self) -> &'static str {
        match self {
            Self::SecretTool => "system keyring (secret-tool)",
            Self::WindowsDpapi => "Windows DPAPI",
            Self::LocalFile => "local credential store",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SecureStore {
    backend: SecureStoreBackend,
    file_path: PathBuf,
}

impl SecureStore {
    pub fn new() -> Self {
        let backend = if cfg!(target_os = "windows") {
            SecureStoreBackend::WindowsDpapi
        } else if command_runner::command_available("secret-tool") {
            SecureStoreBackend::SecretTool
        } else {
            SecureStoreBackend::LocalFile
        };
        Self {
            backend,
            file_path: credentials_path(),
        }
    }

    pub fn backend(&self) -> SecureStoreBackend {
        self.backend
    }

    pub fn get(&self, key: SecretKey) -> Result<Option<String>, String> {
        info!(
            backend = %self.backend.label(),
            key = key.label(),
            "Secure store get requested"
        );
        match self.backend {
            SecureStoreBackend::SecretTool => self.get_secret_tool(key),
            SecureStoreBackend::WindowsDpapi => self.get_windows_dpapi(key),
            SecureStoreBackend::LocalFile => self.get_file(key),
        }
    }

    pub fn set(&self, key: SecretKey, value: &str) -> Result<(), String> {
        if value.trim().is_empty() {
            info!(
                key = key.label(),
                "Secure store set received empty value; deleting instead"
            );
            return self.delete(key);
        }

        info!(
            backend = %self.backend.label(),
            key = key.label(),
            value_len = value.len(),
            "Secure store set requested"
        );
        match self.backend {
            SecureStoreBackend::SecretTool => self.set_secret_tool(key, value),
            SecureStoreBackend::WindowsDpapi => self.set_windows_dpapi(key, value),
            SecureStoreBackend::LocalFile => self.set_file(key, value),
        }
    }

    pub fn delete(&self, key: SecretKey) -> Result<(), String> {
        info!(
            backend = %self.backend.label(),
            key = key.label(),
            "Secure store delete requested"
        );
        match self.backend {
            SecureStoreBackend::SecretTool => self.delete_secret_tool(key),
            SecureStoreBackend::WindowsDpapi => self.delete_windows_dpapi(key),
            SecureStoreBackend::LocalFile => self.delete_file(key),
        }
    }

    pub fn contains(&self, key: SecretKey) -> Result<bool, String> {
        Ok(self.get(key)?.is_some())
    }

    fn get_secret_tool(&self, key: SecretKey) -> Result<Option<String>, String> {
        for entry_name in
            std::iter::once(key.attribute_name()).chain(key.legacy_entry_names().iter().copied())
        {
            let mut command = Command::new("secret-tool");
            command
                .args(["lookup", "service", SERVICE_LABEL, "entry"])
                .arg(entry_name);
            let output = command_runner::output(&mut command)
                .map_err(|error| format!("failed to query secret-tool: {error}"))?;

            if output.status.success() {
                let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !value.is_empty() {
                    info!(
                        key = key.label(),
                        entry = entry_name,
                        "secret-tool lookup found value"
                    );
                    return Ok(Some(value));
                }
                info!(
                    key = key.label(),
                    entry = entry_name,
                    "secret-tool lookup returned empty value"
                );
            } else if output.status.code() != Some(1) {
                warn!(
                    key = key.label(),
                    entry = entry_name,
                    status = %output.status,
                    stderr = %String::from_utf8_lossy(&output.stderr).trim(),
                    "secret-tool lookup failed"
                );
                return Err(format!(
                    "secret-tool lookup failed: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ));
            }
        }

        info!(key = key.label(), "secret-tool lookup did not find a value");
        Ok(None)
    }

    fn set_secret_tool(&self, key: SecretKey, value: &str) -> Result<(), String> {
        let mut command = Command::new("secret-tool");
        command
            .args([
                "store",
                "--label",
                SERVICE_LABEL,
                "service",
                SERVICE_LABEL,
                "entry",
            ])
            .arg(key.attribute_name())
            .stdin(std::process::Stdio::piped());
        let mut child = command_runner::spawn(&mut command)
            .map_err(|error| format!("failed to start secret-tool store: {error}"))?;

        use std::io::Write;
        {
            let mut stdin = child
                .stdin
                .take()
                .ok_or_else(|| "failed to open secret-tool stdin".to_string())?;
            stdin
                .write_all(value.as_bytes())
                .map_err(|error| format!("failed to write secret-tool input: {error}"))?;
        }
        info!(
            key = key.label(),
            "secret-tool input written and stdin closed"
        );

        let status = child
            .wait()
            .map_err(|error| format!("failed to wait for secret-tool store: {error}"))?;
        if status.success() {
            info!(
                key = key.label(),
                "secret-tool store completed successfully"
            );
            Ok(())
        } else {
            warn!(key = key.label(), status = %status, "secret-tool store failed");
            Err(format!("secret-tool store exited with status {status}"))
        }
    }

    fn delete_secret_tool(&self, key: SecretKey) -> Result<(), String> {
        for entry_name in
            std::iter::once(key.attribute_name()).chain(key.legacy_entry_names().iter().copied())
        {
            let mut command = Command::new("secret-tool");
            command
                .args(["clear", "service", SERVICE_LABEL, "entry"])
                .arg(entry_name);
            let status = command_runner::status(&mut command)
                .map_err(|error| format!("failed to start secret-tool clear: {error}"))?;

            if !status.success() && status.code() != Some(1) {
                warn!(key = key.label(), entry = entry_name, status = %status, "secret-tool clear failed");
                return Err(format!("secret-tool clear exited with status {status}"));
            }
        }

        info!(key = key.label(), "secret-tool clear completed");
        Ok(())
    }

    fn get_file(&self, key: SecretKey) -> Result<Option<String>, String> {
        let secrets = self.read_file_store()?;
        if let Some(value) = secrets.entries.get(key.entry_name()) {
            info!(key = key.label(), "file credential store found value");
            return Ok(Some(value.clone()));
        }

        let value = key
            .legacy_entry_names()
            .iter()
            .find_map(|entry_name| secrets.entries.get(*entry_name).cloned());
        info!(
            key = key.label(),
            found = value.is_some(),
            "file credential store lookup finished"
        );
        Ok(value)
    }

    #[cfg(target_os = "windows")]
    fn get_windows_dpapi(&self, key: SecretKey) -> Result<Option<String>, String> {
        let secrets = self.read_file_store()?;
        let value = secrets.entries.get(key.entry_name()).cloned().or_else(|| {
            key.legacy_entry_names()
                .iter()
                .find_map(|entry_name| secrets.entries.get(*entry_name).cloned())
        });
        let Some(value) = value else {
            info!(
                key = key.label(),
                "Windows DPAPI store did not find a value"
            );
            return Ok(None);
        };

        decode_windows_secret(&value).map(Some)
    }

    #[cfg(not(target_os = "windows"))]
    fn get_windows_dpapi(&self, _key: SecretKey) -> Result<Option<String>, String> {
        Err("Windows DPAPI is not available on this platform".into())
    }

    fn set_file(&self, key: SecretKey, value: &str) -> Result<(), String> {
        let mut secrets = self.read_file_store()?;
        secrets
            .entries
            .insert(key.entry_name().to_string(), value.to_string());
        self.write_file_store(&secrets)?;
        info!(key = key.label(), path = %self.file_path.display(), "file credential store write completed");
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn set_windows_dpapi(&self, key: SecretKey, value: &str) -> Result<(), String> {
        let mut secrets = self.read_file_store()?;
        secrets
            .entries
            .insert(key.entry_name().to_string(), encode_windows_secret(value)?);
        self.write_file_store(&secrets)?;
        info!(
            key = key.label(),
            path = %self.file_path.display(),
            "Windows DPAPI credential store write completed"
        );
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    fn set_windows_dpapi(&self, _key: SecretKey, _value: &str) -> Result<(), String> {
        Err("Windows DPAPI is not available on this platform".into())
    }

    fn delete_file(&self, key: SecretKey) -> Result<(), String> {
        let mut secrets = self.read_file_store()?;
        secrets.entries.remove(key.entry_name());
        for entry_name in key.legacy_entry_names() {
            secrets.entries.remove(*entry_name);
        }
        self.write_file_store(&secrets)?;
        info!(key = key.label(), path = %self.file_path.display(), "file credential store delete completed");
        Ok(())
    }

    #[cfg(target_os = "windows")]
    fn delete_windows_dpapi(&self, key: SecretKey) -> Result<(), String> {
        self.delete_file(key)
    }

    #[cfg(not(target_os = "windows"))]
    fn delete_windows_dpapi(&self, _key: SecretKey) -> Result<(), String> {
        Err("Windows DPAPI is not available on this platform".into())
    }

    fn read_file_store(&self) -> Result<SecretFile, String> {
        match std::fs::read_to_string(&self.file_path) {
            Ok(contents) => serde_json::from_str(&contents)
                .map_err(|error| format!("failed to parse {}: {error}", self.file_path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(SecretFile::default()),
            Err(error) => Err(format!(
                "failed to read {}: {error}",
                self.file_path.display()
            )),
        }
    }

    fn write_file_store(&self, secrets: &SecretFile) -> Result<(), String> {
        if let Some(parent) = self.file_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create credential directory {}: {error}",
                    parent.display()
                )
            })?;
        }

        let contents = serde_json::to_vec_pretty(secrets)
            .map_err(|error| format!("failed to encode credential store: {error}"))?;
        let temp_path = self.file_path.with_extension("json.tmp");
        std::fs::write(&temp_path, contents).map_err(|error| {
            format!(
                "failed to write temporary credential store {}: {error}",
                temp_path.display()
            )
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&temp_path, std::fs::Permissions::from_mode(0o600)).map_err(
                |error| {
                    format!(
                        "failed to apply permissions to {}: {error}",
                        temp_path.display()
                    )
                },
            )?;
        }

        #[cfg(target_os = "windows")]
        if self.file_path.exists() {
            std::fs::remove_file(&self.file_path).map_err(|error| {
                format!(
                    "failed to replace existing credential store {}: {error}",
                    self.file_path.display()
                )
            })?;
        }

        std::fs::rename(&temp_path, &self.file_path).map_err(|error| {
            format!(
                "failed to finalize credential store {}: {error}",
                self.file_path.display()
            )
        })?;
        Ok(())
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SecretFile {
    #[serde(default)]
    entries: BTreeMap<String, String>,
}

fn credentials_path() -> PathBuf {
    directories::ProjectDirs::from("", "", "nanite-clip")
        .map(|dirs| dirs.config_dir().join("credentials.json"))
        .unwrap_or_else(|| PathBuf::from("nanite-clip-credentials.json"))
}

#[cfg(target_os = "windows")]
fn encode_windows_secret(value: &str) -> Result<String, String> {
    let plaintext = value.as_bytes();
    let input = CRYPT_INTEGER_BLOB {
        cbData: plaintext.len() as u32,
        pbData: plaintext.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB::default();

    // SAFETY: DPAPI reads the input bytes during the call and initializes `output` on success.
    let result = unsafe {
        CryptProtectData(
            &input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    }
    .map_err(|error| format!("Windows DPAPI encryption failed: {error}"));

    result?;

    let encrypted = blob_to_vec(&output);
    free_blob(&mut output);
    Ok(format!(
        "{WINDOWS_DPAPI_PREFIX}{}",
        base64::engine::general_purpose::STANDARD.encode(encrypted)
    ))
}

#[cfg(target_os = "windows")]
fn decode_windows_secret(value: &str) -> Result<String, String> {
    if let Some(encoded) = value.strip_prefix(WINDOWS_DPAPI_PREFIX) {
        let ciphertext = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|error| format!("invalid Windows DPAPI payload: {error}"))?;
        let input = CRYPT_INTEGER_BLOB {
            cbData: ciphertext.len() as u32,
            pbData: ciphertext.as_ptr() as *mut u8,
        };
        let mut output = CRYPT_INTEGER_BLOB::default();

        // SAFETY: DPAPI reads the input bytes during the call and initializes `output` on success.
        let result = unsafe {
            CryptUnprotectData(
                &input,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
        .map_err(|error| format!("Windows DPAPI decryption failed: {error}"));

        result?;

        let decrypted = blob_to_vec(&output);
        free_blob(&mut output);
        return String::from_utf8(decrypted)
            .map_err(|error| format!("Windows DPAPI produced invalid UTF-8: {error}"));
    }

    Ok(value.to_string())
}

#[cfg(target_os = "windows")]
fn blob_to_vec(blob: &CRYPT_INTEGER_BLOB) -> Vec<u8> {
    if blob.cbData == 0 || blob.pbData.is_null() {
        return Vec::new();
    }

    // SAFETY: `pbData` points to `cbData` initialized bytes owned by the Windows API.
    unsafe { std::slice::from_raw_parts(blob.pbData, blob.cbData as usize) }.to_vec()
}

#[cfg(target_os = "windows")]
fn free_blob(blob: &mut CRYPT_INTEGER_BLOB) {
    if blob.pbData.is_null() {
        return;
    }

    // SAFETY: `pbData` was allocated by the Windows API and must be released with LocalFree.
    let _ = unsafe { LocalFree(Some(HLOCAL(blob.pbData as *mut core::ffi::c_void))) };
    blob.pbData = std::ptr::null_mut();
    blob.cbData = 0;
}

impl Default for SecureStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_is_always_selected() {
        let store = SecureStore::new();
        if cfg!(target_os = "windows") {
            assert!(matches!(store.backend(), SecureStoreBackend::WindowsDpapi));
        } else {
            assert!(matches!(
                store.backend(),
                SecureStoreBackend::SecretTool | SecureStoreBackend::LocalFile
            ));
        }
    }
}

use super::*;

pub(crate) fn ensure_supported_obs_version(version: &ObsVersionInfo) -> Result<(), CaptureError> {
    if version.obs_studio_version.major < OBS_MIN_STUDIO_VERSION_MAJOR {
        return Err(CaptureError::SpawnFailed(format!(
            "OBS Studio {} is too old. NaniteClip requires OBS Studio 28.0 or newer.",
            version.obs_studio_version
        )));
    }

    if !version.obs_studio_version.pre.is_empty() {
        return Err(CaptureError::SpawnFailed(format!(
            "OBS Studio {} is a pre-release build. NaniteClip only supports stable OBS releases.",
            version.obs_studio_version
        )));
    }

    if version.obs_web_socket_version.major != OBS_SUPPORTED_WEBSOCKET_VERSION_MAJOR {
        return Err(CaptureError::SpawnFailed(format!(
            "obs-websocket {} is unsupported. NaniteClip requires obs-websocket major version {}.",
            version.obs_web_socket_version, OBS_SUPPORTED_WEBSOCKET_VERSION_MAJOR
        )));
    }

    if version.rpc_version != OBS_SUPPORTED_RPC_VERSION {
        return Err(CaptureError::SpawnFailed(format!(
            "obs-websocket RPC version {} is unsupported. NaniteClip requires RPC version {}.",
            version.rpc_version, OBS_SUPPORTED_RPC_VERSION
        )));
    }

    Ok(())
}

pub(crate) fn map_container(container: &str) -> Result<String, CaptureError> {
    let normalized = container.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "mkv" | "mp4" | "mov" | "flv" | "ts" | "m3u8" | "hls" | "fragmented_mp4"
        | "fragmented_mov" | "mpegts" => Ok(normalized),
        other => Err(CaptureError::Unsupported {
            capability: format!(
                "Container `{other}` is not supported by OBS. Use mkv, mp4, mov, flv, ts, m3u8, hls, fragmented_mp4, fragmented_mov, or mpegts."
            ),
        }),
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ObsConnectionTarget {
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) tls: bool,
}

pub(crate) fn parse_websocket_url(url: &str) -> Result<ObsConnectionTarget, CaptureError> {
    let trimmed = url.trim();
    let parsed = Url::parse(trimmed).map_err(|error| {
        CaptureError::SpawnFailed(format!("OBS websocket URL `{trimmed}` is invalid: {error}"))
    })?;

    let tls = match parsed.scheme() {
        "ws" => false,
        "wss" => true,
        other => {
            return Err(CaptureError::SpawnFailed(format!(
                "OBS websocket URL scheme `{other}` is unsupported. Use ws:// or wss://."
            )));
        }
    };

    let host = parsed.host_str().ok_or_else(|| {
        CaptureError::SpawnFailed(format!("OBS websocket URL `{trimmed}` is missing a host"))
    })?;
    let port = parsed.port_or_known_default().ok_or_else(|| {
        CaptureError::SpawnFailed(format!("OBS websocket URL `{trimmed}` is missing a port"))
    })?;

    if parsed.path() != "/" && !parsed.path().is_empty() {
        return Err(CaptureError::SpawnFailed(format!(
            "OBS websocket URL `{trimmed}` must not include a path"
        )));
    }

    if !is_loopback_host(host) {
        return Err(CaptureError::SpawnFailed(format!(
            "OBS websocket URL `{trimmed}` must point to a local OBS instance (host: {host}). \
             Remote OBS is not supported — the ReplayBufferSaved event returns a path on \
             the OBS machine, and NaniteClip cannot read files that live on another host."
        )));
    }

    Ok(ObsConnectionTarget {
        host: host.to_string(),
        port,
        tls,
    })
}

fn is_loopback_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    // url::Url::host_str() wraps IPv6 literals in brackets (e.g. `[::1]`);
    // strip them before feeding to IpAddr::parse.
    let unbracketed = host
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .unwrap_or(host);
    if let Ok(ip) = unbracketed.parse::<std::net::IpAddr>() {
        return ip.is_loopback();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{
        ManagedProfileSnapshot, ObsParameterKeys, backoff_for_attempt,
        ensure_supported_obs_version, managed_profile_matches_request, map_container,
        parse_websocket_url,
    };
    use obws::responses::general::Version as ObsVersionInfo;
    use semver::Version;
    use std::time::Duration;

    fn sample_obs_version() -> ObsVersionInfo {
        ObsVersionInfo {
            obs_studio_version: Version::new(30, 2, 0),
            obs_web_socket_version: Version::new(5, 5, 0),
            rpc_version: 1,
            available_requests: Vec::new(),
            supported_image_formats: Vec::new(),
            platform: "linux".into(),
            platform_description: "Linux".into(),
        }
    }

    #[test]
    fn obs_container_mapping_accepts_supported_formats() {
        assert_eq!(map_container("mkv").unwrap(), "mkv");
        assert_eq!(map_container("MP4").unwrap(), "mp4");
        assert_eq!(map_container("fragmented_mp4").unwrap(), "fragmented_mp4");
        assert_eq!(map_container("hls").unwrap(), "hls");
        assert_eq!(map_container("mpegts").unwrap(), "mpegts");
        assert!(map_container("webm").is_err());
    }

    #[test]
    fn obs_websocket_url_parses_localhost_ws_and_wss() {
        let ws = parse_websocket_url("ws://127.0.0.1:4455").unwrap();
        assert_eq!(ws.host, "127.0.0.1");
        assert_eq!(ws.port, 4455);
        assert!(!ws.tls);

        let localhost = parse_websocket_url("ws://localhost:4455").unwrap();
        assert_eq!(localhost.host, "localhost");

        let ipv6 = parse_websocket_url("wss://[::1]:8443").unwrap();
        assert_eq!(ipv6.host, "[::1]");
        assert!(ipv6.tls);
    }

    #[test]
    fn obs_websocket_url_rejects_remote_hosts() {
        assert!(parse_websocket_url("ws://obs.example.com:4455").is_err());
        assert!(parse_websocket_url("ws://10.0.0.5:4455").is_err());
        assert!(parse_websocket_url("wss://192.168.1.10:4455").is_err());
    }

    #[test]
    fn obs_parameter_keys_switch_rec_format_name_for_newer_obs() {
        let old = ObsParameterKeys::for_version(&Version::new(29, 1, 0));
        let new = ObsParameterKeys::for_version(&Version::new(30, 2, 0));

        assert_eq!(old.simple_output_rec_format, "RecFormat");
        assert_eq!(new.simple_output_rec_format, "RecFormat2");
    }

    #[test]
    fn reconnect_backoff_matches_documented_schedule() {
        assert_eq!(backoff_for_attempt(1), Duration::from_secs(1));
        assert_eq!(backoff_for_attempt(2), Duration::from_secs(2));
        assert_eq!(backoff_for_attempt(3), Duration::from_secs(5));
        assert_eq!(backoff_for_attempt(4), Duration::from_secs(10));
        assert_eq!(backoff_for_attempt(5), Duration::from_secs(30));
        assert_eq!(backoff_for_attempt(12), Duration::from_secs(30));
    }

    #[test]
    fn obs_version_guard_accepts_supported_stable_versions() {
        assert!(ensure_supported_obs_version(&sample_obs_version()).is_ok());
    }

    #[test]
    fn obs_version_guard_rejects_prerelease_studio_builds() {
        let mut version = sample_obs_version();
        version.obs_studio_version = Version::parse("30.2.0-rc1").unwrap();

        assert!(ensure_supported_obs_version(&version).is_err());
    }

    #[test]
    fn obs_version_guard_rejects_wrong_websocket_major() {
        let mut version = sample_obs_version();
        version.obs_web_socket_version = Version::new(6, 0, 0);

        assert!(ensure_supported_obs_version(&version).is_err());
    }

    #[test]
    fn managed_profile_match_requires_active_snapshot_with_expected_values() {
        let snapshot = ManagedProfileSnapshot {
            file_path: Some("C:\\Clips\\".into()),
            rec_format: Some("MKV".into()),
            rec_rb: Some("1".into()),
            rec_rb_time: Some("030".into()),
            rec_rb_size: Some("0512".into()),
            was_active: true,
        };

        assert!(managed_profile_matches_request(
            &snapshot, "C:/Clips", "mkv", "true", "30", "512",
        ));
    }

    #[test]
    fn managed_profile_match_rejects_mismatched_or_inactive_snapshots() {
        let inactive_snapshot = ManagedProfileSnapshot {
            file_path: Some("C:\\Clips".into()),
            rec_format: Some("mkv".into()),
            rec_rb: Some("true".into()),
            rec_rb_time: Some("30".into()),
            rec_rb_size: Some("512".into()),
            was_active: false,
        };
        assert!(!managed_profile_matches_request(
            &inactive_snapshot,
            "C:\\Clips",
            "mkv",
            "true",
            "30",
            "512",
        ));

        let mismatched_snapshot = ManagedProfileSnapshot {
            file_path: Some("C:\\Other".into()),
            rec_format: Some("mp4".into()),
            rec_rb: Some("false".into()),
            rec_rb_time: Some("15".into()),
            rec_rb_size: Some("256".into()),
            was_active: true,
        };
        assert!(!managed_profile_matches_request(
            &mismatched_snapshot,
            "C:\\Clips",
            "mkv",
            "true",
            "30",
            "512",
        ));
    }
}

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=native/platform_service/CMakeLists.txt");
    println!("cargo:rerun-if-changed=native/platform_service/main.cpp");
    println!("cargo:rerun-if-env-changed=NANITE_CLIP_SKIP_PLASMA_HELPER_BUILD");
    println!("cargo:rerun-if-env-changed=NANITE_CLIP_SKIP_PLATFORM_SERVICE_BUILD");

    let platform_service_path = match build_platform_service() {
        Ok(path) => path,
        Err(error) => {
            println!("cargo:warning={error}");
            String::new()
        }
    };

    println!("cargo:rustc-env=NANITE_CLIP_PLATFORM_SERVICE_PATH={platform_service_path}");
}

fn build_platform_service() -> Result<String, String> {
    if env::var_os("CARGO_CFG_TARGET_OS") != Some(OsString::from("linux")) {
        return Ok(String::new());
    }
    if env_flag_enabled("NANITE_CLIP_SKIP_PLATFORM_SERVICE_BUILD")
        || env_flag_enabled("NANITE_CLIP_SKIP_PLASMA_HELPER_BUILD")
    {
        return Ok(String::new());
    }

    let source_dir = Path::new("native/platform_service");
    if !source_dir.exists() {
        return Ok(String::new());
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").map_err(|error| error.to_string())?);
    let build_dir = out_dir.join("platform_service-build");
    let install_dir = out_dir.join("platform_service-install");
    fs::create_dir_all(&build_dir).map_err(|error| {
        format!(
            "failed to create the platform service build directory `{}`: {error}",
            build_dir.display()
        )
    })?;
    fs::create_dir_all(&install_dir).map_err(|error| {
        format!(
            "failed to create the platform service install directory `{}`: {error}",
            install_dir.display()
        )
    })?;

    run_cmake(
        Command::new("cmake")
            .arg("-S")
            .arg(source_dir)
            .arg("-B")
            .arg(&build_dir)
            .arg(format!("-DCMAKE_INSTALL_PREFIX={}", install_dir.display()))
            .arg("-DCMAKE_BUILD_TYPE=RelWithDebInfo"),
        "configure the platform service",
    )?;

    run_cmake(
        Command::new("cmake")
            .arg("--build")
            .arg(&build_dir)
            .arg("--target")
            .arg("install"),
        "build the platform service",
    )?;

    let platform_service_path = install_dir.join("bin").join("nanite-clip-platform-service");
    if !platform_service_path.is_file() {
        return Err(format!(
            "the platform service build completed, but `{}` was not produced",
            platform_service_path.display()
        ));
    }

    Ok(platform_service_path.display().to_string())
}

fn run_cmake(command: &mut Command, description: &str) -> Result<(), String> {
    let output = command
        .output()
        .map_err(|error| format!("failed to {description}: could not start `cmake`: {error}"))?;
    if output.status.success() {
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let mut details = String::new();
    if !stdout.trim().is_empty() {
        details.push_str("\nstdout:\n");
        details.push_str(stdout.trim());
    }
    if !stderr.trim().is_empty() {
        details.push_str("\nstderr:\n");
        details.push_str(stderr.trim());
    }

    Err(format!(
        "failed to {description} (exit status: {}){details}",
        output.status
    ))
}

fn env_flag_enabled(name: &str) -> bool {
    matches!(
        env::var(name).ok().as_deref().map(str::trim),
        Some("1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON")
    )
}

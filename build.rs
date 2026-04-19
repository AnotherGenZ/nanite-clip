use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/NaniteClips-512.png");
    println!("cargo:rerun-if-changed=native/platform_service/CMakeLists.txt");
    println!("cargo:rerun-if-changed=native/platform_service/main.cpp");
    println!("cargo:rerun-if-env-changed=NANITE_CLIP_SKIP_PLASMA_HELPER_BUILD");
    println!("cargo:rerun-if-env-changed=NANITE_CLIP_SKIP_PLATFORM_SERVICE_BUILD");
    println!("cargo:rerun-if-env-changed=NANITE_CLIP_UPDATE_PUBLIC_KEY");
    println!("cargo:rerun-if-env-changed=NANITE_CLIP_UPDATE_PUBLIC_KEYS");

    let update_public_key = env::var("NANITE_CLIP_UPDATE_PUBLIC_KEY").unwrap_or_default();
    let update_public_keys = env::var("NANITE_CLIP_UPDATE_PUBLIC_KEYS").unwrap_or_default();
    println!("cargo:rustc-env=NANITE_CLIP_UPDATE_PUBLIC_KEY={update_public_key}");
    println!("cargo:rustc-env=NANITE_CLIP_UPDATE_PUBLIC_KEYS={update_public_keys}");

    if let Err(error) = embed_windows_resources() {
        println!("cargo:warning={error}");
    }

    let platform_service_path = match build_platform_service() {
        Ok(path) => path,
        Err(error) => {
            println!("cargo:warning={error}");
            String::new()
        }
    };

    println!("cargo:rustc-env=NANITE_CLIP_PLATFORM_SERVICE_PATH={platform_service_path}");
}

fn embed_windows_resources() -> Result<(), String> {
    if env::var_os("CARGO_CFG_TARGET_OS") != Some(OsString::from("windows")) {
        return Ok(());
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").map_err(|error| error.to_string())?);
    let icon_path = out_dir.join("nanite-clip.ico");
    let rc_path = out_dir.join("nanite-clip.rc");
    let res_path = out_dir.join("nanite-clip.res");

    write_windows_icon(&icon_path)?;
    fs::write(
        &rc_path,
        format!(
            "IDI_APP_ICON ICON \"{}\"\r\n",
            windows_resource_literal(&icon_path)
        ),
    )
    .map_err(|error| format!("failed to write `{}`: {error}", rc_path.display()))?;

    let output = Command::new("rc.exe")
        .arg("/nologo")
        .arg(format!("/fo{}", res_path.display()))
        .arg(&rc_path)
        .output()
        .map_err(|error| {
            format!(
                "failed to compile Windows resources with rc.exe: {error}. \
                 Install the Visual Studio C++ build tools so release builds can embed the app icon."
            )
        })?;
    if !output.status.success() {
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
        return Err(format!(
            "rc.exe failed while compiling `{}` (exit status: {}){details}",
            rc_path.display(),
            output.status
        ));
    }

    println!(
        "cargo:rustc-link-arg-bin=nanite-clip={}",
        res_path.display()
    );
    println!(
        "cargo:rustc-link-arg-bin=nanite-clip-updater={}",
        res_path.display()
    );

    Ok(())
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

fn write_windows_icon(path: &Path) -> Result<(), String> {
    let image = image::open("assets/NaniteClips-512.png")
        .map_err(|error| format!("failed to load Windows app icon asset: {error}"))?;
    let icon = image.resize_exact(256, 256, image::imageops::FilterType::Lanczos3);
    icon.save_with_format(path, image::ImageFormat::Ico)
        .map_err(|error| format!("failed to write Windows icon `{}`: {error}", path.display()))
}

fn windows_resource_literal(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

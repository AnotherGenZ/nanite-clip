#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

#[path = "../update/helper_runner.rs"]
mod helper_runner;
#[path = "../update/helper_shared.rs"]
mod helper_shared;

fn main() {
    if let Err(error) = run() {
        eprintln!("NaniteClip updater error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = std::env::args_os();
    let _ = args.next();
    match (args.next(), args.next()) {
        (Some(flag), Some(plan_path)) if flag == "--apply-plan" => {
            helper_runner::run_apply_plan(std::path::Path::new(&plan_path))
        }
        _ => Err("usage: nanite-clip-updater --apply-plan <path>".into()),
    }
}

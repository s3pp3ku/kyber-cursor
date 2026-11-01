#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(target_os = "windows")]
mod windows;

fn main() {
    #[cfg(target_os = "windows")]
    if let Err(error) = windows::run() {
        if std::env::args_os().any(|a| a == "--self-check") {
            eprintln!("{error}");
        } else {
            windows::show_error(&error);
        }
        std::process::exit(1);
    }
    #[cfg(not(target_os = "windows"))]
    {
        eprintln!("kyber-preview is a Windows-only, window-scoped cursor trial.");
        std::process::exit(3);
    }
}

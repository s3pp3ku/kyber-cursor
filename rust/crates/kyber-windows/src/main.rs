#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(target_os = "windows")]
mod app;
#[cfg(target_os = "windows")]
mod cursor;
#[cfg(target_os = "windows")]
mod guardian;
#[cfg(target_os = "windows")]
mod layer;

#[cfg(target_os = "windows")]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
#[cfg(target_os = "windows")]
fn failed(action: &str) -> String {
    format!("{action}: {}", std::io::Error::last_os_error())
}

fn main() {
    #[cfg(target_os = "windows")]
    {
        let result = if std::env::args_os()
            .nth(1)
            .is_some_and(|a| a == "--watch-parent")
        {
            guardian::watch()
        } else {
            app::run()
        };
        if let Err(error) = result {
            // A guardian must never wait in a dialog while recovery is needed.
            if !std::env::args_os().any(|a| a == "--watch-parent") {
                app::show_error(&error);
            }
            std::process::exit(1);
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        eprintln!("kyber-cursor requires Windows 10/11 x64. Use kyber-ignite on Linux.");
        std::process::exit(3);
    }
}

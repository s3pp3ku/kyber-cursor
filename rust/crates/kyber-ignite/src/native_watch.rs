//! Render recolors outside the compositor and publish complete frame banks atomically.
use std::path::{Path, PathBuf};

fn selected_color(config: &Path, home: &Path) -> Option<String> {
    let mode = std::fs::read_to_string(config.join("color-mode")).ok()?;
    let path = match mode.trim() {
        "custom" => config.join("color"),
        "random" => config.join("random-color"),
        _ => home.join(".local/share/icons/Omarchy-Kyber/.accent"),
    };
    let color = std::fs::read_to_string(path).ok()?.trim().to_lowercase();
    (color.len() == 7 && color.starts_with('#') && kyber_core::color::is_valid_hex(&color))
        .then_some(color)
}

pub fn publish(out: &Path, color: &str) -> Result<(), String> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let name = format!("generation-{}-{stamp}", std::process::id());
    let generation = out.join(&name);
    kyber_core::native::export(&generation, color, 48, 2)?;
    std::fs::write(generation.join("color.txt"), color).map_err(|e| e.to_string())?;
    // The compositor resolves this link once per load. It never reads a half-built bank.
    let next = out.join(format!("current-next-{}-{stamp}", std::process::id()));
    std::os::unix::fs::symlink(&name, &next).map_err(|e| e.to_string())?;
    std::fs::rename(&next, out.join("current")).map_err(|e| e.to_string())?;
    eprintln!("kyber-native: published {color}");
    Ok(())
}

pub fn prepare(out: &Path) -> Result<(), String> {
    let home = kyber_core::paths::home_dir()?;
    let config = kyber_core::paths::settings_dir()?;
    let color = selected_color(&config, &home).ok_or("Cannot resolve the selected cursor color")?;
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    if std::fs::read_to_string(out.join("current/color.txt"))
        .ok()
        .as_deref()
        != Some(&color)
    {
        publish(out, &color)?;
    }
    Ok(())
}

pub fn run(out: &Path) -> Result<(), String> {
    let home = PathBuf::from(std::env::var("HOME").map_err(|e| e.to_string())?);
    let config = kyber_core::paths::settings_dir()?;
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let mut current = std::fs::read_to_string(out.join("current/color.txt")).unwrap_or_default();
    loop {
        if let Some(color) = selected_color(&config, &home) {
            if color != current {
                publish(out, &color)?;
                current = color;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn publisher_exposes_a_complete_generation() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let out = std::env::temp_dir().join(format!(
            "kyber-native-publish-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&out).unwrap();
        super::publish(&out, "#1a05f5").unwrap();
        let old = std::fs::read_link(out.join("current")).unwrap();
        assert!(out.join("current/light-16.png").is_file());
        assert!(out.join("current/light-hold-23.png").is_file());
        assert!(out.join("current/dark-hold-23.png").is_file());
        super::publish(&out, "#f51705").unwrap();
        assert_ne!(old, std::fs::read_link(out.join("current")).unwrap());
        assert!(out.join(old).join("dark-16.png").is_file());
        std::fs::remove_dir_all(out).unwrap();
    }
}

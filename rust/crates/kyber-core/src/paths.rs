//! Linux settings locations shared by the CLI and the native runtime.
use std::path::PathBuf;

pub fn home_dir() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| "cannot locate your home folder".into())
}

pub fn settings_dir() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
    {
        return Ok(path.join("kyber-cursor"));
    }
    Ok(home_dir()?.join(".config/kyber-cursor"))
}

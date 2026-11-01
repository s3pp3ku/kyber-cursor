//! Export the Windows desktop palette with the unchanged shared native renderer.
use std::path::PathBuf;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("usage: windows_frames OUTPUT")?,
    );
    let colors = [
        ("current-green", "#07d815"),
        ("teal", "#24d9c5"),
        ("violet", "#b36bff"),
        ("amber", "#ffb938"),
        ("ice-blue", "#42a5ff"),
        ("coral", "#ff5d68"),
        ("deep-teal", "#156878"),
        ("white", "#ffffff"),
        ("black", "#000000"),
    ];
    for (name, color) in colors {
        kyber_core::native::export(&root.join(name), color, 48, 2)?;
    }
    std::fs::write(
        root.join("colors.json"),
        serde_json::to_vec_pretty(&colors)?,
    )?;
    println!("{}", root.display());
    Ok(())
}

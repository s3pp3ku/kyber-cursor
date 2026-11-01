//! Export the portable native trail renderer without touching the desktop.
use kyber_core::trail::{Bounds, Trail};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("supply output directory")?,
    );
    std::fs::create_dir_all(&out)?;
    for color in ["#4efade", "#080bf9", "#ff0000", "#e4832a", "#1a8297", "#ffffff", "#000000"] {
        for light in [false, true] {
            let mut trail = Trail::default();
            assert!(trail.set_color(color));
            for i in 0..60 {
                let t = i as f64 / 100.0;
                trail.sample(t, 40.0 + t * 540.0, 100.0 + (t * 7.0).sin() * 45.0);
            }
            let view = Bounds {
                x: 0.0,
                y: 0.0,
                width: 400.0,
                height: 200.0,
            };
            // just after the swing, then with the path gone and sparks cooling
            for (now, when) in [(0.60, "now"), (0.95, "later")] {
                if let Some(image) = trail.render(now, view, light, 2.0) {
                    image.pixels.save_png(out.join(format!(
                        "{}-{}-{when}.png",
                        &color[1..],
                        if light { "light" } else { "dark" }
                    )))?;
                }
            }
        }
    }
    println!("Exported trail-only samples into {}", out.display());
    Ok(())
}

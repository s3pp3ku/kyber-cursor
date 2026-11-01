//! Portable visual regression sheet. No desktop access or settings changes.
//! cargo run --release -p kyber-core --example glow_review -- OUTPUT
use image::{imageops, Rgba, RgbaImage};
use kyber_core::{color, config::SmallRng};
use std::path::Path;

fn random_colors() -> Vec<(String, String)> {
    let mut colors: Vec<_> = [
        ("current-teal", "#156878"),
        ("blue", "#0316ff"),
        ("black", "#000000"),
        ("white", "#ffffff"),
        ("near-white", "#f5f4ee"),
        ("gray", "#808080"),
        ("red", "#ff0000"),
        ("green", "#00ff00"),
        ("yellow", "#ffff00"),
        ("magenta", "#ff00ff"),
    ]
    .into_iter()
    .map(|(n, c)| (n.to_owned(), c.to_owned()))
    .collect();
    let mut rng = SmallRng::seed(0x2026_1016_5ade);
    for hue in 0..6 {
        for (brightness, lo, hi) in [
            ("dim", 0.02, 0.18),
            ("mid", 0.30, 0.60),
            ("bright", 0.82, 1.0),
        ] {
            for (shade, sl, sh) in [
                ("muted", 0.0, 0.15),
                ("soft", 0.30, 0.65),
                ("vivid", 0.85, 1.0),
            ] {
                let h = (hue as f64 + rng.next_f64()) / 6.0;
                let s = rng.range(sl, sh);
                let v = rng.range(lo, hi);
                let i = (h * 6.0).floor() as usize;
                let f = h * 6.0 - i as f64;
                let (p, q, t) = (v * (1.0 - s), v * (1.0 - f * s), v * (1.0 - (1.0 - f) * s));
                let (r, g, b) = [
                    (v, t, p),
                    (q, v, p),
                    (p, v, t),
                    (p, q, v),
                    (t, p, v),
                    (v, p, q),
                ][i];
                let hex = color::to_hex([
                    (r * 255.0).round() as u8,
                    (g * 255.0).round() as u8,
                    (b * 255.0).round() as u8,
                ]);
                colors.push((format!("h{hue}-{brightness}-{shade}"), hex));
            }
        }
    }
    colors
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args().nth(1).ok_or("usage: glow_review OUTPUT")?;
    let out = Path::new(&out);
    std::fs::create_dir_all(out)?;
    let report_only = std::env::args().any(|a| a == "--report-only");
    let colors: Vec<(String, String)> = if std::env::args().any(|a| a == "--random") {
        random_colors()
    } else {
        [
            ("blue", "#0316ff"),
            ("red", "#f51705"),
            ("cyan", "#00e5ff"),
            ("green", "#56ff62"),
            ("gold", "#ffd700"),
            ("purple", "#a020f0"),
            ("white", "#ffffff"),
            ("near-black", "#000010"),
        ]
        .into_iter()
        .map(|(n, c)| (n.into(), c.into()))
        .collect()
    };
    let backgrounds = [
        ("white", "light", [255, 255, 255, 255]),
        ("warm-white", "light", [245, 240, 225, 255]),
        ("light-gray", "light", [200, 200, 200, 255]),
        ("dark-gray", "dark", [48, 48, 52, 255]),
        ("dark", "dark", [18, 18, 22, 255]),
        ("black", "dark", [0, 0, 0, 255]),
    ];
    let side = 206;
    let mut sheets: Vec<_> = colors
        .chunks(8)
        .map(|chunk| RgbaImage::new(side * chunk.len() as u32, side * backgrounds.len() as u32))
        .collect();
    let mut html = String::from("<!doctype html><meta charset=utf-8><title>Kyber glow review</title><style>body{font:16px system-ui;background:#222;color:#eee}table{border-collapse:collapse}th{padding:8px}td{padding:0}img{display:block;width:206px;height:206px}section{overflow:auto;margin-bottom:3em}a{color:#9cf}</style><h1>Kyber glow review</h1><p>Rust renderer, full ignition. Each row uses its matching background mode. Each color has 17 ignition frames and 24 held shimmer phases per mode; click a color for its ignition stages.</p>");
    for (col, (name, color)) in colors.iter().enumerate() {
        let dir = out.join(name);
        if !report_only {
            kyber_core::native::export(&dir, color, 48, 2)?;
        }
        for (row, (bg_name, bank, bg)) in backgrounds.iter().enumerate() {
            let frame = image::open(dir.join(format!("{bank}-16.png")))?.to_rgba8();
            let mut tile = RgbaImage::from_pixel(side, side, Rgba(*bg));
            imageops::overlay(&mut tile, &frame, 0, 0);
            tile.save(out.join(format!("{name}-{bg_name}.png")))?;
            imageops::overlay(
                &mut sheets[col / 8],
                &tile,
                (col % 8) as i64 * side as i64,
                row as i64 * side as i64,
            );
        }
    }
    for (page, chunk) in colors.chunks(8).enumerate() {
        html.push_str(&format!(
            "<section><h2>Group {}</h2><table><tr><th>Background</th>",
            page + 1
        ));
        for (name, color) in chunk {
            html.push_str(&format!(
                "<th><a href='{name}/preview.png'>{name}</a><br>{color}</th>"
            ));
        }
        html.push_str("</tr>");
        for (bg_name, _, _) in backgrounds {
            html.push_str(&format!("<tr><th>{bg_name}</th>"));
            for (name, _) in chunk {
                html.push_str(&format!("<td><img loading='lazy' src='{name}-{bg_name}.png' alt='{name} on {bg_name}'></td>"));
            }
            html.push_str("</tr>");
        }
        html.push_str("</table></section>");
    }
    html.push_str("<p><a href='portable.html'>Animated portable test</a></p>");
    std::fs::write(out.join("index.html"), html)?;
    for (page, sheet) in sheets.iter().enumerate() {
        let filename = if page == 0 {
            "matrix.png".into()
        } else {
            format!("matrix-{}.png", page + 1)
        };
        sheet.save(out.join(filename))?;
    }
    let palette = serde_json::to_string_pretty(&colors)?;
    std::fs::write(out.join("colors.json"), &palette)?;
    std::fs::write(
        out.join("portable.html"),
        include_str!("portable_review.html").replace("__KYBER_PALETTE__", &palette),
    )?;
    std::fs::write(
        out.join("TESTING.txt"),
        include_str!("portable_testing.txt"),
    )?;
    println!("{}", out.join("portable.html").display());
    Ok(())
}

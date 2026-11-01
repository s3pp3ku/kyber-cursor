//! Validate the actual packaged artwork through Windows pixel preparation.
use kyber_preview::{load_frame, palette, CursorPixels, Layout};
use std::path::PathBuf;

fn main() -> Result<(), String> {
    let root = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("usage: check_frames FRAME_FOLDER")?,
    );
    let colors = palette(&root)?;
    let mut checked = 0;
    for (name, _) in &colors {
        let directory = root.join(name);
        let layout = Layout::parse(
            &std::fs::read_to_string(directory.join("frames.txt")).map_err(|e| e.to_string())?,
        )?;
        for bank in ["light", "dark"] {
            for file in layout.file_names(bank) {
                let image = load_frame(&directory.join(&file), layout)?;
                for dpi in [96, 120, 192] {
                    let pixels = CursorPixels::new(&image, layout, dpi)?;
                    let mut alpha_sum = 0u64;
                    for (index, pixel) in pixels.bgra.chunks_exact(4).enumerate() {
                        if pixel[..3].iter().any(|channel| *channel > pixel[3]) {
                            return Err(format!(
                                "Invalid premultiplication in {name}/{file} at DPI {dpi}"
                            ));
                        }
                        let x = index as u32 % pixels.size[0];
                        let y = index as u32 / pixels.size[0];
                        if (x == 0 || y == 0 || x + 1 == pixels.size[0] || y + 1 == pixels.size[1])
                            && pixel[3] > 1
                        {
                            return Err(format!("Clipped halo edge in {name}/{file} at DPI {dpi}"));
                        }
                        let mask = pixels.mask[y as usize * pixels.mask_stride + x as usize / 8]
                            & (0x80 >> (x % 8));
                        if (mask != 0) != (pixel[3] == 0) {
                            return Err(format!(
                                "Incorrect AND mask in {name}/{file} at DPI {dpi}"
                            ));
                        }
                        alpha_sum += u64::from(pixel[3]);
                    }
                    if alpha_sum == 0 {
                        return Err(format!("Empty frame in {name}/{file} at DPI {dpi}"));
                    }
                    checked += 1;
                }
            }
        }
    }
    println!(
        "{}",
        serde_json::json!({"colors":colors.len(),"dpi":[96,120,192],"frames_checked":checked,
        "checks":["dimensions","hotspot scaling","premultiplied BGRA","AND mask","transparent padding","nonempty blade"],
        "native_windows_execution":false})
    );
    Ok(())
}

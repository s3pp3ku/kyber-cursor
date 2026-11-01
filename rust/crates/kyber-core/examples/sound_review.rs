//! Export the current Rust synthesizers for an offline listening comparison.
//! No audio device, desktop connection, or settings access.
use kyber_core::{
    blade_audio::HoldingHum,
    sounds::{self, SR},
};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("usage: sound_review OUTPUT")?,
    );
    // Requiring a new directory keeps a comparison from silently changing.
    std::fs::create_dir(&out)?;
    sounds::build_pack(&out)?;
    for pitch in [78, 86, 94] {
        let samples: Vec<_> = HoldingHum::new(pitch as f64, SR)
            .take(SR as usize * 6)
            .collect();
        sounds::save_wav(&out.join(format!("hum-{pitch}.wav")), &samples)?;
    }
    for (index, speed) in [900.0, 2200.0, 4500.0].into_iter().enumerate() {
        sounds::save_wav(
            &out.join(format!("swipe-{}.wav", index + 1)),
            &sounds::swipe(61 + index as u64, speed),
        )?;
    }
    println!("Exported generated Rust sounds to {}", out.display());
    Ok(())
}

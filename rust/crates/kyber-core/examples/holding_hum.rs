//! Audio-only parity fixture for Python's original hum generator.
use kyber_core::{
    blade_audio::HoldingHum,
    sounds::{save_wav, SR},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("supply output directory")?,
    );
    for pitch in [78, 86, 94] {
        let samples: Vec<_> = HoldingHum::new(pitch as f64, SR)
            .take(SR as usize * 12)
            .collect();
        save_wav(&out.join(format!("hum-{pitch}.wav")), &samples)?;
    }
    Ok(())
}

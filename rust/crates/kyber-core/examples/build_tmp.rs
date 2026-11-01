//! Build both cursor themes into a directory. Verification helper only:
//! `cargo run --release --example build_tmp -- /tmp/rs-theme '#c15db1'`
//! Touches nothing outside the given output dir.

fn main() {
    let out = std::env::args()
        .nth(1)
        .expect("usage: build_tmp OUT_DIR ACCENT");
    let accent = std::env::args()
        .nth(2)
        .expect("usage: build_tmp OUT_DIR ACCENT");
    let out = std::path::Path::new(&out);
    kyber_core::theme::build_all(&accent, &accent, out).expect("build_all");
    println!("built {out:?} with accent {accent}");
}

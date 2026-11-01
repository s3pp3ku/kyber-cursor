# Windows native cursor trial

This older visual tool remains available for comparisons. Current Windows
packages use the [desktop app](../kyber-windows/README.md), with system cursor
replacement, tray controls and recovery. Its native Windows drive is pending.

This is the first Windows backend experiment, written in Rust using Win32.
The blade and halo are one native cursor bitmap, so Windows moves them together.
The custom cursor is limited to the test window. It uses the accepted shared
Rust frame exports; it does not change cursor settings or install a helper.
The preview uses the shared embedded private recordings for power-up, held hum
and power-down through the Windows default audio device. Local recordings can
override each role. No files from Downloads or extra audio tools are needed. Movement trails use the shared
Kyber renderer while the button is held, then fade over 600 ms. Desktop-wide
activation and a tray UI are still absent. macOS has no native backend.

Extract the complete test ZIP on a Windows 11 x64 PC and open
`kyber-preview.exe`. Hold the left mouse button while moving inside its white
and dark areas. The cursor ignites in 150 ms, loops the held halo, and retracts
in 110 ms. Leaving the client area or losing focus cancels the glow immediately.

- Space / Left / Right: select another color.
- B: split, white, or dark background.
- V: matching background, fixed light glow, or fixed dark glow.
- S: cycle all sounds, power-up only, or mute.
- Escape: close. The normal Windows pointer is used outside the test window.

Move while holding the button to draw the trail. It fades after release and
clears immediately when leaving the window or switching apps.

To use your own recordings, put supported `.wav`, `.mp3`, `.ogg`, `.oga`, or
`.flac` files in `%APPDATA%\kyber-cursor\sounds\ignite` and
`%APPDATA%\kyber-cursor\sounds\off`. Use the matching `sounds\hum` folder for a recorded hold override. The shared
pack is the fallback for every role. The held recording loops with a short
crossfade and releases gently. The former generated defaults are ignored.
Recordings have separate provenance in the package audio notice.

The source frame bank has 206×206 pixels, hotspot (72,72), and scale 2. At 100%
Windows display scaling this becomes a 103×103 native cursor canvas with hotspot
(36,36), matching the portable preview's reference size. The actual blade is
much smaller than the padded halo canvas. Per-monitor DPI changes rebuild the
current color's cursors at the new size. A cursor over 256 physical pixels is
outside this first trial's supported range and produces an explanatory error.
Mixed-monitor behavior still needs native interactive testing.

## Build and package on Windows

With Rust and its normal Windows build prerequisites already available, run
from `rust/`:

```powershell
powershell -File scripts/build-windows-preview.ps1 -SelfCheck
```

The script tests core/CLI/preview, builds the executable, exports eight colors,
and packages the executable, frames, audio manifest and recording notice. The
selected recordings are embedded in the executable. It downloads no installers and changes
no desktop settings. Its optional self-check creates and destroys every native
cursor at 100% and 200% scaling and verifies dimensions and hotspots. It does
not test pointer movement, button events, visual quality, or focus transitions.

For an existing export, use `kyber-preview.exe --frames PATH`. The folder must
contain `colors.json` and named frame-bank subfolders. By default the executable
reads `frames/` beside itself, independent of the working directory.

Automated native check:

```powershell
$process = Start-Process .\kyber-preview.exe -ArgumentList '--self-check', '--report', 'self-check.json' -Wait -PassThru
if ($process.ExitCode -ne 0) { throw 'Native cursor creation check failed' }
```

## Design references

Cursor creation follows Microsoft's [alpha cursor documentation](https://learn.microsoft.com/en-us/windows/win32/menurc/using-cursors),
with premultiplied BGRA pixels and a separate transparency mask. Application
cursor selection follows [WM_SETCURSOR](https://learn.microsoft.com/en-us/windows/win32/learnwin32/setting-the-cursor-image).
Bitmap/cursor handles are owned and released; the selected custom cursor is
restored before its handles are dropped. Window callbacks use a checked
`RefCell` borrow because Win32 calls can reenter the window procedure.

Pure Rust tests cover the frame manifest, DPI/hotspot conversion, alpha/channel
packing and mask alignment, halo resampling without transparent-color bleed,
animation reversal, held loop wrap, cancellation,
and repeated presses. These can run on Linux while the Win32 code is checked
against an actual Windows target. Cross-compilation is not Windows runtime
validation.

The packaged frames can also be checked through the Windows pixel preparation
at 100%, 125%, and 200% scaling without a Windows desktop:

```sh
cargo run --release -p kyber-preview --example check_frames -- FRAME_FOLDER
```

For a Linux cross-build, use the `x86_64-pc-windows-gnullvm` Rust target with
[LLVM-MinGW](https://github.com/mstorsjo/llvm-mingw) as its linker, as described
in the [Rust target documentation](https://doc.rust-lang.org/rustc/platform-support/windows-gnullvm.html).
The local trial uses Rust 1.89.0 and LLVM-MinGW 20261006. Set
`CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER` to the toolchain's
`x86_64-w64-mingw32-clang` and set `RUSTFLAGS` to
`-C target-feature=+crt-static -C link-arg=-static` before the release build.
The static CRT feature keeps `libunwind.dll` out of the executable's imports;
inspect the imports before packaging rather than assuming the linker flag
alone makes the trial self-contained.

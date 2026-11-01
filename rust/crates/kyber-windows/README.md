# Windows desktop app

`kyber-cursor.exe` replaces the standard Windows cursors across the ordinary
desktop. It has no demo window. This is the next Windows package after the
approved `kyber-preview` visual trial. **The desktop backend is cross-built;
native Windows interaction and listening approval are still pending.**

See [READ-ME.txt](READ-ME.txt) for launching, controls, recovery and first-drive
instructions. The portable x64 package needs no administrator access or runtime
developer tools. It carries the accepted frame bank and the same eleven embedded
recordings as Linux. No Linux installation is changed by building it.

## Implementation

- Native GDI alpha cursors hold the complete body and glow. Windows moves them
  with the real pointer; the app animates the image only. All 13 documented
  standard system cursor slots receive Kyber role shapes while idle, and the
  accepted blade frames during a hold. Apps using private cursor images can
  override these. Copies passed to `SetSystemCursor` are consumed by Windows;
  cached animation handles remain owned by Kyber.
- Background mouse Raw Input supplies button transitions without suppressing
  clicks or installing hooks. Current mouse-button polling repairs missed
  transitions and supports devices that do not deliver Raw Input. No keyboard
  registration, click logging, network calls or elevated permissions.
- A layered, transparent, nonactivating window draws only the shared fading
  trail. It passes hit tests through and never draws another cursor head.
- Per-monitor DPI banks use premultiplied resampling and the original hotspot.
  Banks are warmed for attached monitors. The padded bitmap stops at 256px;
  larger display scaling uses the same capped scale without cropping the halo.
- The tray provides pause, nine colors plus the Windows color picker, two glow
  banks, trail, sound modes, volume and exit. Custom colors use the shared Rust
  renderer and a local cache. Settings are `%APPDATA%/kyber-cursor/windows.json`.
- Recorded power-up after 300 ms; crossfaded recorded hum; off on release;
  double-click clashes and speed-triggered swipes with hum ducking. The common
  `kyber-preview::audio::Player` now hosts playback for both Windows apps.
- Normal exit/pause reloads the saved Windows cursor scheme, preserving animated
  busy cursors. Original image copies are a fallback. Before changing cursors,
  a separate process opens the parent process and signals readiness. It waits
  for normal disarm or parent death; parent death reloads the saved scheme.
  The parent closes if its recovery process disappears. Lock/disconnect and
  inaccessible input desktops suspend effects; a fresh release arms input again.
- A session-local mutex prevents two desktop instances. Relaunch opens the tray
  menu. `--restore` closes a responsive instance and reloads the persisted scheme.

## Build and package

On a Windows developer machine with Rust and the Visual C++ build tools:

```powershell
./rust/scripts/build-windows-desktop.ps1
```

This builds the x64 MSVC application and exports the current palette with the
shared native renderer. It packages locally and does not launch the desktop app.
For an existing cross-built executable and accepted frame archive on Linux:

```sh
python3 rust/scripts/package_windows_desktop.py test-artifacts/windows-desktop-NEW \
  --frames-archive test-artifacts/kyber-windows-preview-glow6-audio-hum.zip
```

No tests are run by either packaging script. The Linux cross-build uses Rust
1.89, target `x86_64-pc-windows-gnullvm`, and the existing LLVM-MinGW toolchain,
with `-C target-feature=+crt-static -C link-arg=-static`. Imported libraries are
Windows system DLLs; no MinGW runtime DLL is needed. Code is MIT; recordings
carry their separate notice and source manifest.

API contracts: [system cursors](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setsystemcursor),
[Raw Input](https://learn.microsoft.com/en-us/windows/win32/inputdev/using-raw-input),
[layered windows](https://learn.microsoft.com/en-us/windows/win32/winmsg/window-features),
[tray controls](https://learn.microsoft.com/en-us/windows/win32/shell/notification-area).

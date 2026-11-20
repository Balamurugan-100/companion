# Packaging and offline verification

## Build

```sh
cargo build --release
```

The binary is written to `target/release/companion` (or
`target/release/companion.exe` on Windows, which is not a target). Build on each
target OS; cross-compiling is not required.

Bevy resolves assets relative to `BEVY_ASSET_ROOT`, then `CARGO_MANIFEST_DIR`,
then the directory containing the executable. A release install must therefore
ship `assets/character.glb` next to the binary:

```text
companion/
├── companion          (macOS/Linux executable)
└── assets/
    └── character.glb
```

The application never downloads the asset. Include `ATTRIBUTION.md` in every
package; the model is CC0 and its rigging/animation and glTF conversion are
CC BY 4.0 with the credits listed there.

## macOS

Create a `.app` bundle with `Contents/MacOS/companion` and
`Contents/Resources/assets/character.glb`, then wrap it in a `.dmg` if desired.
Set `BEVY_ASSET_ROOT` to `Contents/Resources` if the executable is not placed
directly beside `assets/`. Code signing and notarization are out of scope for
v1 and require no account or key.

Debug builds may print a non-fatal `ld: __eh_frame section too large ...` warning
on macOS; the release build on the development host was clean. Either way,
confirm a working artifact by launching it.

## Linux

Build on a machine with the Bevy system dependencies:

```sh
sudo apt-get install g++ pkg-config libx11-dev libasound2-dev libudev-dev libxkbcommon-x11-0
# Optional, for the Wayland fallback:
sudo apt-get install libwayland-dev libxkbcommon-dev
```

The tray icon uses the `ksni` backend, so no GTK development libraries are
needed, but a D-Bus session bus must be present at runtime for the tray icon to
appear.

For an AppImage or `.deb`, install the executable and `assets/character.glb`
together in one directory (for example `/usr/share/companion` with a launcher
that sets `BEVY_ASSET_ROOT`) and include `ATTRIBUTION.md`.

X11 is the primary Linux target. If `WAYLAND_DISPLAY` is set, the Wayland
fallback in `PLATFORMS.md` applies.

## Offline / no-keys verification

1. Build and launch with networking disabled (or in a network namespace with no
   interface other than loopback).
2. Confirm every feature works: tray, hotkey, dialogue, focus, tasks, notes,
   breathing, grounding, and daily summary.
3. Confirm no credential or API key is required at startup or in settings.
4. Confirm the source contains no networking dependencies:

```sh
rg -n 'reqwest|ureq|http|tcp|api[_-]?key|token|secret|webbrowser|open_url' src/
```

The command should produce no matches. Persistence is a single local redb file
under the OS-specific data directory.

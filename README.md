# Companion

An always-visible desktop companion with focus, breathing, grounding, mood,
tasks, notes, and optional opt-in GitHub settings sync.

## Run

Install the launcher once:

```bash
./scripts/install.sh
```

Then start the companion from anywhere:

```bash
companion
```

The installer builds the release binary, copies it to
`~/.local/bin/companion-bin`, and writes a small `~/.local/bin/companion`
launcher. The launcher sets `BEVY_ASSET_ROOT` so the bundled character asset is
found no matter which directory you run from.

Useful commands:

```bash
companion --help
companion --version
```

Hide/show with `Ctrl+Shift+C` or the tray icon. The tray menu and a single click
on the character open the setup panel.

## Development

```bash
cargo fmt --all -- --check
cargo test
cargo check
cargo build --release
```

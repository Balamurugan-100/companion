#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bin_dir="${COMPANION_BIN_DIR:-$HOME/.local/bin}"

cargo build --release --manifest-path "$repo_root/Cargo.toml"
mkdir -p "$bin_dir"

# Keep the real binary and the launcher separate. Writing the launcher through
# an old symlink would overwrite the binary itself and create recursive execs.
rm -f "$bin_dir/companion"
install -m 755 "$repo_root/target/release/companion" "$bin_dir/companion-bin"

cat > "$bin_dir/companion" <<EOF
#!/usr/bin/env bash
set -euo pipefail

# Bevy resolves assets relative to the executable unless BEVY_ASSET_ROOT is
# set. Point it at the repository so the installed binary finds assets/character.glb.
cd "$repo_root"
export BEVY_ASSET_ROOT="$repo_root"

case "\${1:-}" in
  -h|--help)
    echo "Usage: companion"
    echo "Starts the desktop companion. Use the tray icon or Ctrl+Shift+C to hide/show it."
    exit 0
    ;;
  -V|--version)
    echo "companion 0.1.0"
    exit 0
    ;;
esac

exec "$bin_dir/companion-bin" "\$@"
EOF

chmod +x "$bin_dir/companion"

echo "Installed binary: $bin_dir/companion-bin"
echo "Installed launcher: $bin_dir/companion"
echo "Run it with: companion"

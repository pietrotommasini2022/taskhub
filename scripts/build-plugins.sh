#!/usr/bin/env bash
# Build all WASM plugins and install them to ~/.taskhub/plugins/
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
PLUGINS_SRC="$REPO_ROOT/plugins"
INSTALL_DIR="$HOME/.taskhub/plugins"

PLUGINS=(weather github gitlab slack discord rss linear notion calendar_caldav s3 llm)

for plugin in "${PLUGINS[@]}"; do
    src="$PLUGINS_SRC/$plugin"
    if [ ! -f "$src/Cargo.toml" ]; then
        echo "SKIP $plugin (no Cargo.toml)"
        continue
    fi

    echo "Building $plugin..."
    (cd "$src" && cargo build --release --target wasm32-wasip1 2>&1)

    # Find the .wasm file
    wasm_file=$(find "$src/target/wasm32-wasip1/release" -maxdepth 1 -name "*.wasm" 2>/dev/null | head -1)
    if [ -z "$wasm_file" ]; then
        echo "  ERROR: no .wasm output for $plugin"
        continue
    fi

    # Read plugin id from plugin.toml
    plugin_id=$(grep '^id' "$src/plugin.toml" | head -1 | sed 's/id = "\(.*\)"/\1/')

    dest="$INSTALL_DIR/$plugin_id"
    mkdir -p "$dest"
    cp "$src/plugin.toml" "$dest/plugin.toml"
    cp "$wasm_file" "$dest/plugin.wasm"
    echo "  Installed $plugin_id → $dest"
done

echo ""
echo "Done. Run 'taskhub plugin list' to verify."

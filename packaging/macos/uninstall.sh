#!/bin/sh
set -eu

if [ "$(uname -s)" != "Darwin" ]; then
    echo "Error: this uninstaller only supports macOS." >&2
    exit 1
fi

bin_dir="${HELPCORE_BIN_DIR:-$HOME/.local/bin}"
plist_path="$HOME/Library/LaunchAgents/dev.helpcore.server.plist"
service="gui/$(id -u)/dev.helpcore.server"

launchctl bootout "$service" >/dev/null 2>&1 || true
rm -f "$plist_path"
rm -f "$bin_dir/helpcore-server" "$bin_dir/hc" "$bin_dir/helpcore"

echo "Removed the helpcore LaunchAgent and binaries."
echo "User config and data under ${HELPCORE_STATE_DIR:-$HOME/.helpcore} were preserved."

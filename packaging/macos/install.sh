#!/bin/sh
set -eu

if [ "$(uname -s)" != "Darwin" ]; then
    echo "Error: this installer only supports macOS." >&2
    exit 1
fi

repo_dir=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
bin_dir="${HELPCORE_BIN_DIR:-$HOME/.local/bin}"
state_dir="${HELPCORE_STATE_DIR:-$HOME/.helpcore}"
config_path="$state_dir/config.toml"
data_dir="$state_dir/data"
log_dir="$state_dir/logs"
agents_dir="${HELPCORE_LAUNCH_AGENTS_DIR:-$HOME/Library/LaunchAgents}"
plist_path="$agents_dir/dev.helpcore.server.plist"
template_path="$repo_dir/packaging/macos/dev.helpcore.server.plist.in"
domain="gui/$(id -u)"
service="$domain/dev.helpcore.server"

if command -v cargo >/dev/null 2>&1; then
    cargo_bin=$(command -v cargo)
elif command -v rustup >/dev/null 2>&1; then
    cargo_bin=$(rustup which cargo)
    PATH=$(dirname "$cargo_bin"):$PATH
    export PATH
else
    echo "Error: Rust is required. Install it from https://rustup.rs first." >&2
    exit 1
fi

command -v npm >/dev/null 2>&1 || {
    echo "Error: Node.js and npm are required to build the bundled web UI." >&2
    exit 1
}

echo "Building helpcore server and CLI..."
(cd "$repo_dir" && make CARGO="$cargo_bin" build)

mkdir -p "$bin_dir" "$state_dir" "$data_dir" "$log_dir" "$agents_dir"
install -m 0755 "$repo_dir/target/release/helpcore-server" "$bin_dir/helpcore-server"
install -m 0755 "$repo_dir/target/release/hc" "$bin_dir/hc"
install -m 0755 "$repo_dir/target/release/helpcore" "$bin_dir/helpcore"

if [ ! -e "$config_path" ]; then
    install -m 0600 "$repo_dir/config.toml.example" "$config_path"
    echo "Created $config_path"
else
    echo "Keeping existing config at $config_path"
fi

escape_sed() {
    printf '%s' "$1" | sed 's/[\/&]/\\&/g'
}

binary_escaped=$(escape_sed "$bin_dir/helpcore-server")
config_escaped=$(escape_sed "$config_path")
data_escaped=$(escape_sed "$data_dir")
stdout_escaped=$(escape_sed "$log_dir/server.log")
stderr_escaped=$(escape_sed "$log_dir/server.error.log")

sed \
    -e "s/@BINARY@/$binary_escaped/g" \
    -e "s/@CONFIG@/$config_escaped/g" \
    -e "s/@DATA@/$data_escaped/g" \
    -e "s/@STDOUT@/$stdout_escaped/g" \
    -e "s/@STDERR@/$stderr_escaped/g" \
    "$template_path" > "$plist_path"
chmod 0600 "$plist_path"

if [ "${HELPCORE_SKIP_LAUNCH:-0}" != "1" ]; then
    launchctl bootout "$service" >/dev/null 2>&1 || true
    launchctl bootstrap "$domain" "$plist_path"
    launchctl kickstart -k "$service"
fi

echo
if [ "${HELPCORE_SKIP_LAUNCH:-0}" = "1" ]; then
    echo "helpcore is installed. LaunchAgent startup was skipped."
else
    echo "helpcore is installed and running."
fi
echo "  Server: http://localhost:3000"
echo "  Config: $config_path"
echo "  Logs:   $log_dir"
echo "  CLI:    $bin_dir/hc"
echo
echo "Add $bin_dir to PATH if it is not already available in your shell."

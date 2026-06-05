#!/bin/sh
set -eu

config_path="${HELPCORE_CONFIG:-/config/config.toml}"
default_config="${HELPCORE_DEFAULT_CONFIG:-/usr/local/share/helpcore/default-config.toml}"
data_dir="${HELPCORE_DATA:-/data}"

if [ -d "$config_path" ]; then
    echo "Error: $config_path is a directory, not a file." >&2
    exit 1
fi

mkdir -p "$(dirname "$config_path")" "$data_dir"

if [ ! -e "$config_path" ]; then
    cp "$default_config" "$config_path"
    chown helpcore:helpcore "$config_path"
    echo "Created default config at $config_path"
fi

# Fresh Docker volumes are root-owned. The server only needs ownership of the
# volume root; files created below it will inherit the unprivileged user.
chown helpcore:helpcore "$data_dir"

if [ "$(id -u)" = "0" ]; then
    exec su-exec helpcore "$@"
fi

exec "$@"

#!/bin/sh
set -eu

config_path="${HELPCORE_CONFIG:-/config/config.toml}"
default_config="${HELPCORE_DEFAULT_CONFIG:-/usr/local/share/helpcore/default-config.toml}"
data_dir="${HELPCORE_DATA:-/data}"

if [ -d "$config_path" ]; then
    echo "Error: $config_path is a directory, not a file." >&2
    exit 1
fi

config_dir="$(dirname "$config_path")"
mkdir -p "$config_dir" "$data_dir"

if [ "$(id -u)" = "0" ]; then
    chown helpcore:helpcore "$config_dir" 2>/dev/null \
        || echo "Warning: cannot change ownership of config directory $config_dir" >&2
    chmod 0700 "$config_dir" 2>/dev/null \
        || echo "Warning: cannot change mode of config directory $config_dir" >&2
    if [ -f "$config_path" ]; then
        chown helpcore:helpcore "$config_path" 2>/dev/null \
            || echo "Warning: cannot change ownership of config file $config_path" >&2
        chmod 0600 "$config_path" 2>/dev/null \
            || echo "Warning: cannot change mode of config file $config_path" >&2
    fi
fi

if [ ! -e "$config_path" ]; then
    cp "$default_config" "$config_path"
    chown helpcore:helpcore "$config_path"
    chmod 0600 "$config_path"
    echo "Created default config at $config_path"
fi

# Fresh Docker volumes are root-owned. The server only needs ownership of the
# volume root; files created below it will inherit the unprivileged user.
chown helpcore:helpcore "$data_dir"

if [ "$(id -u)" = "0" ]; then
    exec su-exec helpcore "$@"
fi

exec "$@"

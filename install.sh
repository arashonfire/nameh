#!/bin/sh
# Build and install Nameh and its desktop/MIME integration.
set -eu

usage() {
    cat <<'EOF'
Usage: ./install.sh [--prefix PATH] [--offline]

Build the locked release and install Nameh, its application-menu shortcut,
icon, Markdown MIME definitions, and documentation. Refresh available caches.
The default prefix is $HOME/.local (or $PREFIX if set). No sudo is used.

  --prefix PATH  Install beneath this absolute path
  --offline      Use only previously downloaded Cargo dependencies
  -h, --help     Show this help

DESTDIR may be set to stage a package without refreshing desktop caches.
Existing MIME default applications are preserved.
EOF
}

prefix=${PREFIX:-"$HOME/.local"}
cargo_flags=--locked
while [ "$#" -gt 0 ]; do
    case "$1" in
        --prefix)
            [ "$#" -ge 2 ] || { echo 'Missing path after --prefix' >&2; exit 2; }
            prefix=$2
            shift 2
            ;;
        --offline) cargo_flags='--locked --offline'; shift ;;
        -h|--help) usage; exit 0 ;;
        *) echo "Unknown argument: $1" >&2; usage >&2; exit 2 ;;
    esac
done
case "$prefix" in
    /*) ;;
    *) echo 'The installation prefix must be an absolute path.' >&2; exit 2 ;;
esac

root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
for tool in cargo make install pkg-config; do
    command -v "$tool" >/dev/null 2>&1 || {
        echo "Missing build prerequisite: $tool. See README.md for setup." >&2
        exit 1
    }
done
pkg-config --atleast-version=4.10 gtk4 || {
    echo 'GTK 4.10 or newer development libraries are required.' >&2
    exit 1
}

# Reuse this checkout's dependency cache unless the caller chose a Cargo home.
if [ -z "${CARGO_HOME:-}" ] && [ -d "$root/.cargo-home" ]; then
    export CARGO_HOME="$root/.cargo-home"
fi

make -C "$root" install "PREFIX=$prefix" "CARGO_FLAGS=$cargo_flags"
if [ -n "${DESTDIR:-}" ]; then
    echo "Nameh staged under $DESTDIR$prefix."
else
    echo "Nameh installed under $prefix."
    echo "Keep $prefix/bin on your desktop session's PATH for the shortcut."
    echo 'Choose Nameh in your file manager’s Open With menu to open Markdown files.'
fi

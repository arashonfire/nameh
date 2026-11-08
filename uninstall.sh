#!/bin/sh
# Remove only the files installed by Nameh's Makefile.
set -eu

usage() {
    cat <<'EOF'
Usage: ./uninstall.sh [--prefix PATH]

Remove Nameh, its application-menu shortcut, icon, MIME definitions, and
documentation, then refresh available caches. Documents and preferences remain.
The default prefix is $HOME/.local (or $PREFIX if set). No sudo is used.

  --prefix PATH  Remove from this absolute installation prefix
  -h, --help     Show this help

Use the same PREFIX and DESTDIR as when installing. Cargo is not required.
EOF
}

prefix=${PREFIX:-"$HOME/.local"}
while [ "$#" -gt 0 ]; do
    case "$1" in
        --prefix)
            [ "$#" -ge 2 ] || { echo 'Missing path after --prefix' >&2; exit 2; }
            prefix=$2
            shift 2
            ;;
        -h|--help) usage; exit 0 ;;
        *) echo "Unknown argument: $1" >&2; usage >&2; exit 2 ;;
    esac
done
case "$prefix" in
    /*) ;;
    *) echo 'The installation prefix must be an absolute path.' >&2; exit 2 ;;
esac

root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
command -v make >/dev/null 2>&1 || { echo 'Uninstall requires make.' >&2; exit 1; }
make -C "$root" uninstall "PREFIX=$prefix"
echo "Nameh removed from ${DESTDIR:-}$prefix. Documents and preferences kept."

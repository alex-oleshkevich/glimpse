#!/usr/bin/env bash
set -euo pipefail

desktop="${XDG_DATA_HOME:-$HOME/.local/share}/applications/me.aresa.GlimpseExecDemo.desktop"

case "${1:-}" in
    install)
        shift
        script="$(realpath "$(dirname "$0")/exec-applet-demo.py")"
        exec_line=""
        for arg in "$script" "$@"; do
            case "$arg" in
                *[$'\n\r\t'\"\$\`\\]*) printf 'Exec arguments cannot contain quotes, $, `, \\ or control characters: %s\n' "$arg" >&2; exit 2 ;;
            esac
            arg="${arg//%/%%}"
            exec_line+=" \"$arg\""
        done
        mkdir -p "$(dirname "$desktop")"
        printf '[Desktop Entry]\nType=Application\nVersion=1.5\nName=Exec demo\nIcon=applications-science-symbolic\nExec=%s\nNoDisplay=true\nImplements=me.aresa.Glimpse.Applet1\n' "${exec_line:1}" > "$desktop"
        ;;
    remove)
        [[ "$#" -eq 1 ]] || { printf 'Usage: %s install [args...] | remove\n' "$0" >&2; exit 2; }
        rm -f "$desktop"
        ;;
    *)
        printf 'Usage: %s install [args...] | remove\n' "$0" >&2
        exit 2
        ;;
esac

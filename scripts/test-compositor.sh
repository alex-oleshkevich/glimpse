#!/usr/bin/env bash
set -euo pipefail

crate="${1:-}"
filter="${2:-}"

if [ -z "${WAYLAND_DISPLAY:-}" ] && [ -z "${DISPLAY:-}" ]; then
    echo "test-compositor: neither WAYLAND_DISPLAY nor DISPLAY is set; every GTK test would fail at gtk4::init" >&2
    exit 1
fi

if [ -n "$crate" ]; then
    crates="$crate"
    [ -z "$filter" ] && cargo test -p "$crate"
else
    crates=$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[].name')
    cargo test --workspace
fi

manual=(
    "glimpse-services :: services::brightness::tests::a_real_drm_change_event_reaches_the_hotplug_stream"
)
failed=()
left_out=()
for crate in $crates; do
    names=$(cargo test -p "$crate" -- --list --ignored | sed -n 's/: test$//p')
    if [ -n "$filter" ]; then
        names=$(printf '%s\n' "$names" | grep -F -- "$filter") || true
    fi
    while IFS= read -r name; do
        [ -z "$name" ] && continue
        if printf '%s\n' "${manual[@]}" | grep -qxF -- "$crate :: $name"; then
            left_out+=("$crate :: $name")
            continue
        fi
        echo "==> $crate :: $name"
        cargo test -p "$crate" "$name" -- --ignored --exact || failed+=("$crate :: $name")
    done <<< "$names"
done

if [ "${#left_out[@]}" -gt 0 ]; then
    echo "test-compositor: left out, needs a manual action (run with just test-one):"
    printf '  %s\n' "${left_out[@]}"
fi

if [ "${#failed[@]}" -gt 0 ]; then
    echo "test-compositor: failed ignored tests:" >&2
    printf '  %s\n' "${failed[@]}" >&2
    exit 1
fi

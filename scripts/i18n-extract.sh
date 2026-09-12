#!/usr/bin/env bash
set -euo pipefail

# Writes a .pot to $1 (default po/glimpse.pot). Used by `just extract-strings` and by
# `just check-strings`, which runs it into a temporary file and diffs.

out="${1:-po/glimpse.pot}"
version="$(awk -F'"' '/^version = / { print $2; exit }' Cargo.toml)"

# Every Blueprint and Rust file in the tree, with no filter for which of them carry a marker:
# a filter that was wrong would hide a file from the extractor and from the coverage check
# below at the same time, which is the one failure neither could report.
mapfile -t files < <(rg --files -g '*.blp' -g '*.rs' -g '!_old/**' -g '!var/**' | sort)

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

printf '%s\n' "${files[@]}" | python3 scripts/i18n-scrub.py "$work"

warnings="$work/xgettext.stderr"

# Stderr is captured to a file rather than piped, so that a hard xgettext failure is still an
# error here. Through a pipe its exit status is lost and the run fails further down on a
# missing .pot, which says nothing about what actually went wrong.
if ! (
    cd "$work"
    xgettext \
        --from-code=UTF-8 \
        --language=C \
        --keyword=_ \
        --keyword=gettext \
        --keyword=ngettext:1,2 \
        --keyword=pgettext:1c,2 \
        --keyword=npgettext:1c,2,3 \
        --add-comments=TRANSLATORS \
        --sort-by-file \
        --package-name=glimpse \
        --package-version="$version" \
        --msgid-bugs-address="https://github.com/alex-oleshkevich/glimpse/issues" \
        --copyright-holder="Alex Oleshkevich" \
        -o glimpse.pot \
        "${files[@]}"
) 2> "$warnings"; then
    cat "$warnings" >&2
    exit 1
fi

# 'extension blp is unknown' is the fallback --language=C asks for. The 'unterminated' notes are
# the C scanner meeting a Rust raw or multi-line string, which it recovers from; i18n-coverage.py
# is what proves nothing was lost, so neither is repeated on every run. Anything else is new and
# stays visible.
grep -vE "unknown; will try C|unterminated (string literal|character constant)" \
    "$warnings" >&2 || true

mkdir -p "$(dirname "$out")"
cp "$work/glimpse.pot" "$out"

python3 scripts/i18n-coverage.py "$out" "${files[@]}"

# A wrapped msgid still opens with one `msgid ""` line, so this counts each entry once; the
# subtraction drops the header, which is itself an empty msgid.
printf '%s: %d strings from %d files\n' \
    "$out" "$(($(grep -c '^msgid ' "$out") - 1))" "${#files[@]}"

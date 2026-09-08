"""Fail when a marked literal never reached the catalog.

The C scanner xgettext falls back to for Blueprint and Rust can lose its place inside a raw or
multi-line string and skip the rest of a file. The only symptom is a msgid that quietly stopped
existing, which is indistinguishable from a string nobody marked. This reads the markers straight
out of the sources and compares them against what the .pot holds.
"""

import pathlib
import re
import sys

CALL = re.compile(r"(?:^|[^A-Za-z0-9_])(?:_|n?p?gettext)\s*\(")
# Consecutive string arguments, rather than a bracket count: a msgid containing a parenthesis
# would unbalance a count and send the scan off through the rest of the file. Blueprint also
# accepts a single-quoted string, which xgettext's C scanner skips without a word; reading those
# here is what turns that trap into a failure.
ARGUMENT = re.compile(r"""\s*(?:"((?:[^"\\]|\\.)*)"|'((?:[^'\\]|\\.)*)')\s*,?""")
ENTRY = re.compile(r'^(?:msgid|msgid_plural|msgctxt)\s+"((?:[^"\\]|\\.)*)"$')
CONTINUATION = re.compile(r'^"((?:[^"\\]|\\.)*)"$')
ESCAPES = {"n": "\n", "t": "\t", "r": "\r"}


def unescape(text: str) -> str:
    return re.sub(r"\\(.)", lambda found: ESCAPES.get(found.group(1), found.group(1)), text)


def marked(path: str) -> set[str]:
    source = pathlib.Path(path).read_text(encoding="utf-8")
    single_quotes_are_strings = path.endswith(".blp")

    found = set()
    for call in CALL.finditer(source):
        at = call.end()
        while (argument := ARGUMENT.match(source, at)) is not None:
            double, single = argument.group(1), argument.group(2)
            if double is not None:
                found.add(unescape(double))
            elif single_quotes_are_strings:
                found.add(unescape(single))
            at = argument.end()
    return found


def catalog(path: str) -> set[str]:
    # A msgid longer than the wrap width is written as `msgid ""` followed by bare "…" lines,
    # so the pieces are joined back before anything is compared.
    entries: set[str] = set()
    current: str | None = None
    for line in pathlib.Path(path).read_text(encoding="utf-8").splitlines():
        if (entry := ENTRY.match(line)) is not None:
            if current is not None:
                entries.add(current)
            current = unescape(entry.group(1))
        elif current is not None and (more := CONTINUATION.match(line)) is not None:
            current += unescape(more.group(1))
        elif current is not None:
            entries.add(current)
            current = None
    if current is not None:
        entries.add(current)
    return entries


def main() -> None:
    pot, sources = sys.argv[1], sys.argv[2:]
    known = catalog(pot)

    missing = [
        (source, literal)
        for source in sources
        for literal in sorted(marked(source))
        if literal and literal not in known
    ]

    if missing:
        print(f"{len(missing)} marked string(s) never reached {pot}:", file=sys.stderr)
        for source, literal in missing:
            print(f"  {source}: {literal!r}", file=sys.stderr)
        print(
            "This is usually a raw or multi-line string earlier in the file derailing the "
            "C scanner, or a marker written with single quotes.",
            file=sys.stderr,
        )
        raise SystemExit(1)


if __name__ == "__main__":
    main()

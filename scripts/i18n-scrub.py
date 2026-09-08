"""Copy the files named on stdin into the work tree given as $1, blanking Rust lifetimes.

xgettext has no Rust scanner. Its C fallback reads `&'static str` as the start of a character
constant and abandons it at end of line, taking any string that followed with it. Replacing a
lifetime with spaces of the same width keeps every byte position, so the .pot still points a
translator at the real source line. Blueprint is copied unchanged.

Strings and comments are stepped over rather than scrubbed. A regex cannot tell the lifetime in
`&'a str` from the apostrophe in `gettext("It's raining")` — both are a quote followed by letters
and closed by neither — so scrubbing blind would rewrite that msgid to "It  raining" and the
catalog would hold a string the code never asks for. A character literal such as 'a' needs no
special case: its closing quote is what the lifetime pattern refuses to match.
"""

import pathlib
import re
import sys

LIFETIME = re.compile(r"'[A-Za-z_][A-Za-z0-9_]*(?!')")
RAW_STRING = re.compile(r'b?r(#*)"')


def scrub(text: str) -> str:
    out: list[str] = []
    at, end = 0, len(text)

    while at < end:
        if (raw := RAW_STRING.match(text, at)) is not None:
            closing = text.find('"' + raw.group(1), raw.end())
            stop = end if closing < 0 else closing + 1 + len(raw.group(1))
        elif text[at] == '"':
            stop = at + 1
            while stop < end and text[stop] != '"':
                stop += 2 if text[stop] == "\\" else 1
            stop = min(stop + 1, end)
        elif text.startswith("//", at):
            newline = text.find("\n", at)
            stop = end if newline < 0 else newline
        elif text.startswith("/*", at):
            closing = text.find("*/", at + 2)
            stop = end if closing < 0 else closing + 2
        elif (lifetime := LIFETIME.match(text, at)) is not None:
            out.append(" " * len(lifetime.group()))
            at = lifetime.end()
            continue
        else:
            stop = at + 1

        out.append(text[at:stop])
        at = stop

    return "".join(out)


def main() -> None:
    work = pathlib.Path(sys.argv[1])
    for line in sys.stdin:
        relative = line.strip()
        if not relative:
            continue
        source = pathlib.Path(relative)
        text = source.read_text(encoding="utf-8")
        if source.suffix == ".rs":
            text = scrub(text)
        destination = work / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(text, encoding="utf-8")


if __name__ == "__main__":
    main()

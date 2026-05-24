#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
"""
Mock CUPS IPP server for testing the Glimpse printing applet widget.

Runs a minimal IPP-over-HTTP server (no external deps) that cycles through
realistic printer and job states.  Each Enter keypress advances to the next
scenario; --auto N advances automatically every N seconds.

Usage:
    ./scripts/printing-mock-cups.py
    ./scripts/printing-mock-cups.py --auto 4

Launch glimpse-shell with the mock:
    GLIMPSE_CUPS_URL=http://localhost:16631/ ./target/debug/glimpse-shell

Port defaults to 16631; override with GLIMPSE_MOCK_PORT env var.
"""

import argparse
import os
import struct
import sys
import threading
import time
from dataclasses import dataclass, field
from http.server import BaseHTTPRequestHandler, HTTPServer
from typing import Optional


# ── IPP binary constants (RFC 8011) ──────────────────────────────────────────

# Delimiter tags
DTAG_OP      = 0x01
DTAG_JOB     = 0x02
DTAG_END     = 0x03
DTAG_PRINTER = 0x04

# Value tags
TAG_INTEGER  = 0x21
TAG_ENUM     = 0x23
TAG_TEXT     = 0x41   # textWithoutLanguage
TAG_NAME     = 0x42   # nameWithoutLanguage
TAG_KEYWORD  = 0x44
TAG_URI      = 0x45
TAG_CHARSET  = 0x47
TAG_LANG     = 0x48   # naturalLanguage

# Operation codes
OP_GET_JOBS          = 0x000a
OP_CANCEL_JOB        = 0x0008
OP_HOLD_JOB          = 0x000c
OP_RELEASE_JOB       = 0x000d
OP_CUPS_GET_PRINTERS = 0x4001

STATUS_OK = 0x0000


# ── IPP encoding ─────────────────────────────────────────────────────────────

def _enc(tag: int, name: str, value) -> bytes:
    """Encode a single IPP attribute."""
    n = name.encode("ascii")
    if isinstance(value, str):
        v = value.encode("utf-8")
    elif isinstance(value, int):
        v = struct.pack(">i", value)
    elif isinstance(value, bytes):
        v = value
    else:
        raise TypeError(f"Cannot encode {type(value)}")
    return bytes([tag]) + struct.pack(">H", len(n)) + n + struct.pack(">H", len(v)) + v


def attrs(tag: int, name: str, values) -> bytes:
    """Encode one or more values under the same attribute name (IPP set encoding)."""
    if not isinstance(values, list):
        values = [values]
    out = _enc(tag, name, values[0])
    for v in values[1:]:
        out += _enc(tag, "", v)     # subsequent values have empty name
    return out


def response_header(request_id: int, status: int = STATUS_OK) -> bytes:
    """IPP response: version 1.1 + status + request-id."""
    return struct.pack(">BBH", 1, 1, status) + struct.pack(">I", request_id)


def op_group(request_id: int, status: int = STATUS_OK) -> bytes:
    """Standard operation-attributes group that starts every IPP response."""
    return (
        response_header(request_id, status)
        + bytes([DTAG_OP])
        + attrs(TAG_CHARSET, "attributes-charset",          "utf-8")
        + attrs(TAG_LANG,    "attributes-natural-language", "en")
    )


# ── State model ───────────────────────────────────────────────────────────────

@dataclass
class Marker:
    name:  str
    level: int   # 0–100
    kind:  str   # "toner" | "ink"


@dataclass
class Printer:
    name:      str
    model:     str                      = "Generic Printer"
    state:     int                      = 3   # 3=idle 4=processing 5=stopped
    reasons:   list[str]                = field(default_factory=list)
    job_count: int                      = 0
    markers:   list[Marker]             = field(default_factory=list)


@dataclass
class Job:
    id:               int
    name:             str
    printer:          str
    state:            int               = 5   # 3=pending 4=held 5=processing
    pages_completed:  Optional[int]     = None
    pages_total:      Optional[int]     = None


@dataclass
class Scenario:
    label:       str
    description: str
    printers:    list[Printer] = field(default_factory=list)
    jobs:        list[Job]     = field(default_factory=list)


# ── IPP response builders ─────────────────────────────────────────────────────

def _printer_group(p: Printer, port: int) -> bytes:
    uri = f"ipp://localhost:{port}/printers/{p.name}"
    reasons = p.reasons or ["none"]
    out = (
        bytes([DTAG_PRINTER])
        + attrs(TAG_URI,     "printer-uri-supported",   uri)
        + attrs(TAG_NAME,    "printer-name",             p.name)
        + attrs(TAG_TEXT,    "printer-make-and-model",   p.model)
        + attrs(TAG_ENUM,    "printer-state",            p.state)
        + attrs(TAG_KEYWORD, "printer-state-reasons",    reasons)
        + attrs(TAG_INTEGER, "queued-job-count",         p.job_count)
    )
    if p.markers:
        out += attrs(TAG_NAME,    "marker-names",  [m.name  for m in p.markers])
        out += attrs(TAG_INTEGER, "marker-levels", [m.level for m in p.markers])
        out += attrs(TAG_KEYWORD, "marker-types",  [m.kind  for m in p.markers])
    return out


def _job_group(j: Job, port: int) -> bytes:
    printer_uri = f"ipp://localhost:{port}/printers/{j.printer}"
    out = (
        bytes([DTAG_JOB])
        + attrs(TAG_INTEGER, "job-id",          j.id)
        + attrs(TAG_NAME,    "job-name",         j.name)
        + attrs(TAG_ENUM,    "job-state",        j.state)
        + attrs(TAG_URI,     "job-printer-uri",  printer_uri)
    )
    if j.pages_total is not None:
        out += attrs(TAG_INTEGER, "job-impressions", j.pages_total)
    if j.pages_completed is not None:
        out += attrs(TAG_INTEGER, "job-impressions-completed", j.pages_completed)
    return out


def build_get_printers(request_id: int, s: Scenario, port: int) -> bytes:
    body = op_group(request_id)
    for p in s.printers:
        body += _printer_group(p, port)
    return body + bytes([DTAG_END])


def build_get_jobs(request_id: int, s: Scenario, port: int) -> bytes:
    body = op_group(request_id)
    for j in s.jobs:
        body += _job_group(j, port)
    return body + bytes([DTAG_END])


def build_ok(request_id: int) -> bytes:
    return op_group(request_id) + bytes([DTAG_END])


# ── Scenarios ─────────────────────────────────────────────────────────────────

HP   = "Office_Printer"
PIX  = "Photo_Printer"

SCENARIOS: list[Scenario] = [
    Scenario(
        "1 · Idle",
        "No jobs — applet should be hidden",
        printers=[Printer(HP, "HP LaserJet Pro M404n")],
    ),
    Scenario(
        "2 · One job printing (with page progress)",
        "Label '1', spinner replaced by 'Page 3 of 12'",
        printers=[Printer(HP, "HP LaserJet Pro M404n", state=4, job_count=1)],
        jobs=[Job(101, "Report_Q3.pdf", HP, state=5, pages_completed=3, pages_total=12)],
    ),
    Scenario(
        "3 · One job printing (no page count)",
        "Label '1', spinner visible",
        printers=[Printer(HP, "HP LaserJet Pro M404n", state=4, job_count=1)],
        jobs=[Job(102, "Untitled_Document.docx", HP, state=5)],
    ),
    Scenario(
        "4 · Three jobs: processing + 2 pending",
        "Label '3', Pause + Cancel on first job",
        printers=[Printer(HP, "HP LaserJet Pro M404n", state=4, job_count=3)],
        jobs=[
            Job(103, "Invoice_Oct.pdf",      HP, state=5, pages_completed=1, pages_total=2),
            Job(104, "Contract_Draft.docx",  HP, state=3),
            Job(105, "Presentation.pptx",    HP, state=3),
        ],
    ),
    Scenario(
        "5 · One held (paused) job",
        "Resume + Cancel actions visible",
        printers=[Printer(HP, "HP LaserJet Pro M404n", job_count=1)],
        jobs=[Job(106, "BigReport.pdf", HP, state=4)],
    ),
    Scenario(
        "6 · Mixed: processing + held + pending",
        "All three action types in one view",
        printers=[Printer(HP, "HP LaserJet Pro M404n", state=4, job_count=3)],
        jobs=[
            Job(107, "Printing_Now.pdf",  HP, state=5, pages_completed=8, pages_total=20),
            Job(108, "On_Hold.docx",      HP, state=4),
            Job(109, "Waiting.pdf",       HP, state=3),
        ],
    ),
    Scenario(
        "7 · Printer stopped — paper jam",
        "Error banner: 'Paper jam'",
        printers=[Printer(HP, "HP LaserJet Pro M404n", state=5, reasons=["paper-jam"])],
    ),
    Scenario(
        "8 · Printer stopped — out of paper",
        "Error banner: 'Out of paper'",
        printers=[Printer(HP, "HP LaserJet Pro M404n", state=5, reasons=["media-empty"])],
    ),
    Scenario(
        "9 · Multiple errors on same printer",
        "Two error banners: 'Out of paper' + 'Cover open'",
        printers=[Printer(HP, "HP LaserJet Pro M404n", state=5, reasons=["media-empty", "cover-open"])],
    ),
    Scenario(
        "10 · Low ink (toner)",
        "Ink section: Black 8% (critical), Cyan 3% (critical), Magenta 72%, Yellow 45%",
        printers=[
            Printer(HP, "HP Color LaserJet Pro", markers=[
                Marker("Black",   8,  "toner"),
                Marker("Cyan",    3,  "toner"),
                Marker("Magenta", 72, "toner"),
                Marker("Yellow",  45, "toner"),
            ]),
        ],
    ),
    Scenario(
        "11 · Two printers",
        "Office idle, Photo printing — both printer rows visible",
        printers=[
            Printer(HP,  "HP LaserJet Pro M404n"),
            Printer(PIX, "Canon PIXMA Pro-200", state=4, job_count=1, markers=[
                Marker("Black",     45, "ink"),
                Marker("Cyan",      18, "ink"),
                Marker("Magenta",   22, "ink"),
                Marker("Yellow",    60, "ink"),
                Marker("Photo Cyn", 55, "ink"),
            ]),
        ],
        jobs=[Job(110, "Wedding_Photo_12x16.jpg", PIX, state=5)],
    ),
    Scenario(
        "12 · All features combined",
        "Two printers, multiple jobs, ink levels, one error",
        printers=[
            Printer(HP, "HP LaserJet Pro M404n", state=5, reasons=["paper-jam"], job_count=2,
                    markers=[Marker("Black", 15, "toner")]),
            Printer(PIX, "Canon PIXMA Pro-200", state=4, job_count=1, markers=[
                Marker("Black",   85, "ink"),
                Marker("Cyan",    62, "ink"),
                Marker("Magenta", 48, "ink"),
            ]),
        ],
        jobs=[
            Job(111, "Blocked_Report.pdf",   HP,  state=3),
            Job(112, "Also_Waiting.docx",    HP,  state=4),
            Job(113, "Family_Portrait.jpg",  PIX, state=5, pages_completed=1, pages_total=1),
        ],
    ),
]


# ── HTTP / IPP handler ────────────────────────────────────────────────────────

_lock = threading.Lock()
_idx  = 0


def _current() -> Scenario:
    with _lock:
        return SCENARIOS[_idx]


class IppHandler(BaseHTTPRequestHandler):

    def do_POST(self):
        te = self.headers.get("Transfer-Encoding", "").lower()
        if "chunked" in te:
            body = b""
            while True:
                size_line = self.rfile.readline().strip()
                chunk_size = int(size_line, 16)
                if chunk_size == 0:
                    break
                body += self.rfile.read(chunk_size)
                self.rfile.read(2)  # discard trailing CRLF
        else:
            length = int(self.headers.get("Content-Length", 0))
            body = self.rfile.read(length) if length else b""

        if len(body) < 8:
            _info(f"bad request: body={body!r} content-length={self.headers.get('Content-Length')} te={self.headers.get('Transfer-Encoding')}")
            self.send_error(400, "Bad IPP request")
            return

        _, _, operation = struct.unpack(">BBH", body[:4])
        request_id     = struct.unpack(">I",    body[4:8])[0]

        port = self.server.server_address[1]
        s    = _current()

        op_labels = {
            OP_CUPS_GET_PRINTERS: "get-printers",
            OP_GET_JOBS:          "get-jobs",
            OP_CANCEL_JOB:        "cancel-job",
            OP_HOLD_JOB:          "hold-job",
            OP_RELEASE_JOB:       "release-job",
        }
        _info(f"← {op_labels.get(operation, f'op 0x{operation:04x}')}")

        if operation == OP_CUPS_GET_PRINTERS:
            resp = build_get_printers(request_id, s, port)
        elif operation == OP_GET_JOBS:
            resp = build_get_jobs(request_id, s, port)
        elif operation in (OP_CANCEL_JOB, OP_HOLD_JOB, OP_RELEASE_JOB):
            resp = build_ok(request_id)
        else:
            resp = build_ok(request_id)

        self.send_response(200)
        self.send_header("Content-Type",   "application/ipp")
        self.send_header("Content-Length", str(len(resp)))
        self.end_headers()
        self.wfile.write(resp)

    def log_message(self, *_):
        pass


# ── Display ───────────────────────────────────────────────────────────────────

BOLD  = "\033[1m"
CYAN  = "\033[1;36m"
DIM   = "\033[2m"
RED   = "\033[31m"
GREEN = "\033[32m"
RESET = "\033[0m"


def _info(msg: str):
    print(f"  {DIM}→ {msg}{RESET}", flush=True)


def _show(idx: int):
    s = SCENARIOS[idx]
    total = len(SCENARIOS)
    print(f"\n{CYAN}[{idx + 1}/{total}]  {s.label}{RESET}")
    print(f"  {DIM}{s.description}{RESET}")

    pstate = {3: f"{GREEN}idle{RESET}", 4: f"{CYAN}printing{RESET}", 5: f"{RED}stopped{RESET}"}
    jstate = {3: "pending", 4: f"{CYAN}held{RESET}", 5: f"{GREEN}printing{RESET}"}

    for p in s.printers:
        ps = pstate.get(p.state, str(p.state))
        reasons = f"  {RED}({', '.join(p.reasons)}){RESET}" if p.reasons else ""
        ink = ""
        if p.markers:
            ink = "  ink: " + ", ".join(
                f"{m.name}={RED if m.level < 10 else CYAN if m.level < 25 else ''}{m.level}%{RESET}"
                for m in p.markers
            )
        print(f"  {BOLD}Printer{RESET}  {p.name} — {ps}{reasons}{ink}")

    for j in s.jobs:
        js = jstate.get(j.state, str(j.state))
        pg = f"  p.{j.pages_completed}/{j.pages_total}" if j.pages_total is not None else ""
        print(f"  {BOLD}Job #{j.id}{RESET}  {j.name!r} → {j.printer}  [{js}{pg}]")

    if not s.printers and not s.jobs:
        print(f"  {DIM}(empty){RESET}")


# ── Main ─────────────────────────────────────────────────────────────────────

def main():
    global _idx

    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--auto", metavar="SEC", type=float, default=0,
                    help="auto-advance every SEC seconds instead of waiting for Enter")
    args = ap.parse_args()

    port = int(os.environ.get("GLIMPSE_MOCK_PORT", "16631"))
    server = HTTPServer(("127.0.0.1", port), IppHandler)

    print(f"\n{BOLD}Mock CUPS IPP server listening on http://127.0.0.1:{port}/{RESET}")
    print(f"  Launch with:  GLIMPSE_CUPS_URL=http://127.0.0.1:{port}/ glimpse-shell")
    if args.auto:
        print(f"  Auto-advancing every {args.auto}s.  Ctrl-C to stop.")
    else:
        print(f"  Press {BOLD}Enter{RESET} to advance scenario, {BOLD}Ctrl-C{RESET} to stop.")

    threading.Thread(target=server.serve_forever, daemon=True).start()
    _show(0)

    try:
        if args.auto:
            while True:
                time.sleep(args.auto)
                with _lock:
                    _idx = (_idx + 1) % len(SCENARIOS)
                    idx = _idx
                _show(idx)
        else:
            while True:
                input()
                with _lock:
                    _idx = (_idx + 1) % len(SCENARIOS)
                    idx = _idx
                _show(idx)
    except (KeyboardInterrupt, EOFError):
        print(f"\n{DIM}Shutting down.{RESET}")
        server.shutdown()


if __name__ == "__main__":
    main()

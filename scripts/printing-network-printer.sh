#!/usr/bin/env bash
# Publishes a fake IPP network printer that the REAL CUPS daemon discovers, so the printing applet
# can be exercised against the whole stack rather than a stub.
#
# This is not what `printing-mock-cups.py` does. That one REPLACES cups: it serves IPP itself and
# the panel is pointed at it with `[printing] server-url`, so nothing of the real daemon is
# involved. This one goes the other way — `ippeveprinter` (shipped with cups) serves a real IPP
# Everywhere printer and advertises it over DNS-SD, avahi carries it, cups-browsed picks it up and
# creates a temporary queue, and the applet sees it through the ordinary local cups server. Use the
# mock for widget states, this for discovery, job control and anything that has to be true of cups
# itself.
#
# No root: the emulator binds a high port and the queue is created by cups-browsed on its own.
#
#   scripts/printing-network-printer.sh run [options]  publish it and drive it from a menu until ^C
#   scripts/printing-network-printer.sh up [options]   publish it and wait for cups to discover it
#   scripts/printing-network-printer.sh jobs [N]       queue N held jobs (default 3)
#   scripts/printing-network-printer.sh status         emulator, queue and jobs
#   scripts/printing-network-printer.sh down           cancel jobs, stop it, let the queue expire
#
# `run` is the one to reach for by hand: it owns the emulator, so ^C tears everything down, and it
# is the only mode that survives the shell honestly — a backgrounded `up` takes SIGHUP with the
# terminal that started it.
#
# What the applet reads is `printer-state`, `printer-state-reasons` and the job list, so those are
# what the menu moves — and nothing else, because nothing else is reachable. Measured here:
#
#   jobs                  `lp -H hold` to queue one, `lp -i <id> -H resume` to start it. `--slow`
#                         sets a print command that sleeps, which is the only way a job stays in
#                         `processing` long enough to look at.
#   jam / stopped         `cupsdisable -r`, the only route to `printer-state = stopped`, which is
#                         also the only way to reach `render::attention`. **It needs the queue
#                         instantiated by one job first** — until then cups answers
#                         `client-error-not-found`. It reports as `printer-state-reasons = paused`;
#                         the text goes to `printer-state-message`, which this applet does not read.
#
# **Low toner and out-of-paper cannot be simulated for this applet, and the menu does not pretend
# otherwise.** `ippeveprinter`'s own /supplies and /media forms do set real `toner-low-report`,
# `toner-empty-report`, `media-low-report` and `media-empty-report` keywords, confirmed by reading
# the emulator directly on its own port. None of them reach cups: a discovered queue reports
# `printer-state-reasons = none` whether idle or mid-job, a permanent `lpadmin` queue pointed at the
# same device does too, and `fetch_printers` asks the cups server with `CUPS-Get-Printers` rather
# than the device. Seeing a supply reason would need the applet to read the printer directly.
#
# Options for `up`:
#   --name NAME    printer name as advertised (default "Glimpse Test Printer")
#   --port N       port for the emulator (default 18631)
#   --slow N       take N seconds per job, so jobs sit in `processing` long enough to watch
#                  (default 25 under `run`, so a released job can actually be watched)
set -uo pipefail

STATE_DIR="${XDG_RUNTIME_DIR:-/tmp}/glimpse"
PIDFILE="$STATE_DIR/printing-test.pid"
NAMEFILE="$STATE_DIR/printing-test.name"
LOGFILE="$STATE_DIR/printing-test.log"
SPOOL="$STATE_DIR/printing-test-spool"
SLOWCMD="$STATE_DIR/printing-test-slow"

NAME="Glimpse Test Printer"
named=0
job_seq=0
interactive=0
PORT=18631
SLOW=0
DISCOVER_TIMEOUT=30

die() {
    printf 'printing-network-printer: %s\n' "$1" >&2
    exit 1
}
note() { printf 'printing-network-printer: %s\n' "$1"; }

queue_name() { printf '%s\n' "${NAME// /_}"; }

# `up` records the published name so `jobs`, `status` and `down` act on the printer that is
# actually up rather than on the default, which is what a --name on `up` alone would leave them
# doing.
adopt_published_name() {
    [ "$named" -eq 1 ] && return 0
    [ -r "$NAMEFILE" ] || return 0
    NAME=$(cat "$NAMEFILE")
}

running() { [ -r "$PIDFILE" ] && kill -0 "$(cat "$PIDFILE")" 2>/dev/null; }

require() {
    command -v "$1" >/dev/null || die "$1 is missing; install cups$2"
}

await_queue() {
    local want=$1 try
    for try in $(seq "$DISCOVER_TIMEOUT"); do
        lpstat -e 2>/dev/null | grep -qx "$want" && return 0
        sleep 1
    done
    return 1
}

up() {
    require ippeveprinter ""
    require lpstat ""
    running && die "already published; run \`down\` first"

    systemctl is-active --quiet avahi-daemon ||
        die "avahi-daemon is not running, so nothing can discover the printer"
    systemctl is-active --quiet cups ||
        die "cups is not running, so there is nothing to discover it"

    mkdir -p "$STATE_DIR" "$SPOOL"

    local -a command=()
    if [ "$SLOW" -gt 0 ]; then
        printf '#!/bin/sh\nsleep %s\n' "$SLOW" >"$SLOWCMD"
        chmod +x "$SLOWCMD"
        command=(-c "$SLOWCMD")
        note "each job will take ${SLOW}s, so it can be watched in \`processing\`"
    fi

    ippeveprinter -p "$PORT" -d "$SPOOL" -M "Glimpse" -m "Mock Laser" -l "Test Lab" \
        "${command[@]}" "$NAME" >"$LOGFILE" 2>&1 &
    echo $! >"$PIDFILE"
    printf '%s\n' "$NAME" >"$NAMEFILE"

    sleep 1
    running || die "the emulator exited at once; see $LOGFILE"

    local queue
    queue=$(queue_name)
    note "published \"$NAME\" on port $PORT; waiting for cups to discover it"
    if ! await_queue "$queue"; then
        note "cups has not created a queue after ${DISCOVER_TIMEOUT}s"
        note "the emulator is still up; check \`avahi-browse -tr _ipp._tcp\` and cups-browsed"
        return 1
    fi
    note "cups discovered it as \"$queue\""
    status
}

jobs() {
    local count=${1:-3} queue
    queue=$(queue_name)
    lpstat -e 2>/dev/null | grep -qx "$queue" || die "no queue named $queue; run \`up\` first"

    local titles=("Quarterly report" "Invoice 2026-041" "Boarding pass" "Lease agreement.pdf"
        "photo-of-a-very-long-filename-that-should-ellipsize.jpg" "Meeting notes")
    local file="$STATE_DIR/printing-test-page.txt"
    printf 'glimpse printing test page\n' >"$file"

    local index
    for index in $(seq "$count"); do
        local title=${titles[$((job_seq % ${#titles[@]}))]}
        job_seq=$((job_seq + 1))
        lp -d "$queue" -H hold -t "$title" "$file" >/dev/null ||
            die "could not queue \"$title\""
        note "queued (held): $title"
    done
    [ "$interactive" -eq 1 ] || note "release one with: lp -i <job-id> -H resume"
}

status() {
    if running; then
        note "emulator up as pid $(cat "$PIDFILE") on port $PORT"
    else
        note "emulator is not running"
    fi
    local queue
    queue=$(queue_name)
    if lpstat -e 2>/dev/null | grep -qx "$queue"; then
        note "cups queue: $queue"
        lpstat -p "$queue" 2>/dev/null | head -1
        local pending
        pending=$(lpstat -o "$queue" 2>/dev/null | wc -l)
        note "$pending job(s) queued"
        lpstat -o "$queue" 2>/dev/null
    else
        note "cups has no queue for it"
    fi
}

down() {
    local queue
    queue=$(queue_name)
    if lpstat -e 2>/dev/null | grep -qx "$queue"; then
        cancel -a "$queue" 2>/dev/null && note "cancelled its jobs"
    fi
    if running; then
        kill "$(cat "$PIDFILE")" 2>/dev/null
        note "stopped the emulator"
    else
        note "the emulator was not running"
    fi
    rm -f "$PIDFILE" "$NAMEFILE" "$SLOWCMD"
    rm -rf "$SPOOL"
    note "the temporary queue disappears on its own once the advertisement stops"
}

# ---------------------------------------------------------------- simulated conditions

# `cupsdisable` answers client-error-not-found on a queue cups has discovered but never
# instantiated, and one job is what instantiates it. Done once, lazily, so the common path does not
# pay for it.
ensure_instantiated() {
    local queue=$1
    cupsdisable "$queue" 2>/dev/null && {
        cupsenable "$queue" 2>/dev/null
        return 0
    }
    local file="$STATE_DIR/printing-test-page.txt"
    printf 'glimpse printing test page\n' >"$file"
    lp -d "$queue" -H hold -t "Instantiating the queue" "$file" >/dev/null 2>&1 || return 1
    sleep 1
    cancel -a "$queue" 2>/dev/null
    sleep 1
}

newest_job() { lpstat -o 2>/dev/null | tail -1 | awk '{print $1}'; }

# ---------------------------------------------------------------- interactive

menu() {
    cat <<'MENU'

  j  add a held job            J  add a job and start printing it
  r  release the newest job    c  cancel the newest job    C  cancel every job
  p  paper jam (stop it)       e  resume it
  s  status                    ?  this menu                q  quit

MENU
}

run() {
    interactive=1
    [ "$SLOW" -eq 0 ] && SLOW=25
    up || return 1
    trap 'printf "\n"; down; exit 0' INT TERM

    local queue
    queue=$(queue_name)
    note "driving \"$queue\" — ^C or q tears it down"
    menu

    local choice
    while true; do
        printf 'printer> '
        read -r choice || break
        case $choice in
        j) jobs 1 ;;
        J)
            jobs 1
            local id
            id=$(newest_job)
            [ -n "$id" ] && lp -i "$id" -H resume 2>/dev/null &&
                note "printing $id for ${SLOW}s"
            ;;
        c)
            local id
            id=$(newest_job)
            [ -n "$id" ] && cancel "$id" && note "cancelled $id" || note "no job to cancel"
            ;;
        C) cancel -a "$queue" 2>/dev/null && note "cancelled every job" ;;
        r)
            local id
            id=$(newest_job)
            if [ -n "$id" ]; then
                lp -i "$id" -H resume 2>/dev/null &&
                    note "released $id — it prints for ${SLOW}s"
            else
                note "no job to release"
            fi
            ;;
        p)
            if ensure_instantiated "$queue"; then
                cupsdisable -r "Paper jam in tray 2" "$queue" 2>&1 &&
                    note "printer stopped (reasons: paused; the text is in printer-state-message)"
            else
                note "could not instantiate the queue, so it cannot be stopped"
            fi
            ;;
        e) cupsenable "$queue" 2>&1 && note "printer enabled" ;;
        s) status ;;
        "?") menu ;;
        q)
            down
            return 0
            ;;
        "") menu ;;
        *) note "unknown: $choice" ;;
        esac
    done
    down
}

args=("$@")
command=${1:-}
[ $# -gt 0 ] && shift
while [ $# -gt 0 ]; do
    case $1 in
    --name)
        NAME=${2:?--name needs a value}
        named=1
        shift 2
        ;;
    --port)
        PORT=${2:?--port needs a number}
        shift 2
        ;;
    --slow)
        SLOW=${2:?--slow needs a number of seconds}
        shift 2
        ;;
    [0-9]*) break ;;
    *) die "unknown option: $1" ;;
    esac
done

case $command in
run) run ;;
up) up ;;
jobs)
    adopt_published_name
    jobs "${1:-3}"
    ;;
status)
    adopt_published_name
    status
    ;;
down)
    adopt_published_name
    down
    ;;
*) die "usage: printing-network-printer.sh run|up [--name NAME] [--port N] [--slow N] | jobs [N] | status | down" ;;
esac

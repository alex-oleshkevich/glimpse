#!/usr/bin/env bash
set -euo pipefail

if [[ ${1:-} == --help ]]; then
    printf '%s\n' 'Usage: scripts/perf-baseline.sh [--json]' 'Print PSS, swap, anon, threads, fd count and CPU time of every running process whose comm starts with "glimpse".' 'With --json, emit one JSON object per process instead of the text table.'
    exit 0
fi

json_mode=0
if [[ ${1:-} == --json ]]; then
    json_mode=1
fi

hz=$(getconf CLK_TCK 2>/dev/null || printf '100')
read -r UPTIME _ </proc/uptime
UPTIME=${UPTIME%.*}

snapshot() {
    local pid=$1 proc=/proc/$1
    local comm pss swap anon threads fds stat
    local state ppid pgrp session tty_nr tpgid flags minflt cminflt majflt cmajflt
    local utime stime cutime cstime priority nice num_threads itrealvalue starttime

    comm=$(cat "$proc/comm" 2>/dev/null) || return 1
    [[ $comm == glimpse* ]] || return 1
    pss=$(awk '$1 == "Pss:" { print $2; exit }' "$proc/smaps_rollup" 2>/dev/null) || return 1
    swap=$(awk '$1 == "Swap:" { print $2; exit }' "$proc/smaps_rollup" 2>/dev/null) || return 1
    anon=$(awk '$1 == "Pss_Anon:" { print $2; exit }' "$proc/smaps_rollup" 2>/dev/null) || return 1
    threads=$(awk '$1 == "Threads:" { print $2; exit }' "$proc/status" 2>/dev/null) || return 1
    fds=$(find "$proc/fd" -mindepth 1 -maxdepth 1 2>/dev/null | wc -l) || return 1
    stat=$(cat "$proc/stat" 2>/dev/null) || return 1

    stat=${stat##*\) }
    read -r state ppid pgrp session tty_nr tpgid flags minflt cminflt majflt cmajflt \
        utime stime cutime cstime priority nice num_threads itrealvalue starttime _ <<<"$stat" || return 1

    pss=${pss:-0}; swap=${swap:-0}; anon=${anon:-0}
    threads=${threads:-0}; fds=${fds:-0}; utime=${utime:-0}; stime=${stime:-0}; starttime=${starttime:-0}

    local cpu_ms uptime_s
    cpu_ms=$(((utime + stime) * 1000 / hz))
    uptime_s=$((UPTIME - starttime / hz))

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$comm" "$pid" "$pss" "$swap" "$anon" "$threads" "$fds" "$cpu_ms" "$uptime_s"
}

rows=()
for proc in /proc/[0-9]*; do
    if rec=$(snapshot "${proc#/proc/}"); then
        rows+=("$rec")
    fi
done
mapfile -t rows < <(printf '%s\n' "${rows[@]}" | sort -t $'\t' -k2,2n)

humankb() {
    awk -v k="$1" 'BEGIN { if (k >= 1024) printf "%.1fM", k / 1024; else printf "%dK", k }'
}

humans() {
    awk -v t="$1" 'BEGIN { if (t < 60) printf "%.1fs", t; else if (t < 3600) printf "%.0fm", t / 60; else printf "%.1fh", t / 3600 }'
}

if (( json_mode )); then
    {
        for row in "${rows[@]}"; do
            IFS=$'\t' read -r comp pid pss swap anon threads fds cpu_ms _ <<<"$row"
            printf '{"component": %s, "pid": %s, "pss_kb": %s, "swap_kb": %s, "anon_kb": %s, "threads": %s, "fds": %s, "cpu_ms": %s}\n' \
                "$(jq -n --arg c "$comp" '$c')" "$pid" "$pss" "$swap" "$anon" "$threads" "$fds" "$cpu_ms"
        done
    } | jq -s 'sort_by(.pid)'
    exit 0
fi

printf '%-20s %9s %7s %9s %8s %9s %9s\n' COMPONENT PSS+SWAP SWAP ANON THREADS CPU UPTIME
for row in "${rows[@]}"; do
    IFS=$'\t' read -r comp pid pss swap anon threads fds cpu_ms uptime_s <<<"$row"
    printf '%-20s %9s %7s %9s %8s %9s %9s\n' \
        "$comp" \
        "$(humankb "$((pss + swap))")" \
        "$(humankb "$swap")" \
        "$(humankb "$anon")" \
        "$threads" \
        "$(humans "$(awk -v ms="$cpu_ms" 'BEGIN { print ms / 1000 }')")" \
        "$(humans "$uptime_s")"
done

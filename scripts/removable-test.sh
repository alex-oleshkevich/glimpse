#!/usr/bin/env bash
# Attaches a fake removable drive so the removable applet can be exercised without hardware.
#
# The applet lists a drive only when UDisks2 reports it removable, and nothing on this machine is. A
# loop device does not help: it has NO `Drive` object at all (`Block.Drive` is `/`) and `HintSystem`
# is true, so `udisksctl loop-setup` exercises UDisks2 and never reaches the applet. `scsi_debug
# removable=1` is the one route that produces a real SCSI drive with `Removable` set, which is why
# this needs root.
#
# The drive is backed by kernel RAM, so `down` takes the whole thing with it and nothing survives a
# reboot. Nothing here ever touches a device it did not create: every destructive step re-checks
# that the target's SCSI model is `scsi_debug` first.
#
# It becomes root with `sudo`, matching the justfile's own `elevate`, because no polkit agent is
# registered on this machine: pkexec's textual fallback authenticates and then fails with "No
# session for cookie". Set GLIMPSE_SUDO to something else from a launcher with no terminal.
#
#   scripts/removable-test.sh up [options]   attach and format a fake drive
#   scripts/removable-test.sh down           detach it
#   scripts/removable-test.sh status         what is attached right now
#
# Options for `up`:
#   --parts N     partitions on the drive (default 2; 0 is a drive with no volumes)
#   --size MB     total size of the drive (default 128)
#   --optical     a CD-ROM with no media instead of a disk, for the no-media state
#   --readonly    mark every partition read-only, for the read-only trailing icon
set -uo pipefail

MODEL="scsi_debug"
PARTS=2
SIZE=128
PTYPE=0
READONLY=0

die() {
    printf 'removable-test: %s\n' "$1" >&2
    exit 1
}
note() { printf 'removable-test: %s\n' "$1"; }

loaded() { [ -d /sys/module/scsi_debug ]; }

model_of() {
    local name=${1##*/} model
    [ -r "/sys/block/$name/device/model" ] || return 1
    read -r model <"/sys/block/$name/device/model"
    printf '%s\n' "$model"
}

is_fake() { [ "$(model_of "$1" 2>/dev/null)" = "$MODEL" ]; }

fake_disks() {
    local dir
    for dir in /sys/block/sd* /sys/block/sr*; do
        [ -d "$dir" ] || continue
        is_fake "$dir" && printf '/dev/%s\n' "${dir##*/}"
    done
}

partitions_of() {
    local disk=${1##*/} dir
    for dir in "/sys/block/$disk/$disk"*; do
        [ -d "$dir" ] || continue
        printf '/dev/%s\n' "${dir##*/}"
    done
}

await_partitions() {
    local disk=$1 try
    for try in $(seq 20); do
        [ -n "$(partitions_of "$disk")" ] && return 0
        sleep 0.25
    done
    return 1
}

format() {
    local disk=$1 part index=0
    while read -r part; do
        index=$((index + 1))
        if [ $((index % 2)) -eq 1 ]; then
            mkfs.ext4 -q -F -L "Photos" "$part" || die "could not make ext4 on $part"
            note "$part: ext4, labelled Photos"
        else
            command -v mkfs.exfat >/dev/null || die "mkfs.exfat is missing; install exfatprogs"
            mkfs.exfat "$part" >/dev/null || die "could not make exfat on $part"
            note "$part: exfat, deliberately unlabelled"
        fi
    done < <(partitions_of "$disk")
}

up() {
    loaded && die "already attached; run \`down\` first"

    modprobe scsi_debug \
        dev_size_mb="$SIZE" \
        num_parts="$PARTS" \
        ptype="$PTYPE" \
        removable=1 || die "could not load scsi_debug"
    udevadm settle

    local disks
    mapfile -t disks < <(fake_disks)
    [ "${#disks[@]}" -gt 0 ] || die "scsi_debug loaded but no block device appeared"

    local disk=${disks[0]}
    is_fake "$disk" || die "refusing to touch $disk: its model is not $MODEL"

    if [ "$PTYPE" -eq 0 ] && [ "$PARTS" -gt 0 ]; then
        await_partitions "$disk" || die "$disk never exposed a partition"
        format "$disk"
        udevadm settle
    fi

    if [ "$READONLY" -eq 1 ]; then
        local part
        while read -r part; do
            blockdev --setro "$part" && note "$part: read-only"
        done < <(partitions_of "$disk")
    fi

    note "attached $disk"
    status
}

down() {
    loaded || die "nothing attached"

    local disk part
    while read -r disk; do
        while read -r part; do
            findmnt -no TARGET "$part" >/dev/null 2>&1 || continue
            umount "$part" && note "unmounted $part"
        done < <(partitions_of "$disk")
    done < <(fake_disks)

    udevadm settle
    modprobe -r scsi_debug || die "could not unload scsi_debug; something still holds it"
    note "detached"
}

status() {
    loaded || {
        note "nothing attached"
        return
    }
    local disk found=0
    while read -r disk; do
        found=1
        lsblk -o NAME,SIZE,RM,RO,FSTYPE,LABEL,MOUNTPOINT "$disk"
    done < <(fake_disks)
    [ "$found" -eq 1 ] || note "scsi_debug is loaded but exposes no block device"
}

args=("$@")
command=${1:-}
[ $# -gt 0 ] && shift
while [ $# -gt 0 ]; do
    case $1 in
    --parts)
        PARTS=${2:?--parts needs a number}
        shift 2
        ;;
    --size)
        SIZE=${2:?--size needs a number of MB}
        shift 2
        ;;
    --optical)
        PTYPE=5
        PARTS=0
        shift
        ;;
    --readonly)
        READONLY=1
        shift
        ;;
    *) die "unknown option: $1" ;;
    esac
done

elevate() {
    [ "$(id -u)" -eq 0 ] && return 0
    local as=${GLIMPSE_SUDO:-sudo}
    command -v "$as" >/dev/null || die "$as is missing; run this as root"
    exec "$as" "$(realpath "$0")" "${args[@]}"
}

case $command in
up)
    elevate
    up
    ;;
down)
    elevate
    down
    ;;
status) status ;;
*) die "usage: removable-test.sh up [--parts N] [--size MB] [--optical] [--readonly] | down | status" ;;
esac

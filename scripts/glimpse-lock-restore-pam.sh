#!/usr/bin/env bash
set -euo pipefail

backup=/etc/pam.d/glimpse-lock.glimpse-rescue.bak
target=/etc/pam.d/glimpse-lock

if [[ ! -e "$backup" ]]; then
  echo "Backup not found: $backup; glimpse-lock-rescue-pam.sh writes it" >&2
  exit 1
fi

sudo cp -a "$backup" "$target"
sudo rm -f "$backup"

echo "Restored $target from $backup"
ls -l "$target"

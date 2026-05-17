#!/usr/bin/env bash
set -euo pipefail

backup=/etc/pam.d/glimpse-lock.glimpse-rescue.bak
target=/etc/pam.d/glimpse-lock

if [[ ! -e "$backup" ]]; then
  echo "Backup not found: $backup" >&2
  exit 1
fi

sudo cp -a "$backup" "$target"
sudo chmod 0644 "$target"
sudo chown root:root "$target"

echo "Restored $target from $backup"
ls -l "$target"

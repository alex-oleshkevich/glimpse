#!/usr/bin/env bash
set -euo pipefail

backup=/etc/pam.d/glimpse-lock.glimpse-rescue.bak
target=/etc/pam.d/glimpse-lock
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT

if [[ ! -e "$backup" ]]; then
  sudo cp -a "$target" "$backup"
fi

cat >"$tmp" <<'PAM'
#%PAM-1.0

auth      required   pam_permit.so
account   required   pam_permit.so
password  required   pam_permit.so
session   required   pam_permit.so
PAM

sudo install -o root -g root -m 0644 "$tmp" "$target"
sudo faillock --user alex --reset || true

echo "Temporary pam_permit rescue installed for glimpse-lock."
echo "Unlock now, then restore the real PAM file with:"
echo "  sudo cp -a $backup $target"

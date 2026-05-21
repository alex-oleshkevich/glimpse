#!/usr/bin/env bash
set -euo pipefail

WIRED_IF="${GLIMPSE_TEST_WIRED_IF:-veth-glimpse}"
WIRED_PEER="${GLIMPSE_TEST_WIRED_PEER:-vethg-peer}"
WIRED_CON="${GLIMPSE_TEST_WIRED_CON:-Glimpse Test Wired}"
WIRED_ADDR="${GLIMPSE_TEST_WIRED_ADDR:-192.0.2.10/24}"

VPN_IF="${GLIMPSE_TEST_VPN_IF:-wg-glimpse}"
VPN_CON="${GLIMPSE_TEST_VPN_CON:-Glimpse Test VPN}"
VPN_ADDR="${GLIMPSE_TEST_VPN_ADDR:-10.64.0.2/32}"
VPN_UP="${GLIMPSE_TEST_VPN_UP:-1}"

cleanup() {
  set +e
  echo
  echo "Cleaning network test fixtures..."

  sudo nmcli con down "$VPN_CON" >/dev/null 2>&1
  sudo nmcli con delete "$VPN_CON" >/dev/null 2>&1

  sudo nmcli con down "$WIRED_CON" >/dev/null 2>&1
  sudo nmcli con delete "$WIRED_CON" >/dev/null 2>&1

  sudo ip link delete "$WIRED_IF" >/dev/null 2>&1
  sudo ip link delete "$VPN_IF" >/dev/null 2>&1

  echo "Cleanup complete."
}

on_exit() {
  status=$?
  trap - EXIT INT TERM
  cleanup
  exit "$status"
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}

validate_ifname() {
  local name="$1"
  local label="$2"

  if ((${#name} > 15)); then
    echo "$label interface name is too long for Linux: $name" >&2
    echo "Use 15 characters or fewer." >&2
    exit 1
  fi
}

delete_stale_fixtures() {
  sudo nmcli con down "$VPN_CON" >/dev/null 2>&1 || true
  sudo nmcli con delete "$VPN_CON" >/dev/null 2>&1 || true
  sudo nmcli con down "$WIRED_CON" >/dev/null 2>&1 || true
  sudo nmcli con delete "$WIRED_CON" >/dev/null 2>&1 || true
  sudo ip link delete "$WIRED_IF" >/dev/null 2>&1 || true
  sudo ip link delete "$VPN_IF" >/dev/null 2>&1 || true
}

print_state() {
  echo
  echo "NetworkManager devices:"
  nmcli -f DEVICE,TYPE,STATE,CONNECTION dev
  echo
  echo "Glimpse test connections:"
  nmcli -f NAME,TYPE,DEVICE,STATE con show --active | grep -E "^(${WIRED_CON}|${VPN_CON})[[:space:]]" || true
  nmcli -f NAME,TYPE con show | grep -E "^(${WIRED_CON}|${VPN_CON})[[:space:]]" || true
}

setup_wired() {
  echo "Creating virtual wired pair: $WIRED_IF <-> $WIRED_PEER"
  sudo ip link add "$WIRED_IF" type veth peer name "$WIRED_PEER"
  sudo ip link set "$WIRED_PEER" up
  sudo ip link set "$WIRED_IF" up

  sudo nmcli dev set "$WIRED_IF" managed yes
  sudo nmcli con add \
    type ethernet \
    ifname "$WIRED_IF" \
    con-name "$WIRED_CON" \
    ipv4.method manual \
    ipv4.addresses "$WIRED_ADDR" \
    ipv6.method disabled \
    autoconnect no
  sudo nmcli con up "$WIRED_CON"
}

setup_vpn() {
  local private_key

  echo "Creating temporary WireGuard VPN profile: $VPN_CON"
  private_key="$(wg genkey)"
  sudo nmcli con add \
    type wireguard \
    ifname "$VPN_IF" \
    con-name "$VPN_CON" \
    ipv4.method manual \
    ipv4.addresses "$VPN_ADDR" \
    ipv6.method disabled \
    wireguard.private-key "$private_key" \
    autoconnect no

  if [[ "$VPN_UP" == "1" ]]; then
    if ! sudo nmcli con up "$VPN_CON"; then
      echo "Warning: failed to activate $VPN_CON; leaving saved VPN profile for inactive-state testing." >&2
    fi
  fi
}

wait_forever() {
  echo
  echo "Fixtures are active. Run Glimpse now and inspect the Network popover."
  echo "Press Ctrl-C to remove the test fixtures."
  while true; do
    sleep 86400 &
    wait "$!"
  done
}

require_command ip
require_command nmcli
require_command wg
validate_ifname "$WIRED_IF" "Wired"
validate_ifname "$WIRED_PEER" "Wired peer"
validate_ifname "$VPN_IF" "VPN"
sudo -v
nmcli general status >/dev/null

trap on_exit EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

delete_stale_fixtures
setup_wired
setup_vpn
print_state
wait_forever

# Sourced by scripts/install.sh and scripts/uninstall.sh. The two must agree on every
# destination or an uninstall leaves files behind, which is why this is not duplicated.

prefix="${PREFIX:-/usr}"
destdir="${DESTDIR:-}"

bindir="$destdir$prefix/bin"
unitdir="$destdir$prefix/lib/systemd/user"
dbusdir="$destdir$prefix/share/dbus-1/services"
pamdir="$destdir/etc/pam.d"
geocluedir="$destdir/etc/geoclue/conf.d"
sharedir="$destdir$prefix/share/glimpse"

# GLIMPSE_BINARIES is set by the justfile from its single source of truth; the fallback
# here only matters for a direct, non-just invocation.
read -ra binaries <<< "${GLIMPSE_BINARIES:-glimpsectl glimpsed glimpse-panel glimpse-lock glimpse-wallpaper glimpse-sunset}"

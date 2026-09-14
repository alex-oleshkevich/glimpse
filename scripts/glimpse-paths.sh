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
# Catalogs go to the shared locale tree, not under sharedir: gettext resolves a domain by
# scanning <localedir>/<lang>/LC_MESSAGES. glimpse-utils/build.rs reads the same PREFIX to bake
# its default, so the two agree without a second variable to keep in step.
localedir="$destdir$prefix/share/locale"

# GLIMPSE_BINARIES is set by the justfile from its single source of truth; the fallback
# here only matters for a direct, non-just invocation.
read -ra binaries <<< "${GLIMPSE_BINARIES:-glimpsectl glimpsed glimpse-panel glimpse-lock glimpse-wallpaper glimpse-sunset glimpse-notifications glimpse-weather}"

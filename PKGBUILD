pkgname=glimpse-desktop-bin
pkgver=0.16.0
pkgrel=1
pkgdesc="Desktop shell suite for Wayland compositors: panel, wallpaper, lock screen and night light"
arch=('x86_64')
url="https://github.com/alex-oleshkevich/glimpse"
license=('BSD-3-Clause')
depends=('gtk4' 'libadwaita' 'gtk4-layer-shell' 'geoclue' 'libpulse' 'pam')
optdepends=(
    'upower: battery level and peripheral charge'
    'power-profiles-daemon: power modes in the battery popover'
    'networkmanager: the network applet'
    'bluez: the bluetooth applet'
    'udisks2: the removable-media applet'
    'cups: the printing applet'
    'kdeconnect: the phone applet'
    'ddcutil: its udev rule lets the brightness applet reach external displays'
    'xdg-desktop-portal: routes app inhibit requests to glimpse-idle'
)
# cargo already strips (profile.release strip = true), so splitting debug symbols yields a
# package of nothing but .build-id links that pacman will not remove with its parent.
options=('!debug')
backup=('etc/geoclue/conf.d/glimpse.conf' 'etc/pam.d/glimpse-lock')
provides=('glimpse-desktop')
conflicts=('glimpse-desktop')
source_x86_64=("glimpse-$pkgver-x86_64.tar.zst::$url/releases/download/v$pkgver/glimpse-$pkgver-x86_64.tar.zst")
b2sums_x86_64=('SKIP')

package() {
    cp -a "$srcdir/usr" "$pkgdir/"
    if [[ -d "$srcdir/etc" ]]; then
        cp -a "$srcdir/etc" "$pkgdir/"
    fi
    install -Dm644 "$srcdir/usr/share/glimpse/LICENSE" "$pkgdir/usr/share/licenses/$pkgname/LICENSE"
}

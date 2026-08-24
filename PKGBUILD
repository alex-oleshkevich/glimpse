pkgname=glimpse-desktop-bin
pkgver=0.16.0
pkgrel=1
pkgdesc="Desktop shell suite for Wayland compositors: panel, wallpaper renderer, lock screen, and night-light service, backed by one daemon"
arch=('x86_64')
url="https://github.com/alex-oleshkevich/glimpse"
license=('BSD-3-Clause')
depends=('gtk4' 'libadwaita' 'gtk4-layer-shell' 'libheif' 'pam' 'geoclue')
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

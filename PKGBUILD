# Maintainer: Alex Oleshkevich <alex@oleshevich.com>
  pkgname=glimpse
  pkgver=0.1.0
  pkgrel=1
  pkgdesc="Linux launcher"
  arch=('x86_64')
  url="https://github.com/alex-oleshkevich/glimpse"
  license=('MIT')
  depends=(
      'gtk3'
      'glib2'
      'wl-clipboard'
      'xdg-utils'
  )
  makedepends=(
      'rust'
      'cargo'
      'git'
  )
  optdepends=(
      'gtk-launch: for launching desktop files'
  )
  provides=('glimpse')
  conflicts=('glimpse')
  source=("git+file://${PWD}")
  sha256sums=('SKIP')
  install=install/PKGBUILD.install

  build() {
      cd "${srcdir}"
      cargo build --release --workspace

      cd "${srcdir}/glimpse-gui"
      flutter build linux --release
  }

  package() {
      cd "${srcdir}/${pkgname}"

      # Install daemon
      install -Dm755 "target/release/glimpsed" \
          "${pkgdir}/usr/bin/glimpsed"

      # Install plugins
      install -dm755 "${pkgdir}/usr/share/glimpse/plugins"
      install -Dm755 "target/release/glimpse-plugins-apps" \
          "${pkgdir}/usr/share/glimpse/plugins/glimpse-plugins-apps"
      install -Dm755 "target/release/glimpse-plugins-calculator" \
          "${pkgdir}/usr/share/glimpse/plugins/glimpse-plugins-calculator"

      # Install Flutter GUI bundle
      cd glimpse-gui/build/linux/x64/release/bundle
      install -Dm755 "glimpse" "${pkgdir}/usr/bin/glimpse"
      install -dm755 "${pkgdir}/usr/share/glimpse/gui"
      cp -r data "${pkgdir}/usr/share/glimpse/gui/"
      cp -r lib "${pkgdir}/usr/share/glimpse/gui/"

      cd "${srcdir}/${pkgname}"

      # Install icon
      install -Dm644 "install/glimpse.png" \
          "${pkgdir}/usr/share/icons/hicolor/256x256/apps/glimpse.png"

      # Install desktop file
      install -Dm644 "install/glimpse.desktop" "${pkgdir}/usr/share/applications/glimpse.desktop"

      # Install systemd user service
      install -Dm644 "install/glimpsed.service" \
          "${pkgdir}/usr/lib/systemd/user/glimpsed.service"

      # Install license if exists
      if [ -f LICENSE ]; then
          install -Dm644 LICENSE "${pkgdir}/usr/share/licenses/${pkgname}/LICENSE"
      fi

      install -Dm644 "install/me.aresa.glimpse.gschema.xml" "${pkgdir}/usr/share/glib-2.0/schemas/me.aresa.glimpse.gschema.xml"
  }

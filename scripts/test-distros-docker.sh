#!/usr/bin/env bash
set -euo pipefail
shopt -s nullglob

if [[ "${1:-}" != --inside ]]; then
    root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
    (cd "$root/dist" && sha256sum -c SHA256SUMS)
    for distro in arch debian ubuntu fedora opensuse; do
        case "$distro" in
            arch) image='archlinux@sha256:8745817f349ed24373341ddb92776209eeec3f0364ea48f7f645ac5800d30a50' ;;
            debian) image='debian@sha256:7bb96d9b4ccd7f46f54853c258a56df0c6b628a63e65c2317959da18fd36932c' ;;
            ubuntu) image='ubuntu@sha256:da6fc2be547864451aa253836dd926da33623312df4a9a243e35dc877c378a78' ;;
            fedora) image='fedora@sha256:43b29f65a41eb9c35e1cd5323e3bdf3b655c2357a9f4f1ff2f9c2798e5045d80' ;;
            opensuse) image='opensuse/tumbleweed@sha256:007fde2c1e6b3de48681c765b3d6905d4e59d2f7a26f4585ec49dd11305c3fe7' ;;
        esac
        printf '==> %s\n' "$distro"
        docker run --rm \
            --mount "type=bind,src=$root/dist,dst=/artifacts,readonly" \
            --mount "type=bind,src=$root/scripts/test-distros-docker.sh,dst=/test.sh,readonly" \
            "$image" bash /test.sh --inside "$distro"
    done
    exit 0
fi

distro="${2:?distro required}"
package=()
mkdir -p /root/.config/glimpse
printf '[appearance]\ntheme = "adwaita"\n' > /root/.config/glimpse/config.toml
cp /root/.config/glimpse/config.toml /tmp/glimpse-config-before

case "$distro" in
    arch)
        tarball=(/artifacts/glimpse-*-x86_64.tar.zst)
        [[ ${#tarball[@]} -eq 1 && -f /artifacts/PKGBUILD ]]
        pacman -Syu --noconfirm
        useradd -m builder
        mkdir -p /tmp/aur
        cp "${tarball[0]}" /artifacts/PKGBUILD /tmp/aur/
        chown -R builder:builder /tmp/aur
        su builder -c 'cd /tmp/aur && makepkg --force --nodeps --noconfirm'
        package=(/tmp/aur/glimpse-desktop-bin-*.pkg.tar.zst)
        [[ ${#package[@]} -eq 1 ]]
        pacman -U --noconfirm --config <(grep -v '^NoExtract' /etc/pacman.conf) "${package[0]}"
        ;;
    debian|ubuntu)
        package=(/artifacts/*.deb)
        [[ ${#package[@]} -eq 1 ]]
        apt-get update
        DEBIAN_FRONTEND=noninteractive apt-get -o 'Dpkg::Options::=--path-include=/usr/share/locale/*' install -y "${package[0]}"
        ;;
    fedora)
        for file in /artifacts/*.rpm; do
            [[ "$file" == *opensuse* ]] || package=("$file")
        done
        [[ ${#package[@]} -eq 1 ]]
        dnf install -y "${package[0]}"
        ;;
    opensuse)
        package=(/artifacts/*opensuse*.rpm)
        [[ ${#package[@]} -eq 1 ]]
        zypper --non-interactive install --no-recommends diffutils
        zypper --non-interactive install --no-recommends --allow-unsigned-rpm "${package[0]}"
        ;;
esac

for binary in glimpsectl glimpse-panel glimpse-lock glimpse-wallpaper glimpse-sunset glimpse-notifications glimpse-weather glimpse-idle glimpse-lens glimpse-shot; do
    test -x "/usr/bin/$binary"
    "/usr/bin/$binary" --version >/dev/null
    if ldd "/usr/bin/$binary" | grep -q 'not found'; then
        printf 'missing shared library: %s\n' "$binary" >&2
        exit 1
    fi
done

for file in /usr/bin/glimpse-dpms /usr/share/glimpse/config.default.toml /usr/share/glimpse/config.commented.toml /usr/share/glimpse/config.schema.json /usr/share/glimpse/themes/adwaita/default.jpg /usr/share/glimpse/sdk/template/index.tsx /usr/share/locale/ru/LC_MESSAGES/glimpse.mo /usr/lib/systemd/user/glimpse-session.target /usr/share/dbus-1/services/me.aresa.Glimpse.Idle.service /usr/share/xdg-desktop-portal/portals/glimpse.portal /etc/geoclue/conf.d/glimpse.conf /etc/pam.d/glimpse-lock; do
    test -e "$file"
done

case "$distro" in
    arch) grep -q system-auth /etc/pam.d/glimpse-lock ;;
    debian|ubuntu|opensuse) grep -q common-auth /etc/pam.d/glimpse-lock ;;
    fedora) grep -q password-auth /etc/pam.d/glimpse-lock ;;
esac

if command -v systemd-analyze >/dev/null; then
    systemd-analyze verify /usr/lib/systemd/user/glimpse-*.service /usr/lib/systemd/user/glimpse-session.target
fi

case "$distro" in
    arch) pacman -R --noconfirm glimpse-desktop-bin ;;
    debian|ubuntu) DEBIAN_FRONTEND=noninteractive apt-get remove -y glimpse ;;
    fedora) dnf remove -y glimpse ;;
    opensuse) zypper --non-interactive remove glimpse ;;
esac

for file in /usr/bin/glimpsectl /usr/bin/glimpse-panel /usr/bin/glimpse-lock /usr/bin/glimpse-dpms /usr/lib/systemd/user/glimpse-session.target /usr/share/dbus-1/services/me.aresa.Glimpse.Idle.service /usr/share/xdg-desktop-portal/portals/glimpse.portal /usr/share/glimpse/config.default.toml; do
    test ! -e "$file"
done
cmp /tmp/glimpse-config-before /root/.config/glimpse/config.toml

case "$distro" in
    arch) pacman -U --noconfirm --config <(grep -v '^NoExtract' /etc/pacman.conf) "${package[0]}" ;;
    debian|ubuntu) DEBIAN_FRONTEND=noninteractive apt-get -o 'Dpkg::Options::=--path-include=/usr/share/locale/*' install -y "${package[0]}" ;;
    fedora) dnf install -y "${package[0]}" ;;
    opensuse) zypper --non-interactive install --no-recommends --allow-unsigned-rpm "${package[0]}" ;;
esac

test -x /usr/bin/glimpse-panel
cmp /tmp/glimpse-config-before /root/.config/glimpse/config.toml

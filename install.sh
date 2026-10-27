#!/usr/bin/env bash
# curl -fsSL https://raw.githubusercontent.com/alex-oleshkevich/glimpse/master/install.sh | bash
set -euo pipefail

repo="alex-oleshkevich/glimpse"
tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

as_root() {
    if [[ "$(id -u)" -eq 0 ]]; then
        "$@"
    else
        sudo "$@"
    fi
}

echo "Fetching latest glimpse release..."
release_json="$tmpdir/release.json"
curl -fsSL "https://api.github.com/repos/$repo/releases/latest" -o "$release_json"

tag="$(sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' "$release_json" | head -1)"
if [[ -z "$tag" ]]; then
    echo "error: could not determine the latest release tag" >&2
    exit 1
fi
echo "Latest release: $tag"

# Discovers the download URL for the one release asset matching a pattern, rather than
# guessing cargo-deb's/cargo-generate-rpm's exact output filename (arch suffix, separators).
download_matching() {
    local pattern="$1" url name
    url="$(sed -n 's/.*"browser_download_url": *"\([^"]*\)".*/\1/p' "$release_json" | grep -E "$pattern" | head -1)"
    if [[ -z "$url" ]]; then
        echo "error: release $tag has no asset matching $pattern" >&2
        exit 1
    fi
    name="$(basename "$url")"
    curl -fsSL "$url" -o "$tmpdir/$name"
    echo "$name"
}

verify() {
    local name="$1" expected actual
    expected="$(awk -v f="$name" '$2 == f { print $1; exit }' "$tmpdir/SHA256SUMS")"
    if [[ -z "$expected" ]]; then
        echo "error: SHA256SUMS has no entry for $name" >&2
        exit 1
    fi
    actual="$(sha256sum "$tmpdir/$name" | cut -d' ' -f1)"
    if [[ "$expected" != "$actual" ]]; then
        echo "error: checksum verification failed for $name" >&2
        exit 1
    fi
}

id="" id_like="" version=""
if [[ -r /etc/os-release ]]; then
    # shellcheck disable=SC1091
    . /etc/os-release
    id="${ID:-}"
    id_like="${ID_LIKE:-}"
    version="${VERSION_ID:-}"
fi

at_least() {
    [[ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | head -1)" == "$2" ]]
}

case "$id" in
    debian) [[ -z "$version" ]] || at_least "$version" 14 ;;
    ubuntu) at_least "$version" 26.04 ;;
    fedora) at_least "$version" 44 ;;
    opensuse-leap|sles) false ;;
esac || {
    echo "error: ${PRETTY_NAME:-$id $version} is not supported: glimpse needs GTK 4.22 and libadwaita 1.9, first shipped in Debian 14 (testing), Ubuntu 26.04, Fedora 44 and openSUSE Tumbleweed" >&2
    exit 1
}

case " $id $id_like " in
    *" nixos "*)
        echo "NixOS uses the Glimpse flake module: import nixosModules.default and set services.glimpse.enable = true." >&2
        exit 1
        ;;
    *" arch "*)
        echo "Arch Linux detected — install via the AUR instead of running this script as root:"
        echo "  yay -S glimpse-desktop-bin"
        exit 0
        ;;
    *" debian "*|*" ubuntu "*)
        download_matching 'SHA256SUMS$' >/dev/null
        deb="$(download_matching '\.deb$')"
        verify "$deb"
        as_root apt-get install -y "$tmpdir/$deb"
        ;;
    *" opensuse "*|*" suse "*)
        download_matching 'SHA256SUMS$' >/dev/null
        rpm="$(download_matching 'opensuse.*\.rpm$')"
        verify "$rpm"
        as_root zypper --non-interactive install --no-recommends --allow-unsigned-rpm "$tmpdir/$rpm"
        ;;
    *" fedora "*|*" rhel "*)
        download_matching 'SHA256SUMS$' >/dev/null
        rpm="$(download_matching '/glimpse-[^/]+-[0-9]+\.x86_64\.rpm$')"
        verify "$rpm"
        as_root dnf install -y "$tmpdir/$rpm"
        ;;
    *)
        echo "No native package for this distro — falling back to the binary tarball."
        download_matching 'SHA256SUMS$' >/dev/null
        tarball="$(download_matching 'x86_64\.tar\.zst$')"
        verify "$tarball"
        tar --zstd -tf "$tmpdir/$tarball" > "$tmpdir/members"
        touch "$tmpdir/preserved"
        excludes=()
        while IFS= read -r member; do
            config="${member#./}"
            case "$config" in
                etc/pam.d/glimpse-lock|etc/geoclue/conf.d/glimpse.conf|etc/xdg-desktop-portal/glimpse-portals.conf|etc/xdg-desktop-portal/niri-portals.conf|etc/xdg-desktop-portal/hyprland-portals.conf)
                    if [[ -e "/$config" || -L "/$config" ]]; then
                        if [[ "$config" == etc/geoclue/conf.d/glimpse.conf && -f "/$config" && ! -L "/$config" ]]; then
                            existing="$(as_root sha256sum -- "/$config")"
                            if [[ "${existing%% *}" == db86a0fe5a71457f26f5175d97992e7d2405dc42ae977c50cdac8bdbc840e6e7 ]]; then
                                echo "Installing the packaged GeoClue policy over the unchanged 0.15 default."
                                continue
                            fi
                        fi
                        printf '%s\n' "$member" >> "$tmpdir/preserved"
                        excludes+=("--exclude=$member")
                    fi
                    ;;
            esac
        done < "$tmpdir/members"
        if [[ -s "$tmpdir/preserved" ]]; then
            mkdir "$tmpdir/defaults"
            tar --zstd --no-same-owner -xf "$tmpdir/$tarball" -C "$tmpdir/defaults" -T "$tmpdir/preserved"
        fi
        as_root tar --zstd --no-same-owner --no-overwrite-dir "${excludes[@]}" -xf "$tmpdir/$tarball" -C /
        while IFS= read -r member; do
            config="${member#./}"
            incoming="$tmpdir/defaults/$config"
            if ! as_root cmp -s -- "/$config" "$incoming"; then
                as_root install -m 644 -- "$incoming" "/$config.new"
                echo "Kept /$config; review the new packaged default at /$config.new."
            fi
        done < "$tmpdir/preserved"
        for obsolete in usr/bin/glimpse-shell \
            usr/lib/systemd/user/glimpse-shell.service \
            usr/share/dbus-1/services/me.aresa.GlimpseIdle.Portal.service \
            usr/share/glimpse/scripts/monitors; do
            if ! grep -Fxq -e "$obsolete" -e "./$obsolete" "$tmpdir/members"; then
                as_root rm -f -- "/$obsolete"
            fi
        done
        if [[ -f /LICENSE && ! -L /LICENSE ]] && ! grep -Fxq -e LICENSE -e ./LICENSE "$tmpdir/members"; then
            existing="$(as_root sha256sum -- /LICENSE)"
            if [[ "${existing%% *}" == 2b44ad7005d10c7e4cae1318a26b7ce11a7eecd83997a2d722283e44d588a68f ]]; then
                as_root rm -f -- /LICENSE
            fi
        fi
        ;;
esac

echo "glimpse $tag installed."

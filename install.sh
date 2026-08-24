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

id="" id_like=""
if [[ -r /etc/os-release ]]; then
    # shellcheck disable=SC1091
    . /etc/os-release
    id="${ID:-}"
    id_like="${ID_LIKE:-}"
fi

case " $id $id_like " in
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
    *" fedora "*|*" rhel "*)
        download_matching 'SHA256SUMS$' >/dev/null
        rpm="$(download_matching '\.rpm$')"
        verify "$rpm"
        as_root dnf install -y "$tmpdir/$rpm"
        ;;
    *)
        echo "No native package for this distro — falling back to the binary tarball."
        download_matching 'SHA256SUMS$' >/dev/null
        tarball="$(download_matching 'x86_64\.tar\.zst$')"
        verify "$tarball"
        as_root tar --zstd -xf "$tmpdir/$tarball" -C /
        ;;
esac

echo "glimpse $tag installed."

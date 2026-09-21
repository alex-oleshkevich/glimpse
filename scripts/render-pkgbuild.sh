#!/usr/bin/env bash
set -euo pipefail

usage() {
    cat >&2 <<'EOF'
usage:
  scripts/render-pkgbuild.sh --release <version> <x86_64-b2sum>
  scripts/render-pkgbuild.sh --local <version>

--release renders the AUR form: the source is the tarball published on the
release page, checked against its real b2sum.

--local renders the form `just package-aur` builds here: the source is the
tarball `just package-binary` just wrote beside the PKGBUILD, so there is no
download and nothing to check it against.
EOF
    exit 1
}

mode="${1:-}"
case "$mode" in
    --release)
        version="${2:-}"
        b2sum="${3:-}"
        [[ -n "$version" && -n "$b2sum" ]] || usage
        sed -E \
            -e "s/^pkgver=.*/pkgver=${version}/" \
            -e "s/^b2sums_x86_64=.*/b2sums_x86_64=('${b2sum}')/" \
            PKGBUILD
        ;;
    --local)
        version="${2:-}"
        [[ -n "$version" ]] || usage
        sed -E \
            -e "s/^pkgver=.*/pkgver=${version}/" \
            -e "s|^source_x86_64=.*|source_x86_64=(\"glimpse-\$pkgver-x86_64.tar.zst\")|" \
            -e "s/^b2sums_x86_64=.*/b2sums_x86_64=('SKIP')/" \
            PKGBUILD
        ;;
    *)
        usage
        ;;
esac

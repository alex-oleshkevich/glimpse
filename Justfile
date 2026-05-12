set shell := ["bash", "-euo", "pipefail", "-c"]

aur_pkg := "glimpse-desktop-bin"
aur_remote := "ssh://aur@aur.archlinux.org/glimpse-desktop-bin.git"
github_repo := "alex-oleshkevich/glimpse"

default:
    @just --list

version:
    @awk -F'"' '/^version = / { print $2; exit }' Cargo.toml

sync-pkgver:
    sed -i -E "s/^pkgver=.*/pkgver=$(just version)/" PKGBUILD

verify-release: sync-pkgver
    cargo test --locked -p glimpse-core -p glimpse-idle -p glimpse-lock -p glimpse-sunset -p glimpse-wallpaper
    cargo check --locked -p glimpse-core -p glimpse-idle -p glimpse-lock -p glimpse-sunset -p glimpse-wallpaper

binary-package: verify-release
    scripts/package-binary.sh "$(just version)"

aur-pkgbuild:
    #!/usr/bin/env bash
    set -euo pipefail
    version="$(just version)"
    asset="glimpse-${version}-$(uname -m).tar.zst"
    url="https://github.com/{{ github_repo }}/releases/download/v${version}/${asset}"
    tmpdir="$(mktemp -d)"
    trap 'rm -rf "$tmpdir"' EXIT
    curl -fsSL "$url" -o "$tmpdir/$asset"
    checksum="$(b2sum "$tmpdir/$asset" | awk '{ print $1 }')"
    scripts/render-aur-pkgbuild.sh "$version" "$checksum" > dist/PKGBUILD

aur-srcinfo: aur-pkgbuild
    makepkg -p dist/PKGBUILD --printsrcinfo > dist/.SRCINFO

aur-publish: aur-pkgbuild
    #!/usr/bin/env bash
    set -euo pipefail
    version="$(just version)"
    asset="dist/glimpse-${version}-$(uname -m).tar.zst"
    test -f "$asset"
    tmpdir="$(mktemp -d)"
    trap 'rm -rf "$tmpdir"' EXIT
    git clone "{{ aur_remote }}" "$tmpdir"
    cp dist/PKGBUILD "$tmpdir"/
    cd "$tmpdir"
    makepkg --printsrcinfo > .SRCINFO
    git add PKGBUILD .SRCINFO
    if git diff --cached --quiet; then
        echo "AUR package {{ aur_pkg }} already up to date"
    else
        git commit -m "Release ${version}"
        git push origin master
    fi

github-release: binary-package
    #!/usr/bin/env bash
    set -euo pipefail
    tag="v$(just version)"
    asset="dist/glimpse-$(just version)-$(uname -m).tar.zst"
    gh release create "$tag" "$asset" --verify-tag --title "$tag" --notes "Glimpse $(just version)" || gh release upload "$tag" "$asset" --clobber

release-local: binary-package
    #!/usr/bin/env bash
    set -euo pipefail
    tag="v$(just version)"
    asset="dist/glimpse-$(just version)-$(uname -m).tar.zst"
    git diff --quiet
    git diff --cached --quiet
    git rev-parse "$tag" >/dev/null 2>&1 || git tag -a "$tag" -m "Release $tag"
    git push origin HEAD
    git push origin "$tag"
    gh release create "$tag" "$asset" --verify-tag --title "$tag" --notes "Glimpse $(just version)" || gh release upload "$tag" "$asset" --clobber
    just aur-publish

act-ci:
    act push -W .github/workflows/ci.yml

act-release:
    #!/usr/bin/env bash
    set -euo pipefail
    tag="v$(just version)"
    act push -W .github/workflows/release.yml -e <(printf '{"ref":"refs/tags/%s","ref_name":"%s"}\n' "$tag" "$tag")

watch-runs:
    #!/usr/bin/env bash
    set -euo pipefail
    gh run list --limit 10
    run_id="$(gh run list --limit 1 --json databaseId --jq '.[0].databaseId')"
    test -n "$run_id"
    gh run watch "$run_id" --exit-status

# ---- SDK releases ------------------------------------------------------------
# Tag format per SDK:
#   rs -> sdk-rs-vX.Y.Z   (publishes to crates.io)
#   py -> sdk-py-vX.Y.Z   (publishes to PyPI)
#   ts -> sdk-ts-vX.Y.Z   (publishes to npmjs.org)
#   go -> sdk-go/vX.Y.Z   (creates a GitHub Release; Go consumes by tag)
#
# GitHub Actions silently drops tag-push triggers when more than three tags
# are pushed in a single git push. release-sdks-all pushes them one at a time
# to avoid that.

sdk-version LANG:
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{ LANG }}" in
        rs) awk -F'"' '/^version = / { print $2; exit }' sdk/sdk-rs/Cargo.toml ;;
        py) awk -F'"' '/^version = / { print $2; exit }' sdk/sdk-py/pyproject.toml ;;
        ts) node -p "require('./sdk/sdk-ts/package.json').version" ;;
        go) echo "(no manifest; pass version explicitly)" ;;
        *) echo "unknown SDK: {{ LANG }}" >&2; exit 2 ;;
    esac

sdk-versions:
    @echo "rs: $(just sdk-version rs)"
    @echo "py: $(just sdk-version py)"
    @echo "ts: $(just sdk-version ts)"
    @echo "go: $(just sdk-version go)"

sdk-tag LANG VERSION:
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{ LANG }}" in
        rs|py|ts) echo "sdk-{{ LANG }}-v{{ VERSION }}" ;;
        go) echo "sdk-go/v{{ VERSION }}" ;;
        *) echo "unknown SDK: {{ LANG }}" >&2; exit 2 ;;
    esac

# Verify the working tree is clean and the manifest version matches.
sdk-preflight LANG VERSION:
    #!/usr/bin/env bash
    set -euo pipefail
    git diff --quiet
    git diff --cached --quiet
    if [ "{{ LANG }}" != "go" ]; then
        actual="$(just sdk-version {{ LANG }})"
        if [ "$actual" != "{{ VERSION }}" ]; then
            echo "manifest version is $actual but you asked to release {{ VERSION }}" >&2
            exit 1
        fi
    fi

# Tag a single SDK release and push the tag on its own. If the tag
# already exists, delete and re-push it so the publish workflow re-runs.
release-sdk LANG VERSION: (sdk-preflight LANG VERSION)
    #!/usr/bin/env bash
    set -euo pipefail
    tag="$(just sdk-tag {{ LANG }} {{ VERSION }})"
    if git rev-parse "$tag" >/dev/null 2>&1; then
        echo "tag $tag exists; re-pushing"
        git push origin ":$tag" || true
        git tag -d "$tag" >/dev/null
    fi
    git tag -a "$tag" -m "Glimpse {{ LANG }} SDK {{ VERSION }}"
    git push origin "$tag"
    echo "pushed $tag"

# Delete and re-push the tag to retry a failed publish workflow.
retry-sdk-release LANG VERSION:
    #!/usr/bin/env bash
    set -euo pipefail
    tag="$(just sdk-tag {{ LANG }} {{ VERSION }})"
    git push origin ":$tag" || true
    git tag -d "$tag" 2>/dev/null || true
    git tag -a "$tag" -m "Glimpse {{ LANG }} SDK {{ VERSION }}"
    git push origin "$tag"
    echo "re-pushed $tag"

# Release every SDK at the version pinned in the manifests. Verifies
# all three manifest versions match, then tags and pushes them one at
# a time (GitHub Actions silently drops the trigger when more than three
# tags arrive in a single push).
release-sdks:
    #!/usr/bin/env bash
    set -euo pipefail
    rs="$(just sdk-version rs)"
    py="$(just sdk-version py)"
    ts="$(just sdk-version ts)"
    if [ "$rs" != "$py" ] || [ "$rs" != "$ts" ]; then
        echo "manifest versions disagree: rs=$rs py=$py ts=$ts" >&2
        exit 1
    fi
    version="$rs"
    echo "releasing all SDKs at $version"
    just release-sdk rs "$version"
    just release-sdk py "$version"
    just release-sdk ts "$version"
    just release-sdk go "$version"

# Show recent release workflow runs for the four SDKs.
watch-sdk-releases:
    @gh run list --limit 12 --json name,headBranch,status,conclusion,createdAt \
        --jq '.[] | select(.headBranch|startswith("sdk-")) | "\(.headBranch)\t\(.name)\t\(.status)\t\(.conclusion // "")"' \
        | column -t -s $'\t'

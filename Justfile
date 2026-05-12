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

# Tag a single SDK release locally. Used by release-sdks for traceability
# (the registry publish itself runs separately).
sdk-tag-local LANG VERSION:
    #!/usr/bin/env bash
    set -euo pipefail
    tag="$(just sdk-tag {{ LANG }} {{ VERSION }})"
    if git rev-parse "$tag" >/dev/null 2>&1; then
        echo "tag $tag already exists; skipping"
    else
        git tag -a "$tag" -m "Glimpse {{ LANG }} SDK {{ VERSION }}"
    fi

# ---- Local publish recipes ---------------------------------------------------
# Each recipe runs tests, builds, and publishes from this machine without
# going through GitHub Actions. Tokens come from the calling shell's
# environment (in fish: `set -x CRATES_API_TOKEN ...`).

publish-sdk-rs:
    #!/usr/bin/env bash
    set -euo pipefail
    : "${CRATES_API_TOKEN:?CRATES_API_TOKEN must be set in the environment}"
    cd sdk/sdk-rs
    cargo test
    CARGO_REGISTRY_TOKEN="$CRATES_API_TOKEN" cargo publish

publish-sdk-py:
    #!/usr/bin/env bash
    set -euo pipefail
    : "${PYPI_TOKEN:?PYPI_TOKEN must be set in the environment}"
    cd sdk/sdk-py
    python -m unittest discover -s tests
    python -m pip install --quiet --upgrade build twine
    rm -rf dist build glimpse_sdk.egg-info
    python -m build
    TWINE_USERNAME=__token__ TWINE_PASSWORD="$PYPI_TOKEN" python -m twine upload dist/*

publish-sdk-ts:
    #!/usr/bin/env bash
    set -euo pipefail
    : "${NPM_CI_TOKEN:?NPM_CI_TOKEN must be set in the environment}"
    cd sdk/sdk-ts
    npm ci
    npm run build
    npm test
    npmrc="$(mktemp)"
    trap 'rm -f "$npmrc"' EXIT
    chmod 600 "$npmrc"
    printf '//registry.npmjs.org/:_authToken=%s\n' "$NPM_CI_TOKEN" > "$npmrc"
    NPM_CONFIG_USERCONFIG="$npmrc" npm publish --access public

publish-sdk-go VERSION:
    #!/usr/bin/env bash
    set -euo pipefail
    cd sdk/sdk-go
    go build ./...
    go vet ./...
    go test ./...
    cd ../..
    tag="sdk-go/v{{ VERSION }}"
    if ! git rev-parse "$tag" >/dev/null 2>&1; then
        git tag -a "$tag" -m "Glimpse Go SDK {{ VERSION }}"
    fi
    git push origin "$tag" --force
    notes="$(mktemp)"
    trap 'rm -f "$notes"' EXIT
    {
        printf 'Go module: `github.com/alex-oleshkevich/glimpse/sdk/sdk-go` at `%s`.\n\n' "$tag"
        printf 'Install: `go get github.com/alex-oleshkevich/glimpse/sdk/sdk-go@v%s`\n' "{{ VERSION }}"
    } > "$notes"
    if gh release view "$tag" >/dev/null 2>&1; then
        gh release edit "$tag" --title "Go SDK v{{ VERSION }}" --notes-file "$notes"
    else
        gh release create "$tag" --title "Go SDK v{{ VERSION }}" --notes-file "$notes"
    fi

# Release every SDK at the version pinned in the manifests, publishing
# directly from this machine to crates.io / PyPI / npmjs.org / GitHub
# Releases. Requires CRATES_API_TOKEN, PYPI_TOKEN, NPM_CI_TOKEN to be
# exported in the calling shell.
release-sdks:
    #!/usr/bin/env bash
    set -euo pipefail
    git diff --quiet
    git diff --cached --quiet
    rs="$(just sdk-version rs)"
    py="$(just sdk-version py)"
    ts="$(just sdk-version ts)"
    if [ "$rs" != "$py" ] || [ "$rs" != "$ts" ]; then
        echo "manifest versions disagree: rs=$rs py=$py ts=$ts" >&2
        exit 1
    fi
    version="$rs"
    : "${CRATES_API_TOKEN:?CRATES_API_TOKEN must be set in the environment}"
    : "${PYPI_TOKEN:?PYPI_TOKEN must be set in the environment}"
    : "${NPM_CI_TOKEN:?NPM_CI_TOKEN must be set in the environment}"
    echo "releasing all SDKs at $version (local publish)"
    just publish-sdk-rs
    just sdk-tag-local rs "$version"
    just publish-sdk-py
    just sdk-tag-local py "$version"
    just publish-sdk-ts
    just sdk-tag-local ts "$version"
    just publish-sdk-go "$version"
    # Push the rs/py/ts tags last so the registry publish failures (if any)
    # don't leave dangling tags on the remote.
    for lang in rs py ts; do
        tag="$(just sdk-tag $lang $version)"
        git push origin "$tag" --force
    done
    echo
    echo "all SDKs released at $version:"
    echo "  rs -> https://crates.io/crates/glimpse-sdk/$version"
    echo "  py -> https://pypi.org/project/glimpse-sdk/$version/"
    echo "  ts -> https://www.npmjs.com/package/glimpse-sdk/v/$version"
    echo "  go -> https://github.com/alex-oleshkevich/glimpse/releases/tag/sdk-go/v$version"

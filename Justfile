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
    cargo test --locked -p glimpse-core -p glimpse-lock -p glimpse-wallpaper
    cargo check --locked -p glimpse-core -p glimpse-lock -p glimpse-shell -p glimpse-wallpaper

# ---- Local development -------------------------------------------------------

install-local:
    cargo build --release -p glimpse-shell -p glimpse-lock -p glimpse-wallpaper
    pkexec install -Dm755 \
        "$(pwd)/target/release/glimpse-shell" \
        "$(pwd)/target/release/glimpse-lock" \
        "$(pwd)/target/release/glimpse-wallpaper" \
        /usr/bin/
    systemctl --user restart glimpse-shell.service
    systemctl --user is-active glimpse-shell.service

run-shell *args:
    RUST_LOG="${RUST_LOG:-info}" cargo run -p glimpse-shell -- {{ args }}

run-lock *args:
    RUST_LOG="${RUST_LOG:-info}" cargo run -p glimpse-lock -- {{ args }}

run-wallpaper *args:
    RUST_LOG="${RUST_LOG:-info}" cargo run -p glimpse-wallpaper -- {{ args }}

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
    rm -rf dist build glimpse_sdk.egg-info
    uv build
    UV_PUBLISH_TOKEN="$PYPI_TOKEN" uv publish

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
    echo "  py -> https://pypi.org/project/glimpse-applet-sdk/$version/"
    echo "  ts -> https://www.npmjs.com/package/glimpse-sdk/v/$version"
    echo "  go -> https://github.com/alex-oleshkevich/glimpse/releases/tag/sdk-go/v$version"

# Run the end-to-end protocol contract test against every SDK's
# counter example: build, spawn, drive through init + clicks + close,
# assert the status/popover messages match the counter contract.
e2e-sdks:
    python3 scripts/sdk-e2e.py

# Run the e2e test against a single SDK (rs|py|ts|go).
e2e-sdk LANG:
    python3 scripts/sdk-e2e.py -k {{ LANG }}

# Run the wallpaper IPC e2e test. Intrusive — stops/starts
# glimpse-wallpaper.service, briefly changes the real desktop wallpaper,
# and edits config.toml (restored on exit). Requires a live Wayland session.
e2e-wallpaper:
    bash glimpse-wallpaper/tests/ipc_e2e.sh

# Run the shell IPC e2e. Non-intrusive: does NOT stop your shell — starts an
# isolated second shell (sandboxed socket/app-id/config) in the current
# session. Briefly toggles real volume/brightness/dnd/profile (captured &
# restored); destructive commands only guard-checked. Live Wayland.
e2e-shell:
    bash glimpse-shell/tests/ipc_e2e.sh

# Isolated e2e for the `applets` group (ls/new/dev). Fully sandboxed, no
# compositor, doesn't touch a running shell. Set APPLETS_E2E_NIRI=1 (with
# niri installed) to also run the per-language healthy `dev` matrix in a
# nested, isolated niri.
e2e-applets:
    bash glimpse-shell/tests/applets_e2e.sh

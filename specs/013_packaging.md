---
state: draft
---

# 013 — Packaging and release

How glimpse gets from a workspace build to something a user installs: package formats, the
release pipeline that produces them, and the installer that picks the right one.

## Problem

There is no working release pipeline. `.github/workflows/` has one workflow and it only checks
generated config files for drift — no build, test, or publish workflow exists. The justfile's
`install` recipe references a binary named `glimpse` that no crate produces (the panel binary is
`glimpse-panel`, renamed from `glimpse-shell`), so even the manual install path is broken. The two
packaging scripts that exist (`scripts/package-binary.sh`, `scripts/render-aur-pkgbuild.sh`) predate
that rename and reference `data/` paths that no longer exist. There is no `.deb`, no `.rpm`, and no
installer script — a user's only path in is building from source.

## Goals

- A tag push (`vX.Y.Z`) produces, without further action: a GitHub Release carrying an `x86_64`
  binary tarball, a `.deb`, and a `.rpm`, each checksummed.
- The same tag push updates and publishes the AUR package.
- `curl <url> | bash` installs glimpse correctly on Debian/Ubuntu, Fedora, and Arch, and falls back
  to a manual layout everywhere else.
- Every distribution channel installs the same file set a from-source `just install` would —
  binaries, default config, schema, and whatever PAM/systemd/D-Bus assets exist under `data/` at
  build time.
- `just install`/`just uninstall` work again (fixes the `glimpse`/`glimpse-panel` bug).

## Non-goals

- No architectures beyond `x86_64`. No cross-compilation.
- No Flatpak, Snap, or Nix packaging.
- No packaging for anything but Linux.
- No commercial/dual licensing model — BSD-3-Clause covers the whole tree.

## Tech

### Versioning

`version` in the root `Cargo.toml`'s `[workspace.package]` (currently `0.16.0`) is the single
source of truth. A release workflow run fails at its first step if the pushed tag `vX.Y.Z` doesn't
match that version exactly — the tag never sets the version, it only confirms it.

### License

BSD-3-Clause, new `LICENSE` file at the repo root (none exists yet — `_old/LICENSE` was MIT and is
reference-only, not carried forward). Every binary crate's `Cargo.toml` gets `license = "BSD-3-Clause"`.

### Asset set

Every package format below installs the same files, matching what `just install` places under
`$PREFIX`/`/etc` today:

| Asset | Source |
| --- | --- |
| 6 binaries | `glimpsectl`, `glimpsed`, `glimpse-panel`, `glimpse-lock`, `glimpse-wallpaper`, `glimpse-sunset` |
| Default config + schema | `data/config.default.toml`, `data/config.schema.json` |
| D-Bus service files | `data/dbus-1/services/*` |
| PAM config | `data/pam.d/*` |
| systemd user units | `data/systemd/*` |
| Bundled wallpapers | `wallpapers/*` |
| License | `LICENSE` |

`data/dbus-1/services/`, `data/pam.d/`, and `data/systemd/` currently hold only `.gitkeep` — specs
006 and 009 own writing their actual contents. Packaging stages whatever exists in `data/` at build
time rather than hardcoding filenames, so today's packages ship binaries + config + wallpapers +
license and nothing under those three directories yet; that's expected, not a packaging bug, and
fills in as 006/009 land.

`_old/` also shipped an xdg-desktop-portal descriptor and a `me.aresa.GlimpseIdle.Portal` D-Bus
service for idle handling. Neither is in this asset set: no current crate produces them, and
whether idle goes through that portal design at all in this rewrite is a decision for whichever
spec covers idle/session-lock, not this one. Packaging just isn't the place either got dropped.

### Package formats

| Format | Tool | Produces |
| --- | --- | --- |
| Tarball | `scripts/package-binary.sh` | `glimpse-<version>-x86_64.tar.zst`, a staged `usr/`/`etc/` tree |
| `.deb` | `cargo-deb` | via `[package.metadata.deb]` per binary crate |
| `.rpm` | `cargo-generate-rpm` | via `[package.metadata.generate-rpm]` per binary crate |
| AUR | hand-written `PKGBUILD` | `glimpse-desktop-bin`, downloads the tarball, stages it into `$pkgdir` unmodified |

The tarball is the common base: the AUR package and the generic `install.sh` fallback both consume
it directly rather than re-deriving the file list.

### Runtime dependencies

Best-effort mapping below, used only as an explicit fallback listed alongside each tool's own
automatic detection (`depends = ["$auto", ...]` for cargo-deb, `auto-req` plus an explicit
`requires` table for cargo-generate-rpm) — not trusted on its own, since sonames drift across
distro releases. `$auto`/`auto-req` scan the ELF binaries actually being packaged, which is more
authoritative than this table; the explicit list is a safety net in case that scan misses a
bundled binary it wasn't invoked against directly.

| Library | Debian/Ubuntu runtime | Fedora runtime |
| --- | --- | --- |
| GTK4 | `libgtk-4-1` | `gtk4` |
| libadwaita | `libadwaita-1-0` | `libadwaita` |
| gtk4-layer-shell | `libgtk4-layer-shell0` | `gtk4-layer-shell` |
| libheif | `libheif1` | `libheif` |
| PAM | `libpam0g` | `pam-libs` |
| GeoClue | `geoclue-2.0` (D-Bus service, not linked) | `geoclue2` |

### Checksums

`b2sum` for the AUR `PKGBUILD`'s `b2sums` array (matches upstream `makepkg` convention). A
`SHA256SUMS` file ships alongside every GitHub Release, covering the tarball, `.deb`, and `.rpm` —
the convention most install-script consumers and package reviewers expect.

### Release workflow

New `.github/workflows/release.yml`, triggered on `push: tags: v*` and `workflow_dispatch`:

1. **verify** — tag matches `Cargo.toml` version.
2. **test** — `just check`, `just test` in a container with GTK4/libadwaita/gtk4-layer-shell/libheif
   available.
3. **package** — `just package-binary`, `just package-deb`, `just package-rpm`; uploads all as
   workflow artifacts.
4. **publish** — creates the GitHub Release with tarball + `.deb` + `.rpm` + `SHA256SUMS`/`b2sums`
   attached, then pushes the rendered `PKGBUILD` to the AUR git repo over SSH (`AUR_SSH_PRIVATE_KEY`
   secret; skips with a log line if the secret isn't configured).

### install.sh

Root-level script for `curl | bash`. Detects the distro via `/etc/os-release`:

- Debian/Ubuntu → downloads the `.deb`, `apt install ./glimpse*.deb`.
- Fedora/RHEL → downloads the `.rpm`, `dnf install ./glimpse*.rpm`.
- Arch → prints the AUR install command; does not install directly (AUR packages shouldn't be
  root-installed outside a helper).
- Anything else → downloads the tarball, extracts to `/` as root — same layout `just install`
  produces.

Every path verifies against `SHA256SUMS` before installing.

## Alternatives considered

- **nfpm for `.deb`/`.rpm`** — rejected: a single Go binary would build both formats from one YAML
  config, but it's a second toolchain to install in CI and locally, where `cargo-deb`/
  `cargo-generate-rpm` keep packaging config colocated with each crate's `Cargo.toml` and need
  nothing beyond `cargo`.
- **Cross-compiling to aarch64** — rejected for now: GTK4/libadwaita/gtk4-layer-shell/libheif/PAM
  cross-compilation is real added complexity, and nothing has been tested on ARM. Revisit if there's
  actual demand.
- **`install.sh` always extracting the tarball, regardless of distro** — rejected: simpler, one code
  path, but bypasses `apt`/`dnf`'s dependency tracking and uninstall support on distros where a
  native package exists.

## Risks

- **Technical** — the runtime dependency table above can drift from what a given Debian/Fedora
  release actually ships (soname bumps). Confirmed by an actual build: cargo-deb's `$auto` does
  scan every bundled binary, not just glimpsed's own — but it needs `dpkg-shlibdeps` (Debian's
  `dpkg-dev`) to do anything at all, and contributes zero dependencies when that's absent, as it
  was on the machine this was tested on. The explicit fallback list is load-bearing on any host
  without `dpkg-dev`, not just a backstop — a genuinely stale table entry there is a genuinely
  missing dependency, with nothing else to catch it.
- **Technical** — `data/pam.d/`, `data/systemd/`, `data/dbus-1/services/` are empty today. Packages
  built before specs 006/009 land will install binaries and config but no PAM/systemd/D-Bus
  integration — `glimpse-lock` in particular won't function from a packaged install until 006 lands
  its PAM file. Not a packaging defect, but worth calling out so it isn't mistaken for one.
- **Operational** — AUR publishing needs a maintainer SSH key configured as `AUR_SSH_PRIVATE_KEY`.
  Until it exists, the AUR package doesn't update automatically.

## Rollout plan

1. This spec, approved.
2. Fix `justfile`'s `install`/`uninstall`; rewrite `scripts/package-binary.sh`; add `cargo-deb`/
   `cargo-generate-rpm` metadata and `just package-deb`/`just package-rpm`.
3. `.github/workflows/release.yml`.
4. `PKGBUILD` + rewritten `scripts/render-aur-pkgbuild.sh`.
5. `install.sh`.

## Changelog

- 2026-08-22 — created.
- 2026-08-22 — corrected asset table from 5 to 6 binaries; `glimpse-sunset` (night-light service)
  has a `main.rs` and was missed in the initial pass.
- 2026-08-22 — implemented per the rollout plan. Runtime dependencies section corrected: no
  separate apt-cache/dnf repoquery verification step exists in CI; instead `$auto`/`auto-req` plus
  an explicit fallback list are used together, and the table's role is documented as that fallback,
  not a source CI cross-checks against.
- 2026-08-22 — added a note that `_old/`'s xdg-desktop-portal descriptor and
  `me.aresa.GlimpseIdle.Portal` D-Bus service aren't in this asset set, since no current crate
  produces them; whether idle handling still needs that portal design is out of scope here.
- 2026-08-22 — live-tested `just package-binary`/`package-deb`/`package-rpm`/`aur-pkgbuild` end to
  end (6-binary release build, real `.deb`/`.rpm`/tarball inspection, `makepkg --printsrcinfo`,
  `install.sh` against the real GitHub API). Fixed real bugs the test caught: `package-binary.sh`'s
  version check assumed plain `clap` `--version` output, but it's shadow-rs's multi-line format;
  `package-deb`/`package-rpm` depended on `build-release` (whole workspace), needlessly pulling in
  unrelated broken crates — split into a new `build-release-binaries` recipe scoped to the six
  shipped binaries; `cargo generate-rpm -p glimpsed` doesn't work despite the tool's own doc
  comment saying `-p` takes a crate name — it wants a path (`-p crates/glimpsed`), confirmed by
  running it; cargo-deb's short `Description:` and `Copyright:` fields were placeholders because
  `[package].description` and `[package.metadata.deb].copyright` were unset. Confirmed `$auto`
  does scan every bundled binary, not just glimpsed's own (refuting that Risk as stated) — but also
  confirmed it contributes nothing at all when `dpkg-shlibdeps` isn't installed (true on this test
  host), making the explicit fallback `depends` list load-bearing, not just defense-in-depth.

#!/usr/bin/env bash
set -euo pipefail

source "$(dirname "${BASH_SOURCE[0]}")/glimpse-paths.sh"

for b in "${binaries[@]}"; do
    install -Dm755 "target/release/$b" "$bindir/$b"
done

for f in data/systemd/*.service; do
    [[ -e "$f" ]] && install -Dm644 "$f" "$unitdir/$(basename "$f")"
done
for f in data/systemd/*.target; do
    [[ -e "$f" ]] && install -Dm644 "$f" "$unitdir/$(basename "$f")"
done
for f in data/dbus-1/services/*.service; do
    [[ -e "$f" ]] && install -Dm644 "$f" "$dbusdir/$(basename "$f")"
done
for f in data/portals/*.portal; do
    [[ -e "$f" ]] && install -Dm644 "$f" "$portaldir/$(basename "$f")"
done
for f in data/portals/*-portals.conf; do
    [[ -e "$f" ]] && install -Dm644 "$f" "$portalconfdir/$(basename "$f")"
done
for f in data/pam.d/*; do
    [[ -e "$f" && "$(basename "$f")" != .gitkeep ]] && install -Dm644 "$f" "$pamdir/$(basename "$f")"
done
for f in data/geoclue/conf.d/*.conf; do
    [[ -e "$f" ]] && install -Dm644 "$f" "$geocluedir/$(basename "$f")"
done

install -Dm644 data/config.default.toml "$sharedir/config.default.toml"
install -Dm644 data/config.schema.json "$sharedir/config.schema.json"
install -Dm644 data/language-codes.json "$sharedir/language-codes.json"
install -Dm644 LICENSE "$sharedir/LICENSE"

for f in data/themes/*/*.css; do
    [[ -e "$f" ]] && install -Dm644 "$f" "$sharedir/themes/$(basename "$(dirname "$f")")/$(basename "$f")"
done
for f in wallpapers/*; do
    [[ -e "$f" ]] && install -Dm644 "$f" "$sharedir/wallpapers/$(basename "$f")"
done

# Built by `just build-translations`, which `just install` depends on.
for f in target/locale/*/LC_MESSAGES/glimpse.mo; do
    [[ -e "$f" ]] || continue
    lang="$(basename "$(dirname "$(dirname "$f")")")"
    install -Dm644 "$f" "$localedir/$lang/LC_MESSAGES/glimpse.mo"
done

# Seed the user's own configuration, and never replace it: the moment that file exists it is
# theirs, and an upgrade that overwrote it would discard everything they had written. The copy at
# $sharedir/config.default.toml is the reference that stays current.
seed_user_config() {
    # A packaging build stages into DESTDIR and must not touch anybody's home directory.
    if [[ -n "$destdir" ]]; then
        return 0
    fi

    # Under sudo the environment is root's, so $HOME and $XDG_CONFIG_HOME point at the wrong
    # person. The invoking user's passwd entry is what says where their configuration lives.
    local owner home config_home
    if [[ -n "${SUDO_USER:-}" && "${SUDO_USER}" != "root" ]]; then
        owner="$SUDO_USER"
        home="$(getent passwd "$SUDO_USER" | cut -d: -f6)"
        config_home="$home/.config"
    else
        owner=""
        home="${HOME:-}"
        config_home="${XDG_CONFIG_HOME:-$home/.config}"
    fi

    if [[ -z "$home" || ! -d "$home" ]]; then
        return 0
    fi

    local config="$config_home/glimpse/config.toml"
    if [[ -e "$config" ]]; then
        printf 'keeping existing %s\n' "$config"
        return 0
    fi

    mkdir -p "$config_home/glimpse"
    install -m644 data/config.default.toml "$config"
    # Installed as root the file would be root-owned, and the user could not edit their own
    # configuration without sudo.
    if [[ -n "$owner" ]]; then
        chown "$owner:$(id -gn "$owner")" "$config_home/glimpse" "$config"
    fi
    printf 'installed a starting configuration at %s\n' "$config"
}

seed_user_config

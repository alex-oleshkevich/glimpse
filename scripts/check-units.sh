#!/usr/bin/env bash
set -euo pipefail

binaries=" ${GLIMPSE_BINARIES:?set by the justfile; run through just} "
lock=data/systemd/glimpse-lock.service

noise='is not executable: No such file or directory|^Configuration file .* is marked'
if systemd-analyze --user verify data/systemd/*.service data/systemd/*.target 2>&1 | grep -Ev "$noise" | grep .; then
    exit 1
fi

for f in data/systemd/*.service; do
    bin=$(grep -m1 -oE '^ExecStart=[^ ]+' "$f" | sed 's|.*/||')
    case "$binaries" in
        *" $bin "*) ;;
        *) echo "$f: ExecStart names '$bin', which is not a shipped binary"; exit 1 ;;
    esac
done

for key in $(sed -n '/^\[Service\]/,/^\[/p' "$lock" | grep -oE '^[A-Za-z]+=' | tr -d '='); do
    case " Type ExecStart ExecReload Restart RestartSec " in
        *" $key "*) ;;
        *) echo "$lock: [Service] carries $key= — sandboxing breaks PAM, see README"; exit 1 ;;
    esac
done

if grep -qE '^(BindsTo|Conflicts|Requires|Requisite)=' "$lock"; then
    echo "$lock: a Requires-class or Conflicts= edge can stop the locker mid-lock"; exit 1
fi
if grep -E '^PartOf=' "$lock" | grep -qv '^PartOf=graphical-session.target$'; then
    echo "$lock: PartOf= anything but graphical-session.target can stop the locker mid-lock"; exit 1
fi
members="glimpse-panel glimpse-wallpaper glimpse-sunset glimpse-notifications"
target=data/systemd/glimpse-session.target
for member in $members; do
    unit="data/systemd/$member.service"
    grep -qx 'PartOf=glimpse-session.target' "$unit" || {
        echo "$unit: missing PartOf=glimpse-session.target"; exit 1;
    }
    grep -Eq "^Wants=.*${member}\.service" "$target" || {
        echo "$target: missing Wants=$member.service"; exit 1;
    }
    grep -Eq "^PropagatesReloadTo=.*${member}\.service" "$target" || {
        echo "$target: missing PropagatesReloadTo=$member.service"; exit 1;
    }
done
if grep -Eq '^(Wants|PropagatesReloadTo)=.*glimpse-lock\.service' "$target"; then
    echo "$target: the on-demand locker must stay outside the suite lifecycle"; exit 1
fi
if grep -l '^WantedBy=graphical-session.target$' data/systemd/*.service | grep .; then
    echo "member service is directly enabled by graphical-session.target"; exit 1
fi
grep -qx 'WantedBy=graphical-session.target' "$target" || {
    echo "$target: not enabled by graphical-session.target"; exit 1;
}

echo "units ok"

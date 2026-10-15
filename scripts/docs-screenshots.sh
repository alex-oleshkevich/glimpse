#!/usr/bin/env bash
set -euo pipefail

output="${GLIMPSE_DOCS_OUTPUT:?set by the justfile; run through just}"
read -ra applets <<< "${GLIMPSE_DOCS_APPLETS:?set by the justfile; run through just}"

panel="$PWD/target/debug/glimpse-panel"
ctl="$PWD/target/debug/glimpsectl"
dest="$PWD/website/public/screenshots/applets"
scratch="$(mktemp -d)"
pid=""

stop() {
    if [[ -n "$pid" ]]; then
        kill "$pid" 2>/dev/null || true
        wait "$pid" 2>/dev/null || true
        pid=""
    fi
}
trap 'stop; rm -rf "$scratch"' EXIT

mkdir -p "$dest" "$scratch/config" "$scratch/shots"
top_ignore=90
bottom_ignore=60
padding=24

zone=""
for applet in "${applets[@]}"; do
    zone+="\"$applet\", "
done

for scheme in light dark; do
    cat > "$scratch/config/config.toml" <<EOF
[appearance]
color-scheme = "$scheme"
animation-speed = 0
blur = ["all"]

[[panels]]
monitor = "$output"
position = "bottom"
left = []
center = [${zone%, }]
right = []

[applets.printing]
show = "always"
EOF

    app_id="me.aresa.Glimpse.DocsShots$RANDOM$RANDOM"
    env -u GLIMPSE_CONFIG_PATH LANGUAGE=en LC_ALL=en_US.UTF-8 GLIMPSE_PANEL_APP_ID="$app_id" \
        "$panel" --config "$scratch/config/config.toml" > "$scratch/panel-$scheme.log" 2>&1 &
    pid=$!
    sleep 6

    grim -o "$output" "$scratch/shots/base.png"
    for applet in "${applets[@]}"; do
        if ! env GLIMPSE_PANEL_APP_ID="$app_id" "$ctl" panel popover open "$output" "$applet" > /dev/null 2>&1; then
            echo "skip $applet ($scheme): no popover"
            continue
        fi
        sleep 2
        grim -o "$output" "$scratch/shots/open.png"
        env GLIMPSE_PANEL_APP_ID="$app_id" "$ctl" panel popovers close > /dev/null 2>&1 || true
        magick compare -fuzz 3% "$scratch/shots/base.png" "$scratch/shots/open.png" \
            -compose Src -highlight-color white -lowlight-color black "$scratch/shots/mask.png" 2> /dev/null || true
        read -r width height < <(magick identify -format '%w %h\n' "$scratch/shots/mask.png")
        box="$(magick "$scratch/shots/mask.png" -fill black \
            -draw "rectangle 0,0 $width,$top_ignore" \
            -draw "rectangle 0,$((height - bottom_ignore)) $width,$height" \
            -trim -format '%wx%h%X%Y' info:)"
        if [[ "$box" == 1x1* ]]; then
            echo "skip $applet ($scheme): nothing opened"
            continue
        fi
        IFS='x+' read -r bw bh bx by <<< "${box/+/x}"
        bx=$((bx > padding ? bx - padding : 0))
        by=$((by > padding ? by - padding : 0))
        bw=$((bw + 2 * padding))
        bh=$((bh + 2 * padding))
        magick "$scratch/shots/open.png" -crop "${bw}x${bh}+${bx}+${by}" +repage \
            "$dest/$applet-$scheme.png"
        echo "saved $applet-$scheme.png ($box)"
        sleep 0.4
    done
    stop
done

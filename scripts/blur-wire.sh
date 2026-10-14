#!/bin/sh
verbose=0
if [ "$1" = "-v" ]; then
    verbose=1
    shift
fi
exec awk -v verbose="$verbose" '
function field(line, index_from_end,    parts, count) {
    count = split(line, parts, /[(), ]+/)
    return parts[count - index_from_end]
}
function id(text,    found) {
    match(text, /#[0-9]+/)
    return substr(text, RSTART + 1, RLENGTH - 1)
}
/ -> .*\.get_background_effect\(new id ext_background_effect_surface_v1#[0-9]+, wl_surface#[0-9]+\)/ {
    match($0, /ext_background_effect_surface_v1#[0-9]+/)
    effect = id(substr($0, RSTART, RLENGTH))
    match($0, /wl_surface#[0-9]+\)/)
    surface = id(substr($0, RSTART, RLENGTH - 1))
    owner[effect] = surface
    live[surface]++
    if (live[surface] > peak[surface]) peak[surface] = live[surface]
}
/ -> ext_background_effect_surface_v1#[0-9]+\.destroy\(\)/ {
    match($0, /ext_background_effect_surface_v1#[0-9]+/)
    effect = id(substr($0, RSTART, RLENGTH))
    if (effect in owner) live[owner[effect]]--
}
/ -> wl_surface#[0-9]+\.destroy\(\)/ {
    match($0, /wl_surface#[0-9]+/)
    live[id(substr($0, RSTART, RLENGTH))] = 0
}
/ -> .*\.create_region\(new id wl_region#[0-9]+\)/ {
    match($0, /wl_region#[0-9]+/)
    region = id(substr($0, RSTART, RLENGTH))
    count[region] = 0; area[region] = 0; list[region] = ""
    left[region] = ""; top[region] = ""; right[region] = ""; bottom[region] = ""
}
/ -> wl_region#[0-9]+\.add\(/ {
    match($0, /wl_region#[0-9]+/)
    region = id(substr($0, RSTART, RLENGTH))
    height = field($0, 1); width = field($0, 2); y = field($0, 3); x = field($0, 4)
    count[region]++
    area[region] += width * height
    list[region] = list[region] sprintf("    rect %d %d %d %d\n", x, y, width, height)
    if (left[region] == "" || x < left[region]) left[region] = x
    if (top[region] == "" || y < top[region]) top[region] = y
    if (right[region] == "" || x + width > right[region]) right[region] = x + width
    if (bottom[region] == "" || y + height > bottom[region]) bottom[region] = y + height
}
/ -> ext_background_effect_surface_v1#[0-9]+\.set_blur_region\(/ {
    match($0, /ext_background_effect_surface_v1#[0-9]+/)
    effect = id(substr($0, RSTART, RLENGTH))
    surface = (effect in owner) ? owner[effect] : "?"
    if ($0 ~ /set_blur_region\(nil\)/) {
        printf "set_blur_region effect#%s wl_surface#%s none\n", effect, surface
        next
    }
    match($0, /wl_region#[0-9]+/)
    region = id(substr($0, RSTART, RLENGTH))
    printf "set_blur_region effect#%s wl_surface#%s rects=%d area=%d bbox=(%d,%d,%d,%d)\n", effect, surface, count[region], area[region], left[region], top[region], right[region] - left[region], bottom[region] - top[region]
    if (verbose) printf "%s", list[region]
}
/wl_display#1\.error/ {
    print "ERROR " $0
    failed = 1
}
END {
    for (surface in peak) {
        bad = peak[surface] > 1
        printf "surface wl_surface#%s objects=%d%s\n", surface, peak[surface], bad ? " BAD" : ""
        if (bad) failed = 1
    }
    exit failed
}
' "$@"

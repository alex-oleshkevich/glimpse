build-ui:
    #!/usr/bin/env bash
    for file in resources/ui/*.blp; do
        if [ -f "$file" ]; then
            output="${file%.blp}.ui"
            echo "Compiling $file -> $output"
            blueprint-compiler compile --output "$output" "$file"
        fi
    done


run:
    GLIMPSE_DEBUG_CLOSE_ON_CLOSE=1 cargo run

run-debug:
    GLIMPSE_DEBUG_CLOSE_ON_CLOSE=1 GTK_DEBUG=interactive cargo run

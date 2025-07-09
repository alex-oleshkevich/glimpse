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
    cargo run

run-debug:
    GTK_DEBUG=interactive cargo run

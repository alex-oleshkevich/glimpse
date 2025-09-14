daemon:
    GLIMPSED_PLUGIN_DIR=./var/plugins cargo run -p glimpsed

debug:
    cargo run -p glimpse-plugins-debug

[working-directory: 'glimpse-gui']
gui:
    flutter run \
        --dart-define=GLIMPSED_BIN=/home/alex/projects/glimpse/target/debug/glimpsed \
        --dart-define=GLIMPSED_PLUGIN_DIR=/home/alex/projects/glimpse/glimpsed/var/plugins


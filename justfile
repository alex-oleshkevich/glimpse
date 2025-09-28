export GLIMPSED_BIN := "/home/alex/projects/glimpse/target/debug/glimpsed"
export GLIMPSE_PLUGIN_DIR := "/home/alex/projects/glimpse/var/plugins"

daemon:
    GLIMPSE_PLUGIN_DIR=./var/plugins cargo run -p glimpsed

debug:
    cargo run -p glimpse-plugins-debug

[working-directory: 'glimpse-gui']
gui:
    flutter run \
        --dart-define=GLIMPSED_BIN=/home/alex/projects/glimpse/target/debug/glimpsed \
        --dart-define=GLIMPSE_PLUGIN_DIR=/home/alex/projects/glimpse/glimpsed/var/plugins


build-glimpsed:
    cargo build -p glimpsed

build-debug-plugin:
    cargo build -p glimpse-plugins-debug

build-all: build-glimpsed build-debug-plugin

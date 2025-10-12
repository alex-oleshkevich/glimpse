export GLIMPSED_BIN := "/home/alex/projects/glimpse/target/debug/glimpsed"
export GLIMPSE_PLUGIN_DIR := "/home/alex/projects/glimpse/var/plugins"

daemon:
    GLIMPSE_PLUGIN_DIR=./var/plugins cargo run -p glimpsed

run-plugin plugin:
    cargo run -p glimpse-plugins-{{plugin}}

[working-directory: 'glimpse-gui']
gui: build-glimpsed build-calculator-plugin build-apps-plugin
    flutter run \
        --dart-define=GLIMPSED_BIN=/home/alex/projects/glimpse/target/debug/glimpsed \
        --dart-define=GLIMPSE_PLUGIN_DIR=/home/alex/projects/glimpse/glimpsed/var/plugins

build-glimpsed:
    cargo build -p glimpsed

build-apps-plugin:
    cargo build -p glimpse-plugins-apps
    ln -sf $(realpath target/debug/glimpse-plugins-apps) var/plugins/glimpse-plugins-apps

build-calculator-plugin:
    cargo build -p glimpse-plugins-calculator
    ln -sf $(realpath target/debug/glimpse-plugins-calculator) var/plugins/glimpse-plugins-calculator

build-all: build-glimpsed build-apps-plugin build-calculator-plugin

[working-directory: 'glimpse-gui']
release-ui:
    flutter build linux --release
    install -Dm755 build/linux/x64/release/bundle/glimpse ~/.local/bin/glimpse

release-glimpsed:
    cargo build -p glimpsed --release
    install -Dm755 target/release/glimpsed ~/.local/bin/glimpsed

release-plugins:
    cargo build -p glimpse-plugins-apps --release
    cargo build -p glimpse-plugins-calculator --release

release: release-ui release-glimpsed release-plugins

install:
    makepkg -i .

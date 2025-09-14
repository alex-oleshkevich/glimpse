daemon:
    GLIMPSED_PLUGIN_DIR=./var/plugins cargo run -p glimpsed

debug:
    cargo run -p glimpse-plugins-debug

[working-directory: 'glimpse-gui']
gui:
    flutter run

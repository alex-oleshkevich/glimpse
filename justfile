daemon:
    cargo run -p glimpsed

echo:
    cargo run -p glimpse-plugins-echo

[working-directory: 'glimpse-gui']
gui:
    flutter run

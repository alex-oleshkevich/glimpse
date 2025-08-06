ui:
    cargo run -p glimpse-gui

deaemon:
    cargo run -p glimpsed

calculator:
    cargo run -p glimpse-plugins-calculator -- /run/user/1000/glimpsed-plugins.sock

apps:
    cargo run -p glimpse-plugins-apps -- /run/user/1000/glimpsed-plugins.sock

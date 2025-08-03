This is a linux laucher project (similar to ULauncher, Spotlight, krunner).
The project has this architecture:
1. daemon process (glimpse directory) - runs in the background and listens 2 unix sockets:
    one for GUI connections and another one for plugin connections
2. plugins (glimpse-plugins directory) are binaries that started by daemon or by other way.
    They connect to the daemon via glimplse-plugins.sock socket.
3. GUI is a process that connects to the daemon via unix socket (glimpse.sock)
    and render UI using iced.

DO NOT WRITE ANY CODE! Output it to the terminal.
Always suggest the best solution, don't waste my time!
Always check the current code and align your suggestions to it!

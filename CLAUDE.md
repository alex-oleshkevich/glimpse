# App concept
This is a linux laucher project (similar to ULauncher, Spotlight, krunner).
As I user I type into search bar and get results rendered as a list.
Typical plugins are:
- app search and launch
- calculator
- authenticator code generator
- web bookmarks
- window management
- and more

# Guidelines
DO NOT WRITE ANY CODE! Output it to the terminal.
Always suggest the best solution, don't waste my time!
Always check the current code and align your suggestions to it!
DOT NOT ENTER OR READ directories listed in .gitignore file

# Architecture Overview
## Daemon responsibilities:

- Maintain plugin registry with health checks
- Route messages (broadcast or targeted)
- Track in-flight requests for cancellation
- Batch responses before sending to UI
- Auto-restart crashed plugins

## Plugin responsibilities:

- Register with name on connect
- Handle cancellation via tokio select!
- Maintain own state
- Respond with plugin name as source

## Cancellation strategy:

- Daemon tracks request IDs per UI connection
- When new request comes with same method/context, send Cancel message
- Plugins use tokio::select! with cancellation token

## Batching strategy:

- Daemon collects responses within time window (e.g., 10ms)
- Sends array of responses to UI
- UI can handle both single and batched responses

# Components
The project has these components:

1. daemon process (glimpsed directory)
   - runs in the background
   - spawns plugins and manages their lifecycle via stdio
   - receives JSONRPC messages from GUI client and forwards them to plugins
   - receives JSONRPC messages from Plugins, batches them and sends them back to GUI
2. plugins (glimpse-plugins directory) are binaries that started by daemon
    - they listen for GUI events (forwarded by the Daemon)
    - perform requested action
    - respond with the result
3. GUI is a Flutter application
    - it connects to the Daemon
    - listens to user events
    - forwards user events to the Daemon
    - renders plugin responses


Plugins respond with SearchItem struct which can contain a list of Action enums.
User can use UI controls provided by GUI app to dispatch these actions.
All actions except Custom are dispatched by the Daemon.

# Things to consider

## Backpressure & Flow Control
What if a plugin sends responses faster than UI can consume? Or Host gets overwhelmed?

- Consider bounded channels between components
- Maybe add Pause/Resume messages for stream control
- Set max in-flight requests per UI connection

## Error Recovery & Reconnection

- What if Unix socket disconnects mid-stream?
- Should Host queue messages during plugin restart?
- How to handle partial results when plugin crashes?
- Consider sequence numbers for message ordering after reconnect

## Security & Sandboxing

- Should plugins have restricted filesystem access?
- Rate limiting per plugin/UI?
- Authentication between components?
- Validate message sizes (prevent memory exhaustion)

## Monitoring & Debugging

## Configuration & Lifecycle

## Plugin Discovery & Capabilities

- How are plugins configured (CLI args, config file)?
- Hot reload of plugins?
- Graceful shutdown sequence?
- Plugin dependencies (one plugin needs another)?

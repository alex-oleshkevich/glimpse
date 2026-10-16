# Architecture

glimpse is a set of small programs, not one big shell program. Each program owns one job and reads the same config file. This page is the map of the developer section: it tells you which page holds what, and gives you the shape of the system in one picture.

Start with [Design principles](/developers/principles) if you are new here. It explains the few rules that decide most choices. Then read the page for the part you are changing.

## Which page do you need?

| Page                                                 | What it covers                                                         | Read it when                                            |
| ---------------------------------------------------- | ---------------------------------------------------------------------- | ------------------------------------------------------- |
| [Design principles](/developers/principles)          | The rules behind the shape, and the reason each rule exists            | You wonder why glimpse does it this way                 |
| [How exec applets work](/developers/exec-applets)    | How the panel finds, starts and talks to applet programs               | You work on the applet host or debug a starting problem |
| [Trust model](/developers/trust)                     | What an applet can reach, what the panel checks, and what a crash does | You judge a security or failure question                |
| [How the lock screen works](/developers/lock-screen) | Locking, logind and password checking                                  | You touch anything near locking                         |
| [Applet protocol](/developers/protocol)              | Every message between panel and applet, field by field                 | You need the exact wire format                          |
| [Installed files](/developers/files)                 | Every installed file, unit and bus name                                | You add a file or chase a missing one                   |
| [Build glimpse from source](/contribute/build)       | Building and running the tree                                          | You are setting up to contribute                        |

If you want to write an applet rather than change glimpse itself, start at [Build applets](/applets/). The settings themselves live under [Configuration](/config/).

## The programs

| Program                   | Its one job                                                                                                                           |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| `glimpse-panel`           | The bars, built-in applets and popovers, the exec applet host, and clipboard history. Most services run as tasks inside this process. |
| `glimpse-wallpaper`       | The background image on each screen. It is separate so restarting the panel does not black the screen.                                |
| `glimpse-lock`            | The lock screen and password checking.                                                                                                |
| `glimpse-notifications`   | Notification history and the popup stack other programs post through.                                                                 |
| `glimpse-weather`         | Weather fetching. It fetches nothing while no consumer holds a lease.                                                                 |
| `glimpse-sunset`          | Night light. It follows the settings daemon and warms every screen.                                                                   |
| `glimpse-idle`            | Idle timers and the inhibit lists behind them.                                                                                        |
| `glimpse-settings-daemon` | Location, sunrise and sunset times, and light or dark mode. It is the one process that works out where you are.                       |
| `glimpse-shot`            | Screenshots, and the screenshot portal when run as `glimpse-shot portal`.                                                             |
| `glimpsectl`              | The command line. Each command is one typed call to one provider, or a read of the config from disk.                                  |

Bus names, units and start order for every program are on [Installed files](/developers/files).

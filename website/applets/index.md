# Choosing an applet kind

Use a built-in applet for desktop controls, a command applet for a button that runs a program, or an exec applet for your own changing display and popover.

| Kind | What it can do | When to choose it |
| --- | --- | --- |
| Built-in | Show desktop state and control services, with a ready-made indicator and popover. | An [existing applet](/config/applets/) already covers the job. |
| Command | Show an icon or label and run a different program for each click or scroll gesture. | You want a screenshot button, launcher, or shortcut without writing code. |
| Exec | Run a separate program that supplies indicators and a popover, keeps data, and reacts to events. The React SDK supplies components, state hooks, and host requests. | You need your own interface or behavior. |

A command applet has a fixed display. Its command's output does not become the label. Start with [a screenshot button](/applets/command/).

An exec applet runs as a systemd user unit with your full session access. The panel renders the UI it sends, and the SDK handles communication. Start with [the todo tutorial](/applets/exec/).

Both custom kinds use panel zones and applet configuration. The [command and exec settings reference](/config/applets/custom) lists their settings.

# Add a screenshot button

Add a camera button to your panel, then use it to select a region for a screenshot. You'll need a running panel and `glimpse-shot` installed.

## 1. Define the button

Open your glimpse `config.toml` in an editor. Add this top-level applet table:

```toml
[applets.screenshot]
extends = "command"
icon = "camera-photo-symbolic"
tooltip-format = "Select a screenshot region"
on-click = ["glimpse-shot", "--region"]
```

`screenshot` is the name you'll put in a panel zone. `extends` selects the command kind, and `on-click` supplies the program and its arguments.

## 2. Put it on the panel

Find the `[[panels]]` table for your panel. Add `"screenshot"` to its existing `right` list, keeping the applets already there. For a panel with network and audio controls, that list becomes:

```toml
right = ["screenshot", "network", "audio"]
```

Save the file. A camera icon appears in that zone. Hover over it to see "Select a screenshot region".

## 3. Take a screenshot

Left-click the camera, then drag a rectangle over the area you want. The screenshot editor opens for that region. Its editing and saving controls are covered in [Screenshots](/ecosystem/shot).

## 4. Add middle-click and scroll actions

Add these lines inside `[applets.screenshot]`, alongside `on-click`:

```toml
on-middle-click = ["glimpse-shot", "--window"]
on-scroll-up = ["glimpse-shot", "--screen"]
on-scroll-down = ["glimpse-shot", "--region"]
```

Save again. Middle-click now lets you pick a window. One upward scroll notch starts a screen capture; one downward notch starts region selection. Each notch runs its command, so use a single notch when trying these actions.

The [command settings reference](/config/applets/custom#command) also lists the horizontal scroll gestures.

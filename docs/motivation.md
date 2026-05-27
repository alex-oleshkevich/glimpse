# Motivation

Glimpse exists because I wanted a Wayland desktop around Niri that felt like a real desktop shell, not a collection of disconnected status widgets.

I was a KDE user for a long time, starting around KDE 3.5, and contributed during the KDE 4 era. KDE shaped a lot of what I expect from a desktop: rich integration, strong configuration, and the feeling that the shell is a real part of the system.

Later, I moved away from KDE because I was no longer happy with its look and feel on my own machines. I used GNOME for about two years, then moved through Hyprland and Niri while looking for a workflow that felt better on modern Wayland.

Niri felt right. The missing piece was everything around it.

## Why Another Shell

I was not satisfied with the existing Wayland bar options. Many are useful, but I wanted something closer to a desktop shell: polished, coherent, GTK-based, and visually aligned with the rest of the applications I use.

I tried Ironbar and liked the direction, but could not get it themed the way I wanted. At that point, building my own shell around Niri became the more direct path.

Glimpse is that shell: panel, applets, wallpaper, lock screen, idle behavior, night light, notifications, and custom widgets designed to feel like one desktop.

## What Glimpse Optimizes For

| Value | What it means |
|---|---|
| **Visual coherence** | The shell should look like it belongs next to GTK applications, not like a separate overlay. |
| **Desktop feel** | Panel, lock screen, wallpaper, idle behavior, and session controls should work together. |
| **Niri-first workflow** | Glimpse is built around a modern Wayland compositor workflow, not a traditional floating desktop model. |
| **Plain configuration** | Layout, applets, services, and themes should live in readable TOML and CSS. |
| **Extensibility** | Custom launchers, menus, status widgets, and popovers should be possible without rebuilding the shell. |
| **Daily comfort** | The common things should feel calm, polished, and ready for everyday use. |

## Authorship And AI Assistance

I design the architecture, core services, public configuration, and overall direction of Glimpse. Many routine implementation tasks, fixes, and repetitive code changes were done with AI assistance under my review.

That is intentional. AI helps the project move faster, but the product direction, integration decisions, taste, and final review remain human.

## Design Disclaimer

I am not a professional designer. I make the interface as polished and useful as I can, guided by my own taste and daily use.

The current look and feel is intentionally GNOME-ish. I wanted to recreate a familiar, comfortable environment first, especially because GTK applications already define much of the visual language on my desktop.

That is not the final goal. Over time, I want Glimpse to discover its own visual identity: personal, recognizable, and shaped with help from people who care about desktop UI and UX.

If someone with strong UI or UX skills wants to help improve Glimpse, I would be glad to use that expertise.

# React to media players and workspaces

Read live media and compositor snapshots with `useMpris()` and `useCompositor()`, then send actions through `mpris` and `compositor`. Both hooks return `null` before their first snapshot.

## Show players and workspaces

Replace a generated project's `index.tsx` with this component and run `glimpsectl applet run --dev .`:

```tsx
import {
  ActionRow, compositor, Indicator, mpris, Popover, run, Section,
  useCompositor, useMpris,
} from "glimpse-sdk";

function DesktopControls() {
  const players = useMpris();
  const desktop = useCompositor();
  return (
    <>
      <Indicator icon="multimedia-player-symbolic" tooltip="Desktop controls" />
      <Popover title="Desktop controls">
        <Section title="Media players">
          {(players ?? []).map((player) => (
            <ActionRow
              key={player.id}
              title={player.title ?? player.identity}
              subtitle={player.artist ?? undefined}
              inactive={!player.can.play && !player.can.pause}
              onActivate={() => mpris.control(player.id, "play-pause")}
            />
          ))}
        </Section>
        <Section title="Workspaces">
          {(desktop?.workspaces ?? []).map((workspace) => (
            <ActionRow
              key={workspace.id}
              title={workspace.name ?? String(workspace.index ?? workspace.id)}
              value={workspace.focused ? "Focused" : undefined}
              onActivate={() => compositor.focusWorkspace({ id: workspace.id })}
            />
          ))}
        </Section>
      </Popover>
    </>
  );
}

await run(<DesktopControls />);
```

Open a media player to populate the first section. Activating its row sends play/pause. Activating a workspace row focuses that workspace. Empty snapshots produce empty sections.

## Send other requests

Use the player's `id` with `mpris.control`, `mpris.seek`, or `mpris.setPosition`. Seek offsets and positions are microseconds; negative seek offsets go backward. Check the player's `can` flags before offering controls. `positionNow(player)` estimates the current position from the last snapshot and playback rate; it does not schedule updates.

Workspace targets can be `{ id }`, `{ index }`, `{ name }`, `"next"`, or `"prev"`. Window targets can be `{ id }`, `"next"`, or `"prev"`. The compositor requests also support focusing windows, moving windows, renaming workspaces, and closing windows. See [the SDK API](/applets/exec/reference/sdk#service-requests) for their arguments.

The host accepts at most ten MPRIS and compositor requests combined per second per applet slot, dropping excess requests. Snapshot changes reach the hooks automatically; requests return `void`, so use later snapshots to observe the result.

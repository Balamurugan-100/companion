# Platform behaviour and manual verification

Companion targets macOS and Linux/X11 first. On Wayland it runs in a reduced
fallback mode because transparent, always-on-top, click-through overlay surfaces
are restricted by most compositors.

## Detection and behaviour matrix

At startup, `platform::is_wayland()` checks whether `WAYLAND_DISPLAY` is set. The
result is held in the `WaylandFallback` resource and applied once:

| Session | Window | Click-through | Screen-edge clamp |
|---|---|---|---|
| macOS / Linux X11 | Transparent, undecorated, always-on-top; full colour by default | Clicks captured only over the character; everything else passes through | Enabled |
| macOS (AeroSpace/Spaces) | Same as above, plus accessory activation policy so AeroSpace ignores the window, and `canJoinAllSpaces`, `stationary`, `ignoresCycle` and full-Space-auxiliary AppKit flags, re-applied every two seconds | Same ghost/touch behaviour | Enabled |
| Wayland | Normal decorated, opaque, best-effort always-on-top | Disabled: the whole window is interactive | Skipped (`outer_position()` is unsupported on Wayland) |

The always-on-top setting is best-effort on Wayland; compositors may ignore it.
This is expected and is why the fallback uses a normal decorated window the user
can move and close.

## Manual verification checklist

The automated test suite covers the pure clamp logic, the Wayland detection
helper, and long-delta timer behaviour. The following must be checked on real
hardware; it cannot be proven by unit tests.

### macOS / AeroSpace all-workspaces

1. Start AeroSpace and place normal windows on at least two workspaces.
2. Launch the release companion binary and confirm the startup log says:
   `Companion is an accessory ghost visible on all macOS Spaces/AeroSpace workspaces.`
3. Confirm `aerospace list-windows --all --format '%{window-id}|%{app-name}'`
   does **not** include `companion`. This proves AeroSpace is ignoring it.
4. Switch between AeroSpace workspaces, including one with a full-screen app.
5. Confirm the companion stays visible in the same screen position, with no
   blink or offscreen jump. Its window bounds must remain unchanged.
6. Confirm the companion is full colour. Clicks over the character are captured;
   single-click it to open the panel, or drag it to move the window. Clicks
   anywhere else in the window pass straight through.
7. Press `Ctrl+Shift+C` and confirm show/hide still works.
8. Start a focus session, switch workspace, and confirm the HUD still counts
   down and the rest prompt appears when work completes.

### Wayland fallback (run on at least one real Wayland session)

1. Log into a Wayland session (for example GNOME on Wayland, KDE Plasma on
   Wayland, or sway). Confirm `echo $WAYLAND_DISPLAY` prints a value such as
   `wayland-0`.
2. Launch the release binary from a terminal.
3. Confirm the window is **decorated and opaque**, not a frameless transparent
   overlay.
4. Click and drag the window using its title bar. Confirm it moves normally.
5. Confirm clicks on the character are received by the window (click-through is
   off there), and that the tray icon and `Ctrl+Shift+C` hotkey still work.
6. Confirm focus/breathing timers and the daily summary still work.
7. Record the compositor, session type (`loginctl show-session "$XDG_SESSION_ID" -p Type`),
   and the result of each step.

### Multi-monitor / monitor unplug

1. With two monitors attached, drag the companion fully onto the secondary
   monitor.
2. Unplug that monitor (or disable it in display settings).
3. Within about one second, the window should be moved back onto the remaining
   monitor with a small margin. It should never stay fully off-screen.

### Sleep/wake

1. Start a focus session and a breathing session.
2. Put the machine to sleep for at least ten minutes, then wake it.
3. Confirm the focus timer advances by at most one capped tick instead of
   jumping through the whole session, and that at most one focus-completion
   signal is emitted.
4. Confirm the breathing pacer resumes from its previous phase instead of
   cycling repeatedly.
5. Confirm CPU usage settles back to the idle range (see `PERFORMANCE.md`).

Bevy's `Time<Virtual>` caps a single frame delta at 250 ms; on top of that, the
pure `focus` and `breathing` state machines cap each tick at one second. A long
sleep gap therefore cannot produce a burst of timer events.

### Optional GitHub settings sync

This path needs no GitHub access to verify locally.

1. Create a bare repo: `git init --bare --initial-branch=main /tmp/companion-sync.git`.
2. On a fresh data directory, launch the app. The panel should open on Settings
   and ask for a repository. Press "Not now" and restart: it must not ask again.
3. Enter the local bare repo path, choose a strategy and interval, and press
   "Use repo". Confirm `companion/settings.json` appears in the bare repo.
4. Save a settings change in the Settings tab, press "Sync now", and confirm
   the new value appears in the repo file.
5. Change the repo URL to a second bare repo. Confirm the app offers "Pull
   remote", "Push local", and "Rebase + merge" instead of overwriting anything.
6. Create conflicting edits locally and in the repo, sync, and confirm each
   differing field offers "Use local" / "Use remote". Pick a mix, apply, and
   confirm the pushed file matches those choices.
7. Disable the network and confirm the companion, timers, bubbles, and local
   settings still work; a failed sync should only show a status message.

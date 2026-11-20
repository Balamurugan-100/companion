# Performance budget and measurement

## Budget

From `TECH.md`:

- Idle CPU: low single-digit percentage on a desktop machine.
- Idle rendering: about 30 fps, with higher responsiveness only during
  interaction.
- Hidden/minimized companion: avoid continuous full-speed rendering.

## Implementation

- `WinitSettings` uses `UpdateMode::reactive(33 ms)` while focused and
  `reactive_low_power(33 ms)` while unfocused. This caps idle wakes at roughly
  30 fps while still letting window and input events wake the app immediately.
- Bevy's `Time<Virtual>` limits a single frame delta to 250 ms. The `focus` and
  `breathing` state machines additionally cap each tick at `MAX_TICK` (1 s), so
  sleep/wake or a stalled frame cannot fast-forward timers or emit event bursts.
- The screen-edge clamp runs at most once per second (`WindowBoundsCheck`) and
  only writes a window position when the pure clamp function reports a change.
- The macOS all-Spaces check runs every two seconds (`AllWorkspacesCheck`) and
  only writes AppKit window flags; the developer-nudge timer is a separate
  low-frequency `Timer` (four to seven minutes).
- The app is fully offline: no network client, no API keys, and no runtime
  downloads. The character asset is loaded from disk at startup.

## How to measure on target hardware

Run a release build with the companion visible and idle for 30 minutes, then
sample CPU and memory over a further 60 seconds.

macOS:

```sh
PID=$(pgrep -x companion)
ps -p "$PID" -o pid=,%cpu=,rss=,command=
# Repeat ps every 5s for a minute and average %cpu; rss is in KB.
```

Linux:

```sh
PID=$(pgrep -x companion)
pidstat -p "$PID" 5 12    # average %CPU and RSS over one minute
# or: top -b -p "$PID" -d 5 -n 12
```

Record the averaged CPU percentage and resident memory (convert RSS KB to MB).
Compare the result with the low-single-digit idle target above, and repeat after
hiding the companion to confirm rendering backs off.

No idle-resource numbers were captured on the development host; the commands
above are the procedure for recording them on macOS and Linux hardware.

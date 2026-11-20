# Companion — Technical Spec

Status: Draft for approval
Date: 2026-11-20
Companion document: `PRODUCT.md`

## Decision record

| Decision | Choice | Reason |
|---|---|---|
| Platforms for v1 | macOS, Linux (X11) | Android deferred; Wayland is a follow-up due to overlay restrictions |
| Language | Mostly Rust | Small footprint, native window control, low idle cost |
| 3D + UI engine | Bevy + `bevy_egui` | One event loop; real glTF skeletal animation; egui panels for tools |
| Communication | Reactive rule engine, no LLM, no API key | User explicitly wants no AI keys and a "live" companion |
| ML | On-device only, optional, not in v1 | `tract` (ONNX) / `whisper.cpp` later if needed; no network |
| Data | Local `redb` store | Embedded, simple, no server |
| Network | None at runtime | Offline/private invariant |

Rejected alternatives: Tauri + React/Three.js (two languages, weaker fit for "mostly Rust"); pure egui + `three-d` (two renderers/glue); Bevy-only game UI (painful for text-heavy tools); cloud LLM (explicitly out of scope).

## Architecture

Single Rust binary built around Bevy's app/event loop.

```
src/
  main.rs                 app bootstrap, plugin registration
  app/
    mod.rs                top-level state, mode switching (companion / dashboard)
    settings.rs           persisted settings (sizes, timings, themes)
    events.rs             shared app events (FocusStarted, BreakDue, MoodChanged, ...)
  companion/
    mod.rs                companion plugin
    window.rs             frameless/transparent/always-on-top window, click-through, drag
    character.rs          glTF rig, animation state machine, cursor tracking
    behaviour.rs          wander/idle/reaction scheduling
    dialogue.rs           rule engine -> authored bubble lines
    lines.rs              authored line library (data, easily edited)
  ui/
    mod.rs                bevy_egui root panel
    dashboard.rs          tabs: Focus, Tasks, Notes, Calm, Summary
    bubbles.rs            speech bubble overlay rendered in egui
  tools/
    focus.rs              Pomodoro timer state machine + session history
    tasks.rs              task CRUD
    notes.rs              plaintext notes
    stress.rs             breathing pacer, grounding steps, mood check-in
    summary.rs            daily aggregation
  store/
    mod.rs                redb schema + repository helpers
  platform/
    mod.rs                tray icon, global hotkey, display/monitor handling
  util/
    time.rs               timers that survive sleep/wake without runaway catch-up
assets/
  character.glb           bundled rigged character with named clips
  lines/                  optional external authored line files
```

### Windows

Two Bevy windows:

1. **Companion window** — transparent, frameless, always-on-top, initially small, draggable. Click-through is toggled via winit's `set_ignore_cursor_events`: on when the pointer is outside the character's hit area, off when inside.
2. **Dashboard window** — normal window holding the egui panels, opened by double-clicking the character, the tray icon, or a global hotkey.

On Linux/X11 these are plain winit windows with `transparent: true`, `decorations: false`, `always_on_top: true`. On macOS the same configuration applies, with extra care for click-through and focus behaviour. Wayland: detect the compositor and fall back to a normal always-on-top window with click-through disabled.

### Character and animation

- Asset: rigged glTF/GLB with named clips — `idle`, `walk`, `blink`, `happy`, `curious`, `sleep` — produced in Blender or Mixamo + Blender export.
- Bevy loads the scene with `SceneLoader`, collects `AnimationClip` handles by name, and drives an `AnimationPlayer`.
- Animation state machine in `companion/character.rs`: states and transitions driven by `app::events` and `behaviour.rs`.
  - `Idle -> Walk` when a wander timer fires.
  - `Walk -> Idle` when the target is reached.
  - `Idle -> Curious` when the cursor enters the hit area.
  - `Any -> Happy` on `TaskCompleted` / `FocusSessionCompleted`.
  - `Idle -> Sleep` after a long idle timeout; wakes on interaction.
- Blink is a short clip layered on idle/walk by a timer, or a separate clip switched to briefly if layering is too complex in v1.
- Cursor tracking: read cursor position from winit, convert to world space, rotate/aim the character's head or body toward it.

### Dialogue rule engine

`dialogue.rs` receives an `app::events` stream and returns an optional line from `lines.rs`. Rules are ordered, first match wins, with a cooldown to prevent spam:

- `MoodChanged(Stressed)` -> breathing suggestion.
- `FocusSessionCompleted` -> encouragement.
- `BreakDue` -> break nudge.
- `Idle > 30min` -> gentle stretch/rest line.
- Time-of-day ranges -> morning/late-evening lines.
- No match -> occasional ambient line on a long randomized timer.

Rules and lines are plain Rust data so they are easy to extend without touching rendering code.

## Crates

| Purpose | Crate |
|---|---|
| Engine / 3D / windowing | `bevy` |
| UI panels | `bevy_egui` (plus `egui` extras as needed) |
| System tray | `tray-icon` |
| Global hotkey | `global-hotkey` |
| Local store | `redb` |
| Sound (later) | `rodio` |
| Optional on-device ML (later) | `tract` (ONNX) |
| Serialization | `serde`, `serde_json` or `toml` |
| Errors / logging | `anyhow`, `tracing` |
| Tests | built-in Rust test harness; `tempfile` for store tests |

No HTTP client, no networking crates, no credentials handling.

## Data model (redb)

- `settings` — key/value settings (focus durations, calm mode, sound on/off, window position).
- `focus_sessions` — start/end timestamps, planned duration, completed flag.
- `tasks` — id, text, done flag, created/completed timestamps.
- `notes` — id, title/body, updated timestamp.
- `mood_checkins` — timestamp, mood value.
- `dialogue_state` — last shown line id, cooldown timestamps.

All tables live in one database file under the OS-appropriate local data directory.

## Performance budget and behaviour

- Idle target: low single-digit CPU on desktop; avoid continuous full-speed rendering when the character is hidden or the screen is locked.
- Throttle animation/render to ~30fps while idle; raise only during interaction.
- Pause expensive work when the companion window is hidden or minimized.
- Timers use monotonic time and cap catch-up after sleep/wake so no burst of events fires on resume.
- Clamp the pet window back on-screen after monitor changes (track last monitor geometry).

## Testing strategy

- Unit tests: dialogue rule selection and cooldown, focus timer transitions, task CRUD, store round-trips, timer catch-up logic.
- Integration tests: simulate event streams and assert the correct animation state and bubble line.
- Manual on real hardware: macOS and Linux X11 — click-through, drag, tray, hotkey, multi-monitor unplug, sleep/wake, full-day idle.
- Offline verification: run with network disabled; confirm no network code paths exist and no keys are required.

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| Rigged character asset is hard to source/create | Start with a simple free glTF rig; define the minimal clip set early; keep a static fallback sprite/mesh if animation pipeline stalls |
| egui text UX is weaker for chat/notes | Keep notes short and plaintext; no free-form chat promised in product spec |
| Wayland overlay restrictions | X11-first; detect and fall back to a normal always-on-top window; document limitation |
| Click-through bugs on macOS | Isolate in `companion/window.rs`; test drag/hover explicitly; provide tray toggle to disable click-through |
| Bevy's always-on-top/transparency config limits | Access winit window handle through Bevy's `WinitWindows`; keep platform code isolated |
| Asset/model quality affects "feels alive" most | Prioritize animation polish in milestone 1; iterate with the user once a mesh exists |

## Build and packaging

- `cargo build --release` for macOS and Linux.
- Package macOS as `.app`/`.dmg`, Linux as AppImage and/or `.deb`; code signing/notarization is a later concern (no account/key requirement in v1).
- Bundle `assets/character.glb` into the binary or install it alongside the binary; must work on first run with no downloads.

## Implementation order

1. Bevy app skeleton + transparent always-on-top companion window + click-through + drag + tray/hotkey.
2. Load bundled glTF character, wire idle/walk/blink animation state machine.
3. Dialogue rule engine + authored line library + mood check-in.
4. Focus timer, tasks, notes, breathing/grounding, daily summary in `bevy_egui` dashboard.
5. Calm mode, performance pass, Linux X11 hardening, Wayland fallback, packaging.

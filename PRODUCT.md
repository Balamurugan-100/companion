# Companion — Product Spec

Status: Draft for approval
Date: 2026-11-20
Related issue: none (personal project; no Linear workspace connected)
Design mock: none available — called out explicitly; interaction design below is text-only.

## Problem

There is no always-present, personal companion on the desktop that feels genuinely alive. Existing options are either chat bots behind an API key, static wallpapers, or game characters that do nothing useful. The user wants a small character that lives on the desktop, feels present through motion and personality, and quietly supports both stress relief and getting things done — without any cloud account, API key, or network dependency.

## Product vision

A small 3D creature that lives on the desktop (macOS and Linux first). It walks, idles, blinks, reacts, and speaks in short authored bubbles. It is a companion, not a chatbot: its personality comes from animation, timing, and context-aware reactions. When wanted, it becomes a calm productivity and wellbeing tool; when not, it stays out of the way.

Out of scope for v1: Android/iOS, voice input/output, cloud AI, any API key, any network call, multi-user, sync between machines.

## Success criteria

1. The character feels alive: distinct idle, walk, blink, react, and sleep animations, with at least one context-driven behaviour (e.g. follows the cursor, reacts when a focus session starts or ends).
2. It is present but not annoying: frameless, always-on-top, click-through when idle, summonable from the tray or a global hotkey.
3. It is useful: a focus timer, quick tasks, notes, and a mood/breathing exercise are reachable from one small panel.
4. It is offline and private: zero API keys, zero network requests, all state stored locally.
5. It is light: idle CPU and memory stay low enough to leave running all day.

The single most important success criterion is **presence** — the character feeling alive. Stress and productivity features are supporting cast.

## Core experience

### 1. The living character

- A small rigged 3D character rendered in a transparent, frameless, always-on-top window.
- Animation set: idle/breathing, walk, blink, happy react, curious react, sleep.
- Behaviours: wanders a small area or walks along a surface, turns to look at the cursor, reacts to app events (focus start/end, task completed, break due, long idle).
- Draggable to reposition; double-click opens the main panel.
- Can be hidden to the tray and restored from the tray or a global hotkey.
- On macOS the window joins every Space (including AeroSpace workspaces) and
  stays stationary while the user switches windows and desktops.
- Procedural stage details (a lit base, glowing ring and orbiting motes) add
  visual depth around the bundled character without another asset or network call.

### 2. Communication (reactive personality, no AI)

- Short speech bubbles built from an authored library of lines, selected by rules: time of day, focus state, mood check-in answer, idle duration, completed tasks, streak counts.
- Mood check-in: a few simple choices ("good / okay / stressed / tired"), with a matching reaction and a suggested next action.
- No free-form chat claim. The companion says timely, contextual things; it does not understand arbitrary text. This expectation is intentional.
- Lines are local data files, easy to edit and extend.

### 3. Stress release

- Guided breathing: a visual pacer (the character's breathing and a simple ring) with configurable inhale/hold/exhale durations and a session length.
- Grounding exercise: a short prompt sequence the user can step through.
- Mood check-in with a suggested action (breathing, break, stretch, shutdown reminder).
- Calm mode: dims UI, slows animations, mutes non-essential bubbles.

### 4. Productivity

- Focus timer (Pomodoro-style): work/break lengths, start/pause/reset, session history.
- Tasks: quick capture, complete, remove; local only.
- Notes: small plaintext notes, local only.
- A compact focus/rest HUD is always visible in the companion window, with a
  phase label, countdown and progress bar. When the work phase ends, the bubble
  explicitly tells the user to rest and the HUD shows `REST NOW`.
- Gentle reminders tied to the character's bubbles ("break due", "long session").
- Occasional offline, developer-flavoured nudges chosen from authored lines by
  a local selector (four to seven minute cadence), suppressed by calm mode and
  while the companion is hidden.
- Daily summary of focus minutes and completed tasks, shown in the panel.

## Invariants

- No network access at runtime. No telemetry, no analytics, no API keys, no account.
- All user data lives in a local database/file under the user's data directory; no data leaves the machine.
- The companion never steals focus, never shows modal dialogs unexpectedly, and never blocks the cursor while idle.
- If the 3D renderer or GPU is unavailable, the app must still start and show a clear message; the tools must remain usable.
- Features degrade gracefully on multi-monitor changes, display sleep/wake, and low battery.

## Edge cases and constraints

- **Linux Wayland:** transparent always-on-top click-through overlays are restricted. X11 is the reliable v1 target; Wayland is a compatibility follow-up with a fallback (normal always-on-top window, click-through disabled).
- **AeroSpace / macOS Spaces:** the pet window must remain visible on every
  workspace; macOS AppKit collection flags are applied and re-checked
  periodically in case a window manager resets them.
- **Multi-monitor / resolution changes:** the pet window must be clamped back on-screen if a monitor is unplugged.
- **Sleep/wake and long idle:** animations and AI-free reaction timers must resume sanely; no runaway CPU.
- **Click-through vs interaction:** when the cursor is over the character, input must be captured so it can be dragged/clicked; elsewhere it passes through.
- **Missing character asset:** ship with at least one bundled rigged glTF character so the app is usable on first run.
- **No design mock:** visual style is intentionally simple and will need iteration once a character asset exists.

## Validation

- Manual: run on macOS and Linux (X11), leave running for a full day, confirm presence, click-through, tray/hotkey, and all four feature areas.
- Automated: unit tests for the dialogue rule engine, focus timer state machine, and data store; snapshot tests for authored line selection where practical.
- Performance: measure idle CPU/RAM after 30 minutes; compare against the light-footprint criterion.
- Offline proof: block all network access during testing and confirm full functionality.

## Milestones

1. **Vertical slice:** transparent always-on-top window + rigged glTF character with idle/walk/blink animations + click-through + tray/hotkey.
2. **Personality:** reactive bubble engine, mood check-in, authored line library.
3. **Tools:** focus timer, tasks, notes, breathing/grounding, daily summary.
4. **Polish:** calm mode, performance pass, Linux X11 hardening, Wayland fallback, packaging for macOS and Linux.

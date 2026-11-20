# 05: Focus timer and session history

**What to build:** A Pomodoro-style focus timer can be started, paused, resumed and reset from the panel. Completed sessions are saved locally and shown as a simple history. Starting and completing a session produces a visible character reaction and an authored bubble so the companion participates rather than just hosting a utility.

**Blocked by:** 03: Local store and settings; 04: Reactive personality and mood check-in.

**Status:** done

- [ ] Timer supports work/break durations from settings with start, pause, resume and reset.
- [ ] Completed sessions persist and are visible as a short history.
- [ ] Completion emits an event that triggers a character reaction and bubble.
- [ ] Timer transitions, including pause/resume and long-idle catch-up, are unit-tested.
- [ ] No notification path requires network access.

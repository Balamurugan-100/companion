# 02: Character comes alive

**What to build:** The placeholder is replaced by a bundled rigged 3D character that visibly breathes at rest, blinks, walks short distances and turns to look toward the pointer. Animation choices are driven by an explicit state machine so behaviour is predictable and testable. A missing or broken asset produces a clear message rather than a crash.

**Blocked by:** 01: Bootable companion window.

**Status:** done

- [ ] Character asset ships with the app; first run requires no download.
- [ ] Idle, blink and walk animations play and transition without popping or freezing.
- [ ] Character turns toward the pointer and wanders within the available on-screen area.
- [ ] Animation state transitions are covered by tests.
- [ ] Missing or invalid asset shows a readable error and the app still starts.

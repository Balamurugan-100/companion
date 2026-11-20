# 03: Local store and settings

**What to build:** User settings and app data are persisted in a local embedded database under the OS data directory. A small settings panel lets the user adjust core preferences such as focus durations, calm mode and window opacity, and those changes survive a full restart. Everything stays on-device with no sync and no network.

**Blocked by:** 01: Bootable companion window.

**Status:** done

- [ ] Database file is created in the OS-appropriate local data directory on first run.
- [ ] Settings panel opens from the companion and edits are saved.
- [ ] Settings values survive an app restart.
- [ ] Store round-trip and empty-database bootstrap tests pass.
- [ ] No network access and no API keys.

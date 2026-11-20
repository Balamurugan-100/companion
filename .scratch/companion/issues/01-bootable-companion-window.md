# 01: Bootable companion window

**What to build:** From a cold start the app launches on macOS and Linux/X11 and presents a small transparent, frameless, always-on-top companion window on the desktop. It can be dragged, passes mouse clicks through when the pointer is outside the companion area, can be hidden and restored from the tray or a global hotkey, and quits cleanly from the tray. A placeholder shape stands in for the character until ticket 02.

**Blocked by:** None (can start immediately).

**Status:** done

- [ ] Launches on macOS and Linux/X11 from a fresh checkout without downloads or extra setup.
- [ ] Window is frameless, transparent and always-on-top; the desktop remains visible behind it.
- [ ] Pointer outside the companion passes clicks through to the desktop; pointer over it captures clicks.
- [ ] Dragging moves the window and its position is kept for the session.
- [ ] Tray icon hides/shows and quits; a global hotkey toggles visibility.
- [ ] No network calls and no credentials of any kind.

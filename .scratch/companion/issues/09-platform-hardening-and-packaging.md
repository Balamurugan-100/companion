# 09: Platform hardening and packaging

**What to build:** The companion behaves sanely across real desktop conditions: monitor changes or unplug, display sleep/wake and long idle. Idle resource use is kept low, Wayland gets a documented degraded fallback, and release builds are packaged for macOS and Linux. Final verification proves the whole app runs with no network.

**Blocked by:** 02: Character comes alive; 08: Daily summary and calm mode.

**Status:** done

- [ ] Character window is clamped back on-screen after monitor changes or unplug.
- [ ] Sleep/wake does not cause event bursts or runaway CPU.
- [ ] Idle CPU and memory are measured and within the product spec budget.
- [ ] Wayland fallback is documented and verified on at least one Wayland session.
- [ ] macOS and Linux release artifacts build and launch.
- [ ] Full offline verification passes: no keys, no network.

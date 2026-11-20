# 04: Reactive personality and mood check-in

**What to build:** The companion speaks through short authored bubbles chosen by a rule engine reacting to time of day, idle time and app events. A mood check-in lets the user pick a state (good / okay / stressed / tired) and the character responds with a matching expression and a helpful suggestion. Lines live in editable data and a cooldown keeps the companion quiet rather than spammy.

**Blocked by:** 02: Character comes alive; 03: Local store and settings.

**Status:** done

- [ ] Bubbles appear in response to at least three distinct triggers.
- [ ] Mood check-in offers four choices and produces a distinct reaction and suggestion for each.
- [ ] A cooldown prevents repeated bubbles in a short window.
- [ ] Rule selection and cooldown behaviour are unit-tested.
- [ ] Lines are data, not hardcoded in rendering or UI code.

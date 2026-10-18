# Sundial — Design Spec

**Date:** 2026-10-18
**Status:** Approved for implementation

## Overview

Sundial is a Windows desktop application in Rust that answers one question: *how long was I actually at this machine today?*

It runs continuously, counting time while the session is unlocked and the user is present. A small always-on-top widget shows today's running total as `HH:MM:SS`. Clicking the widget opens a compact analytics panel anchored to the bottom-right of the screen. The widget can be hidden without stopping tracking; a tray icon brings it back.

The product goals, in priority order:

1. **Trustworthy** — the number must never be inflated by a forgotten lock, a sleeping machine, or a crash.
2. **Lightweight** — a single self-contained executable, no installer, no runtime dependency, minimal idle CPU and memory.
3. **Unobtrusive** — a glanceable widget that stays out of the way.

## Non-goals

- Per-application or per-project breakdown. Sundial tracks presence, not activity categories.
- Cross-platform support. Windows only.
- Cloud sync, accounts, export, or reporting beyond the analytics panel.
- Manual start/stop. Tracking is automatic; there is no timer to forget to start.

## Platform and stack

| Concern | Choice |
|---|---|
| Target | Windows 10/11, x86_64, single-user desktop session |
| UI | `egui` / `eframe` (current release) |
| Win32 APIs | `windows` (windows-rs) |
| Tray | `tray-icon` |
| Serialization | `serde`, `serde_json` |
| Dates | `chrono` (local timezone, `NaiveDate`) |
| Registry | `winreg` |

**Why egui over Tauri.** Tauri would allow styling the UI in HTML/CSS and produce nicer typography, but it ships a WebView2 process per window; an always-running widget plus a popup panel lands around 120–200 MB of RAM. For a process that sits on screen all day that is the wrong trade. egui compiles to a single ~6 MB executable with no runtime dependency, uses roughly 30 MB of RAM, and is GPU-composited, which makes the borderless translucent always-on-top widget straightforward. Raw Win32 + Direct2D would be smaller still (~1 MB) but is substantially more work for no user-visible gain.

Release builds use `opt-level = "z"`, LTO, `codegen-units = 1`, `panic = "abort"`, and `strip = true`. The binary is built with the Windows subsystem (no console window).

## Time accounting semantics

These rules define what the number means. Everything else in the system exists to implement them correctly.

- Time accrues **only** while the session is unlocked *and* the user has produced input within the idle timeout.
- The idle timeout defaults to **300 seconds** (5 minutes). Returning from idle is immediate on the first input event; the idle period itself is never credited retroactively.
- **The guarantee is bounded, and the bound is stated deliberately:** recorded time may exceed actual presence by at most **one idle timeout per absence**, and can never exceed it by more. Presence is evaluated once per tick with no retroactive correction, so walking away without locking credits the full 5-minute window before the clock pauses. This grace window is intentional — it is what stops the timer pausing while you read a long document — but it means the number measures *presence with a grace window*, not literal keyboard activity. The alternative (deducting `min(idle_timeout, accrued)` when presence flips false) was considered and rejected: a timer that visibly jumps backwards by five minutes reads as a bug.
- Totals are per **local calendar day** and reset at local midnight.
- A tracking period crossing midnight is **split at the boundary**, with each portion credited to its own date.
- Elapsed time is measured on the **monotonic clock**. Wall-clock time is used only to decide which date a measurement belongs to.
- Time is never credited for a period the application was not running.

### State machine

The tracker holds two independent booleans rather than a single enum, because the real system has two orthogonal signals and combining them into one enum makes transitions like "resume from sleep" ambiguous:

- `locked` — set by `WTS_SESSION_LOCK`, `WTS_SESSION_LOGOFF` and console disconnect; cleared by `WTS_SESSION_UNLOCK` / `WTS_SESSION_LOGON` / reconnect.
- `present` — true while the last input was within the idle timeout.

**Time accrues if and only if `!locked && present`.** `locked` takes precedence: input observed on the lock screen does not start the clock.

The displayed state is derived for the status dot only:

| Derived state | Dot | Condition |
|---|---|---|
| `Counting` | green | `!locked && present` |
| `IdlePaused` | amber | `!locked && !present` |
| `Locked` | grey | `locked` |

Sleep is handled without a third flag. `PBT_APMSUSPEND` flushes the store and records the suspend. On `PBT_APMRESUMEAUTOMATIC` / `PBT_APMRESUMESUSPEND`, the tracker resets the tick baseline and sets `present = false`, crediting nothing for the sleep interval. It makes no guess about lock state: if the machine resumed to a lock screen, `locked` is already true or the lock event arrives normally; if it resumed unlocked, the next user input sets `present` and counting restarts. Either way the worst case is losing a few seconds, never gaining any. The delta clamp below is the independent backstop for a sleep where no power event arrived at all.

Lock/unlock and power events are delivered by a dedicated background thread that owns a **message-only window** (`HWND_MESSAGE`) registered via `WTSRegisterSessionNotification` and receiving `WM_POWERBROADCAST`. Events are pushed to the tracker over a channel. This is the supported Windows mechanism; the app does not poll for lock state.

Idle is sampled once per second with `GetLastInputInfo`.

### The tick

A 1 Hz tick drives accounting. On each tick, if the state is `Counting`, the monotonic delta since the previous tick is credited to the local date the tick falls in.

- **Delta clamp.** Any delta greater than **5 seconds** is discarded and the tick is treated as a resynchronisation point. This is the backstop that makes a six-hour sleep, a suspended process, a debugger pause, or a wall-clock jump incapable of inflating the total — even if the corresponding system event never arrived.
- **Midnight split.** If a tick's interval straddles local midnight, the delta is divided at the boundary and each part credited to its own date. Because every tick attributes itself to a date independently, there is no separate day-rollover code path and therefore no day-rollover bug.

There is no "current session" record. Today's total *is* the accumulator.

## Storage

Single JSON file at `%APPDATA%\Sundial\data.json`.

```json
{
  "version": 1,
  "days": {
    "2026-10-16": 18120,
    "2026-10-17": 24480,
    "2026-10-18": 15132
  },
  "settings": {
    "idle_timeout_secs": 300,
    "autostart": false,
    "widget_visible": true
  },
  "widget": { "x": 1700, "y": 940 }
}
```

- `days` maps `YYYY-MM-DD` (local) to whole seconds.
- History is retained **forever**. A year is ~365 entries; ten years is under 100 KB. SQLite was rejected because `rusqlite` compiles SQLite from source and would add an MSVC C-toolchain build requirement to store what is effectively a small dictionary.

**Durability.** The file is flushed every **30 seconds** while counting, and on every state transition, on widget move, on settings change, and on exit. Writes are atomic: serialize to `data.json.tmp`, `fsync`, then rename over `data.json`. A power loss mid-write cannot corrupt the file. A crash loses at most 30 seconds and can never *add* time, because nothing is credited for the window between the last flush and the crash.

**Corruption recovery.** If the file is missing, empty, or fails to parse, it is renamed to `data.json.corrupt-<unix-ts>` and a fresh store is created. Startup never fails on bad data.

**Single instance.** A named mutex (`Global\Sundial.SingleInstance`) is acquired at startup. If it is already held, the second process signals the first to show its widget and exits. Two running copies would double-count.

## Statistics

Computed from `days` plus the live in-memory total for today.

| Stat | Definition |
|---|---|
| Today | today's accumulated seconds, live |
| Yesterday | total for the previous calendar day |
| This week | sum of totals from **Monday 00:00 local** through now (calendar week, not rolling) |
| Daily average | mean of the last 30 calendar days that have a recorded total > 0, **excluding today** (today is partial and would drag the mean down) |
| 7-day chart | Monday through Sunday of the current calendar week; future days render empty; today's bar is highlighted |
| History | all recorded days, newest first, scrollable |

"This week" is a calendar week rather than a rolling 7-day sum: a rolling sum labelled "this week" invites misreading. On Monday morning the figure is legitimately small, and the daily-average stat covers for it.

Durations display as `4h 12m` in the panel and `HH:MM:SS` on the widget.

## UI

Shared dark theme in `ui/theme.rs`: background `#1A1C1F` at ~87% opacity, text `#E8EAED`, muted text `#9AA0A6`, accent `#4ADE80`, 10 px corner radius, monospace digits.

### Widget

- 140 × 40 logical px, borderless, always-on-top, no taskbar entry, translucent.
- Contents: a 6 px status dot, then the elapsed time as `HH:MM:SS`.
- Dot colour: **green** `#4ADE80` counting · **amber** `#FBBF24` idle · **grey** `#6B7280` locked. This is the reason the dot exists — with idle-pause in play, a stopped timer and a running timer are otherwise visually identical.
- Draggable from anywhere in the widget. Position is persisted and restored, clamped to a visible monitor's work area on startup in case the display layout changed.
- **Left click** (press and release with less than 4 px of movement, to distinguish from a drag) toggles the analytics panel.
- **Right click** opens a context menu: Hide · Analytics · Start with Windows (checkbox) · Quit.
- Hiding the widget does not stop tracking.

### Analytics panel

- 320 × 400 logical px, borderless, always-on-top, same theme.
- Anchored to the bottom-right of the **work area of the monitor containing the widget**, with a 12 px margin — derived from `MonitorFromWindow` + `GetMonitorInfo().rcWork`, not from primary-screen pixel dimensions. This is what makes placement correct with the taskbar on any edge, on a secondary monitor, and at non-100% DPI scaling.
- Layout, top to bottom: `TODAY` label and hero figure with status dot · a single row of Yesterday / This week / Daily average · the 7-day bar chart with weekday initials · a divider · the scrollable history list.
- Bars reveal their exact duration on hover.
- Closes on **Esc**, on losing focus, on a second click of the widget, or via the tray menu.
- With no data yet, the panel shows today's figure and an empty-state line in place of the history list.

### Tray

- Left click toggles widget visibility.
- Menu: **Show/Hide widget** · **Analytics** · **Start with Windows** (checkbox) · **Quit**.
- The tray icon is the only exit path once the widget is hidden, so it is always present while the app runs.

## Autostart

A registry value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, named `Sundial`, set to the quoted absolute path of the running executable. **Off by default**, toggled from the tray menu and from the widget context menu. The toggle reads actual registry state at startup rather than trusting the settings file, so moving or deleting the executable self-corrects.

## Module structure

```
src/
  main.rs            app wiring, single-instance guard, event loop
  tracker.rs         state machine + tick accounting   (pure, testable)
  stats.rs           today/yesterday/week/average/series (pure, testable)
  store.rs           load/save, atomic write, corruption recovery
  platform/
    mod.rs           SystemEvents trait + fake impl for tests
    session.rs       message-only window: WTS + power broadcasts
    idle.rs          GetLastInputInfo sampling
    monitor.rs       work-area geometry for panel placement
    autostart.rs     registry Run entry
  ui/
    theme.rs         colours, spacing, fonts
    widget.rs        floating timer window
    analytics.rs     bottom-right panel
    chart.rs         7-day bar chart
  tray.rs            tray icon and menu
```

`tracker` and `stats` take timestamped events and explicit clock values as arguments — they make no Windows calls and read no clocks. All platform behaviour sits behind the `SystemEvents` trait so tests can inject synthetic event streams.

## Testing

**Unit tests** (run anywhere, no Windows needed):

- `tracker`: idle entry and exit · lock during idle · unlock resets the idle counter · suspend/resume · time never accrues outside `Counting`.
- `tracker`: midnight split credits each date correctly; a tick exactly on the boundary; a day with no activity stays absent rather than recording zero.
- `tracker`: delta clamp — a 6-hour gap credits nothing; repeated clamped ticks cannot accumulate.
- `tracker`: DST forward and backward transitions produce the correct per-date attribution.
- `stats`: week boundary on Monday · average excludes today and zero days · average with fewer than 30 days of history · empty store.
- `store`: round-trip · atomic write leaves no partial file · corrupt file is quarantined and recovered from · unknown `version` is handled.

**Manual verification checklist** (Windows-only integration, documented in the plan):

lock/unlock · sleep/resume · leave idle 5 minutes then return · crash recovery (kill the process, confirm ≤30 s lost and nothing gained) · midnight rollover (with the clock temporarily advanced) · panel placement with the taskbar moved to the left edge, on a second monitor, and at 150% scaling · autostart toggle on and off · single-instance guard · widget position restored after a display-layout change.

## Build and distribution

`cargo build --release` produces a single `sundial.exe`. No installer. Development requires rustup with the MSVC toolchain and the Visual Studio Build Tools (C++ build tools + Windows SDK) — a one-time install, not currently present on this machine.

## Decisions recorded

- Idle timeout 5 minutes, configurable in `data.json`, no settings UI in v1. Its over-credit bound (one timeout per absence) is accepted and documented rather than corrected — see the time-accounting rules above.
- History retained forever.
- Autostart off by default.
- "This week" is a calendar week starting Monday.
- Daily average excludes today and days with no recorded time.
- No manual pause/resume control; locking the machine is the pause.

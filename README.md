# Sundial

A small Windows app that tracks how long you are actually at your machine each day.

Time counts while your session is unlocked and you have used the keyboard or mouse
within the last 5 minutes. Locking, sleeping, or walking away pauses it. Totals reset
at local midnight; a stretch crossing midnight is split across both days.

## Usage

- The floating widget shows today's total. Drag to move, click to open analytics.
- The dot is **green** while counting, **amber** while idle, **grey** while locked.
- Right-click the widget, or use the tray icon, to hide it, open analytics, toggle
  "Start with Windows", or quit. **Hiding does not stop tracking** — the tray icon
  brings it back.
- Press Esc or click elsewhere to dismiss the analytics panel.

## What the number means

Recorded time may exceed actual presence by at most **one idle timeout (5 minutes)
per absence**, and can never exceed it by more. Presence is checked once a second
with no retroactive correction, so walking away without locking credits the grace
window before the clock pauses. That window is deliberate — it is what stops the
timer pausing while you read.

It can never *over*-count beyond that: elapsed time comes from a monotonic clock,
any tick gap over 5 seconds is discarded outright, and a crash loses at most 30
seconds rather than inventing any.

## Data

`%APPDATA%\Sundial\data.json` — one entry per day, in seconds. Edit
`idle_timeout_secs` there to change the 5-minute threshold. History is kept forever
(a decade is well under 100 KB). The file is written atomically, and a corrupt or
unreadable file is renamed aside rather than taking the app down.

## Build

Requires the MSVC Rust toolchain and Visual Studio Build Tools (C++ workload).

    cargo build --release

Output: `target/release/sundial.exe` — self-contained, no installer.

Note: the renderer must be glow. The wgpu backend exits the process silently on
this target before the app is created.

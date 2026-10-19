//! Presence accounting.
//!
//! # What the recorded number guarantees
//!
//! Time accrues if and only if the session is unlocked and the user has produced
//! input within the idle timeout. Sleep, suspension, crashes and wall-clock jumps
//! cannot inflate it — see [`MAX_TICK_DELTA`].
//!
//! It is **not** a measure of literal keyboard-and-mouse activity, and the
//! difference is worth stating precisely:
//!
//! > Recorded time may exceed actual presence by at most one `idle_timeout` per
//! > absence, and can never exceed it by more.
//!
//! Presence is evaluated once per tick with no retroactive correction, so walking
//! away without locking credits the full idle timeout (5 minutes by default)
//! before the clock pauses. That grace window is deliberate — it is what stops the
//! timer pausing while you read a long document — but it means a user who touches
//! the machine once every six minutes would accrue nearly a full day.
//!
//! If a tighter number is ever wanted, the fix is to deduct
//! `min(idle_timeout, accrued_today)` on the tick where presence flips false; the
//! `idle` argument to [`Tracker::tick`] already carries the information needed.
//! That was considered and deliberately not done, because a timer that visibly
//! jumps backwards by five minutes reads as a bug.

use chrono::{DateTime, Local, NaiveDate, TimeZone};
use std::time::Duration;

/// Any tick gap larger than this is discarded. This is the backstop that makes
/// sleep, suspension, or a clock jump incapable of inflating the total even when
/// the corresponding system event never arrives.
pub const MAX_TICK_DELTA: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DerivedState {
    Counting,
    IdlePaused,
    Locked,
}

pub struct Tracker {
    idle_timeout: Duration,
    locked: bool,
    present: bool,
    /// Monotonic reading at the previous tick. The previous tick's *wall* clock is
    /// deliberately not stored: `split_at_midnight` reconstructs the interval start
    /// from the monotonic delta instead, so a jumped wall clock cannot change how
    /// much time is credited — only which date it lands on.
    last: Option<Duration>,
    counting_since_last_tick: bool,
}

impl Tracker {
    pub fn new(idle_timeout: Duration) -> Self {
        Self {
            idle_timeout,
            locked: false,
            present: false,
            last: None,
            counting_since_last_tick: false,
        }
    }

    pub fn set_locked(&mut self, locked: bool) {
        self.locked = locked;
        if locked {
            self.counting_since_last_tick = false;
        }
        self.refresh_counting();
    }

    /// Called on resume from sleep. Drops the baseline so the sleep interval is
    /// never credited, and assumes absence until the next input is observed.
    pub fn resume(&mut self) {
        self.last = None;
        self.present = false;
        self.counting_since_last_tick = false;
    }

    pub fn state(&self) -> DerivedState {
        if self.locked {
            DerivedState::Locked
        } else if self.present {
            DerivedState::Counting
        } else {
            DerivedState::IdlePaused
        }
    }

    pub fn tick(
        &mut self,
        mono: Duration,
        wall: DateTime<Local>,
        idle: Duration,
    ) -> Vec<(NaiveDate, Duration)> {
        let credits = match self.last {
            Some(prev) if self.counting_since_last_tick => {
                let delta = mono.saturating_sub(prev);
                if delta > MAX_TICK_DELTA || delta.is_zero() {
                    Vec::new()
                } else {
                    split_at_midnight(wall, delta)
                }
            }
            _ => Vec::new(),
        };

        self.present = idle < self.idle_timeout;
        self.last = Some(mono);
        self.refresh_counting();
        credits
    }

    fn refresh_counting(&mut self) {
        self.counting_since_last_tick = !self.locked && self.present;
    }
}

/// Splits `delta` ending at `end` across local calendar dates. Because `delta`
/// is bounded by `MAX_TICK_DELTA`, it spans at most two dates.
fn split_at_midnight(end: DateTime<Local>, delta: Duration) -> Vec<(NaiveDate, Duration)> {
    let span = match chrono::Duration::from_std(delta) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let start = end - span;
    let end_date = end.date_naive();
    let start_date = start.date_naive();

    if start_date == end_date {
        return vec![(end_date, delta)];
    }

    // Local midnight at the start of `end_date`. If the timezone makes that
    // instant ambiguous or non-existent (a DST shift exactly at midnight),
    // fall back to crediting the whole delta to the earlier date rather than
    // guessing — the error is bounded by MAX_TICK_DELTA.
    let midnight = match end_date.and_hms_opt(0, 0, 0) {
        Some(naive) => match Local.from_local_datetime(&naive).earliest() {
            Some(dt) => dt,
            None => return vec![(start_date, delta)],
        },
        None => return vec![(start_date, delta)],
    };

    let after = (end - midnight).to_std().unwrap_or(Duration::ZERO);
    let after = after.min(delta);
    let before = delta - after;

    let mut out = Vec::with_capacity(2);
    if !before.is_zero() {
        out.push((start_date, before));
    }
    if !after.is_zero() {
        out.push((end_date, after));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Local, NaiveDate, TimeZone};

    const IDLE: Duration = Duration::from_secs(300);

    fn wall(h: u32, m: u32, s: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 10, 18, h, m, s).unwrap()
    }
    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 18).unwrap()
    }

    fn active() -> Duration {
        Duration::ZERO
    }

    #[test]
    fn first_tick_credits_nothing() {
        let mut t = Tracker::new(IDLE);
        let c = t.tick(Duration::from_secs(0), wall(9, 0, 0), active());
        assert!(c.is_empty(), "no baseline yet, nothing to credit");
    }

    #[test]
    fn counting_credits_the_elapsed_delta() {
        let mut t = Tracker::new(IDLE);
        t.tick(Duration::from_secs(0), wall(9, 0, 0), active());
        let c = t.tick(Duration::from_secs(1), wall(9, 0, 1), active());
        assert_eq!(c, vec![(day(), Duration::from_secs(1))]);
        assert_eq!(t.state(), DerivedState::Counting);
    }

    #[test]
    fn idle_beyond_timeout_stops_crediting() {
        let mut t = Tracker::new(IDLE);
        t.tick(Duration::from_secs(0), wall(9, 0, 0), active());
        // this tick observes 301s of idleness -> present becomes false
        t.tick(
            Duration::from_secs(1),
            wall(9, 0, 1),
            Duration::from_secs(301),
        );
        assert_eq!(t.state(), DerivedState::IdlePaused);
        let c = t.tick(
            Duration::from_secs(2),
            wall(9, 0, 2),
            Duration::from_secs(302),
        );
        assert!(c.is_empty(), "no time accrues while idle");
    }

    #[test]
    fn input_after_idle_resumes_counting() {
        let mut t = Tracker::new(IDLE);
        t.tick(
            Duration::from_secs(0),
            wall(9, 0, 0),
            Duration::from_secs(301),
        );
        assert_eq!(t.state(), DerivedState::IdlePaused);
        t.tick(Duration::from_secs(1), wall(9, 0, 1), active());
        assert_eq!(t.state(), DerivedState::Counting);
        let c = t.tick(Duration::from_secs(2), wall(9, 0, 2), active());
        assert_eq!(c, vec![(day(), Duration::from_secs(1))]);
    }

    #[test]
    fn locked_beats_present() {
        let mut t = Tracker::new(IDLE);
        t.tick(Duration::from_secs(0), wall(9, 0, 0), active());
        t.set_locked(true);
        assert_eq!(t.state(), DerivedState::Locked);
        let c = t.tick(Duration::from_secs(1), wall(9, 0, 1), active());
        assert!(c.is_empty(), "input on the lock screen must not count");
    }

    #[test]
    fn unlock_resumes_counting() {
        let mut t = Tracker::new(IDLE);
        t.tick(Duration::from_secs(0), wall(9, 0, 0), active());
        t.set_locked(true);
        t.tick(Duration::from_secs(1), wall(9, 0, 1), active());
        t.set_locked(false);
        t.tick(Duration::from_secs(2), wall(9, 0, 2), active());
        let c = t.tick(Duration::from_secs(3), wall(9, 0, 3), active());
        assert_eq!(c, vec![(day(), Duration::from_secs(1))]);
    }

    #[test]
    fn oversized_delta_is_clamped_to_nothing() {
        let mut t = Tracker::new(IDLE);
        t.tick(Duration::from_secs(0), wall(9, 0, 0), active());
        // six hours of sleep with no power event at all
        let c = t.tick(Duration::from_secs(6 * 3600), wall(15, 0, 0), active());
        assert!(c.is_empty(), "a 6h gap must credit nothing");
        // and the next normal tick resynchronises rather than back-filling
        let c = t.tick(Duration::from_secs(6 * 3600 + 1), wall(15, 0, 1), active());
        assert_eq!(c, vec![(day(), Duration::from_secs(1))]);
    }

    #[test]
    fn resume_credits_nothing_for_the_sleep_interval() {
        let mut t = Tracker::new(IDLE);
        t.tick(Duration::from_secs(0), wall(9, 0, 0), active());
        t.resume();
        assert_eq!(
            t.state(),
            DerivedState::IdlePaused,
            "resume assumes absence until input"
        );
        let c = t.tick(Duration::from_secs(3600), wall(10, 0, 0), active());
        assert!(c.is_empty());
    }

    #[test]
    fn tick_straddling_midnight_is_split_across_two_dates() {
        let mut t = Tracker::new(IDLE);
        let before = Local.with_ymd_and_hms(2026, 10, 18, 23, 59, 58).unwrap();
        let after = Local.with_ymd_and_hms(2026, 10, 19, 0, 0, 2).unwrap();
        t.tick(Duration::from_secs(0), before, active());
        let c = t.tick(Duration::from_secs(4), after, active());

        assert_eq!(c.len(), 2, "should split: {c:?}");
        assert_eq!(
            c[0],
            (
                NaiveDate::from_ymd_opt(2026, 10, 18).unwrap(),
                Duration::from_secs(2)
            )
        );
        assert_eq!(
            c[1],
            (
                NaiveDate::from_ymd_opt(2026, 10, 19).unwrap(),
                Duration::from_secs(2)
            )
        );

        let total: Duration = c.iter().map(|(_, d)| *d).sum();
        assert_eq!(
            total,
            Duration::from_secs(4),
            "split must conserve total time"
        );
    }

    #[test]
    fn tick_landing_exactly_on_midnight_credits_only_the_previous_day() {
        let mut t = Tracker::new(IDLE);
        let before = Local.with_ymd_and_hms(2026, 10, 18, 23, 59, 59).unwrap();
        let at = Local.with_ymd_and_hms(2026, 10, 19, 0, 0, 0).unwrap();
        t.tick(Duration::from_secs(0), before, active());
        let c = t.tick(Duration::from_secs(1), at, active());
        assert_eq!(
            c,
            vec![(
                NaiveDate::from_ymd_opt(2026, 10, 18).unwrap(),
                Duration::from_secs(1)
            )]
        );
    }
}

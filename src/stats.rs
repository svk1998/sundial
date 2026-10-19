use chrono::{Datelike, NaiveDate};
use std::collections::BTreeMap;
use std::time::Duration;

const AVERAGE_WINDOW_DAYS: i64 = 30;

#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub today: Duration,
    pub yesterday: Duration,
    pub this_week: Duration,
    pub daily_average: Duration,
    /// Monday..Sunday of the current calendar week. `None` = future day.
    pub week_series: [Option<Duration>; 7],
    /// Newest first, includes today.
    pub history: Vec<(NaiveDate, Duration)>,
}

pub fn summarize(days: &BTreeMap<NaiveDate, Duration>, today: NaiveDate) -> Summary {
    let get = |d: NaiveDate| days.get(&d).copied().unwrap_or(Duration::ZERO);

    let monday = today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64);

    let mut week_series = [None; 7];
    let mut this_week = Duration::ZERO;
    for offset in 0..7i64 {
        let date = monday + chrono::Duration::days(offset);
        if date > today {
            continue; // stays None - future
        }
        let value = get(date);
        week_series[offset as usize] = Some(value);
        this_week += value;
    }

    let window_start = today - chrono::Duration::days(AVERAGE_WINDOW_DAYS);
    let (sum, count) = days
        .range(window_start..today)
        .filter(|(_, v)| !v.is_zero())
        .fold((Duration::ZERO, 0u32), |(sum, n), (_, v)| (sum + *v, n + 1));
    let daily_average = if count == 0 {
        Duration::ZERO
    } else {
        sum / count
    };

    let mut history: Vec<(NaiveDate, Duration)> = days.iter().map(|(d, v)| (*d, *v)).collect();
    history.reverse();

    Summary {
        today: get(today),
        yesterday: get(today - chrono::Duration::days(1)),
        this_week,
        daily_average,
        week_series,
        history,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }
    fn h(hours: u64) -> Duration {
        Duration::from_secs(hours * 3600)
    }

    /// Oct 2026: 12th is a Monday, 14th Wednesday, 18th Sunday.
    fn week_of_oct_2026() -> BTreeMap<NaiveDate, Duration> {
        let mut m = BTreeMap::new();
        m.insert(d(12), h(5)); // Mon
        m.insert(d(13), h(6)); // Tue
        m.insert(d(14), h(4)); // Wed
        m
    }

    #[test]
    fn this_week_sums_from_monday_only() {
        let mut m = week_of_oct_2026();
        m.insert(d(11), h(9)); // previous Sunday - must be excluded
        let s = summarize(&m, d(14));
        assert_eq!(s.this_week, h(15), "Mon+Tue+Wed only");
    }

    #[test]
    fn today_and_yesterday_read_the_right_days() {
        let s = summarize(&week_of_oct_2026(), d(14));
        assert_eq!(s.today, h(4));
        assert_eq!(s.yesterday, h(6));
    }

    #[test]
    fn week_series_is_monday_first_with_future_days_empty() {
        let s = summarize(&week_of_oct_2026(), d(14));
        assert_eq!(s.week_series[0], Some(h(5)));
        assert_eq!(s.week_series[1], Some(h(6)));
        assert_eq!(s.week_series[2], Some(h(4)));
        assert_eq!(s.week_series[3], None, "Thursday is in the future");
        assert_eq!(s.week_series[6], None, "Sunday is in the future");
    }

    #[test]
    fn past_day_with_no_record_is_zero_not_empty() {
        let mut m = BTreeMap::new();
        m.insert(d(12), h(5)); // Mon
        m.insert(d(14), h(4)); // Wed, Tue missing
        let s = summarize(&m, d(14));
        assert_eq!(
            s.week_series[1],
            Some(Duration::ZERO),
            "past gap renders as a zero bar"
        );
    }

    #[test]
    fn daily_average_excludes_today_and_zero_days() {
        let mut m = BTreeMap::new();
        m.insert(d(12), h(4));
        m.insert(d(13), h(6));
        m.insert(d(14), Duration::ZERO); // recorded but zero - excluded
        m.insert(d(15), h(100)); // today - excluded
        let s = summarize(&m, d(15));
        assert_eq!(s.daily_average, h(5), "(4+6)/2");
    }

    #[test]
    fn daily_average_ignores_days_older_than_thirty() {
        let mut m = BTreeMap::new();
        m.insert(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), h(100));
        m.insert(d(14), h(6));
        let s = summarize(&m, d(15));
        assert_eq!(s.daily_average, h(6));
    }

    #[test]
    fn empty_store_is_all_zeroes() {
        let s = summarize(&BTreeMap::new(), d(14));
        assert_eq!(s.today, Duration::ZERO);
        assert_eq!(s.yesterday, Duration::ZERO);
        assert_eq!(s.this_week, Duration::ZERO);
        assert_eq!(s.daily_average, Duration::ZERO);
        assert!(s.history.is_empty());
    }

    #[test]
    fn history_is_newest_first() {
        let s = summarize(&week_of_oct_2026(), d(14));
        assert_eq!(s.history[0].0, d(14));
        assert_eq!(s.history[2].0, d(12));
    }
}

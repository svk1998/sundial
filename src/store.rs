use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    pub idle_timeout_secs: u64,
    pub autostart: bool,
    pub widget_visible: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { idle_timeout_secs: 300, autostart: false, widget_visible: true }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WidgetPos {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Store {
    pub days: BTreeMap<NaiveDate, Duration>,
    pub settings: Settings,
    pub widget: Option<WidgetPos>,
}

#[derive(Serialize, Deserialize)]
struct Dto {
    version: u32,
    days: BTreeMap<String, u64>,
    #[serde(default)]
    settings: Settings,
    #[serde(default)]
    widget: Option<WidgetPos>,
}

const DATE_FMT: &str = "%Y-%m-%d";

impl Store {
    pub fn data_path() -> PathBuf {
        let mut p = dirs::data_dir().unwrap_or_else(std::env::temp_dir);
        p.push("Sundial");
        p.push("data.json");
        p
    }

    pub fn add(&mut self, date: NaiveDate, amount: Duration) {
        *self.days.entry(date).or_default() += amount;
    }

    pub fn load(path: &Path) -> Store {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(_) => return Store::default(),
        };
        match serde_json::from_str::<Dto>(&text) {
            Ok(dto) => Store {
                days: dto
                    .days
                    .into_iter()
                    .filter_map(|(k, v)| {
                        NaiveDate::parse_from_str(&k, DATE_FMT).ok().map(|d| (d, Duration::from_secs(v)))
                    })
                    .collect(),
                settings: dto.settings,
                widget: dto.widget,
            },
            Err(_) => {
                quarantine(path);
                Store::default()
            }
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let dto = Dto {
            version: 1,
            days: self
                .days
                .iter()
                .map(|(d, dur)| (d.format(DATE_FMT).to_string(), dur.as_secs()))
                .collect(),
            settings: self.settings.clone(),
            widget: self.widget,
        };
        let json = serde_json::to_string_pretty(&dto)?;

        let tmp = path.with_extension("json.tmp");
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(json.as_bytes())?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

fn quarantine(path: &Path) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let dest = path.with_extension(format!("json.corrupt-{ts}"));
    let _ = std::fs::rename(path, dest);
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use std::time::Duration;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = tempdir();
        let path = dir.join("data.json");
        let mut s = Store::default();
        s.add(d(2026, 10, 18), Duration::from_secs(4212));
        s.settings.autostart = true;
        s.widget = Some(WidgetPos { x: 100.0, y: 200.0 });
        s.save(&path).unwrap();

        let loaded = Store::load(&path);
        assert_eq!(loaded.days.get(&d(2026, 10, 18)), Some(&Duration::from_secs(4212)));
        assert!(loaded.settings.autostart);
        assert_eq!(loaded.widget.unwrap().x, 100.0);
    }

    #[test]
    fn add_accumulates_sub_second_precision() {
        let mut s = Store::default();
        for _ in 0..4 {
            s.add(d(2026, 10, 18), Duration::from_millis(250));
        }
        assert_eq!(s.days[&d(2026, 10, 18)], Duration::from_secs(1));
    }

    #[test]
    fn missing_file_yields_defaults() {
        let dir = tempdir();
        let s = Store::load(&dir.join("nope.json"));
        assert!(s.days.is_empty());
        assert_eq!(s.settings.idle_timeout_secs, 300);
        assert!(s.settings.widget_visible);
    }

    #[test]
    fn corrupt_file_is_quarantined_and_recovered() {
        let dir = tempdir();
        let path = dir.join("data.json");
        std::fs::write(&path, b"{ this is not json").unwrap();

        let s = Store::load(&path);
        assert!(s.days.is_empty());

        let quarantined: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("corrupt"))
            .collect();
        assert_eq!(quarantined.len(), 1, "corrupt file should be renamed aside");
    }

    #[test]
    fn save_leaves_no_temp_file_behind() {
        let dir = tempdir();
        let path = dir.join("data.json");
        Store::default().save(&path).unwrap();
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }

    fn tempdir() -> std::path::PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("sundial-test-{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}

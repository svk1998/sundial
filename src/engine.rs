use crate::platform::{self, SystemEvent};
use crate::stats::{self, Summary};
use crate::store::{Settings, Store};
use crate::tracker::{DerivedState, Tracker};
use chrono::Local;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_secs(1);
const FLUSH_EVERY: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub today: Duration,
    pub state: DerivedState,
    pub summary: Summary,
}

#[derive(Debug, Clone, Copy)]
pub enum Command {
    SetWidgetPos(f32, f32),
    SetAutostart(bool),
    SetWidgetVisible(bool),
    Quit,
}

pub struct Engine {
    pub snapshot: Arc<Mutex<Snapshot>>,
    pub settings: Arc<Mutex<Settings>>,
    pub commands: Sender<Command>,
    pub initial_widget_pos: Option<crate::store::WidgetPos>,
}

pub fn start() -> Engine {
    let path = Store::data_path();
    let store = Store::load(&path);

    let settings = Arc::new(Mutex::new(store.settings.clone()));

    // Registry is the source of truth; a moved or deleted exe self-corrects.
    {
        let actual = platform::autostart::is_enabled();
        settings.lock().unwrap().autostart = actual;
    }

    let initial_widget_pos = store.widget;
    let today = Local::now().date_naive();

    let snapshot = Arc::new(Mutex::new(Snapshot {
        today: store.days.get(&today).copied().unwrap_or_default(),
        state: DerivedState::IdlePaused,
        summary: stats::summarize(&store.days, today),
    }));

    let (events_tx, events_rx) = channel::<SystemEvent>();
    if let Err(e) = platform::session::spawn(events_tx) {
        eprintln!("sundial: session events unavailable: {e}");
    }

    let (cmd_tx, cmd_rx) = channel::<Command>();

    {
        let snapshot = Arc::clone(&snapshot);
        let settings = Arc::clone(&settings);
        std::thread::Builder::new()
            .name("sundial-engine".into())
            .spawn(move || run(store, path, snapshot, settings, events_rx, cmd_rx))
            .expect("spawn engine thread");
    }

    Engine { snapshot, settings, commands: cmd_tx, initial_widget_pos }
}

fn run(
    mut store: Store,
    path: PathBuf,
    snapshot: Arc<Mutex<Snapshot>>,
    settings: Arc<Mutex<Settings>>,
    events: Receiver<SystemEvent>,
    commands: Receiver<Command>,
) {
    let idle_timeout = Duration::from_secs(store.settings.idle_timeout_secs);
    let mut tracker = Tracker::new(idle_timeout);
    let origin = Instant::now();
    let mut last_flush = Instant::now();
    let mut dirty = false;

    loop {
        std::thread::sleep(TICK);

        while let Ok(event) = events.try_recv() {
            match event {
                SystemEvent::Locked => tracker.set_locked(true),
                SystemEvent::Unlocked => tracker.set_locked(false),
                SystemEvent::Suspend => {
                    let _ = store.save(&path);
                    last_flush = Instant::now();
                    dirty = false;
                }
                SystemEvent::Resume => tracker.resume(),
            }
        }

        let mut quit = false;
        while let Ok(cmd) = commands.try_recv() {
            match cmd {
                Command::SetWidgetPos(x, y) => {
                    store.widget = Some(crate::store::WidgetPos { x, y });
                    dirty = true;
                }
                Command::SetAutostart(on) => {
                    let _ = platform::autostart::set(on);
                    store.settings.autostart = on;
                    settings.lock().unwrap().autostart = on;
                    dirty = true;
                }
                Command::SetWidgetVisible(v) => {
                    store.settings.widget_visible = v;
                    settings.lock().unwrap().widget_visible = v;
                    dirty = true;
                }
                Command::Quit => quit = true,
            }
        }

        let credits = tracker.tick(
            origin.elapsed(),
            Local::now(),
            platform::idle::since_last_input(),
        );
        for (date, amount) in &credits {
            store.add(*date, *amount);
            dirty = true;
        }

        let today = Local::now().date_naive();
        {
            let mut snap = snapshot.lock().unwrap();
            snap.today = store.days.get(&today).copied().unwrap_or_default();
            snap.state = tracker.state();
            snap.summary = stats::summarize(&store.days, today);
        }

        if quit {
            let _ = store.save(&path);
            return;
        }
        if dirty && last_flush.elapsed() >= FLUSH_EVERY {
            let _ = store.save(&path);
            last_flush = Instant::now();
            dirty = false;
        }
    }
}

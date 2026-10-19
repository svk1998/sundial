use crate::platform::{self, SystemEvent};
use crate::stats::{self, Summary};
use crate::store::{Store, WidgetPos};
use crate::tracker::{DerivedState, Tracker};
use chrono::Local;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const TICK: Duration = Duration::from_secs(1);
const FLUSH_EVERY: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct Snapshot {
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
    pub commands: Sender<Command>,
    /// Read from the registry, not the store, so a moved or deleted exe
    /// self-corrects.
    pub initial_autostart: bool,
    pub initial_visible: bool,
    pub initial_widget_pos: Option<WidgetPos>,
}

/// Owned by `main`, which outlives the UI: the process exits as soon as `main`
/// returns, so the final save has to be waited for rather than requested.
pub struct EngineThread {
    commands: Sender<Command>,
    handle: JoinHandle<()>,
}

impl EngineThread {
    /// Saves and stops the engine. Called however the UI ended - tray Quit, a
    /// WM_CLOSE from the installer, or session end.
    pub fn shutdown(self) {
        let _ = self.commands.send(Command::Quit);
        let _ = self.handle.join();
    }
}

pub fn start() -> (Engine, EngineThread) {
    let path = Store::data_path();
    let store = Store::load(&path);
    let today = Local::now().date_naive();

    let snapshot = Arc::new(Mutex::new(Snapshot {
        state: DerivedState::IdlePaused,
        summary: stats::summarize(&store.days, today),
    }));

    let (events_tx, events_rx) = channel::<SystemEvent>();
    if let Err(e) = platform::session::spawn(events_tx) {
        eprintln!("sundial: session events unavailable: {e}");
    }

    let (cmd_tx, cmd_rx) = channel::<Command>();
    let engine = Engine {
        snapshot: Arc::clone(&snapshot),
        commands: cmd_tx.clone(),
        initial_autostart: platform::autostart::is_enabled(),
        initial_visible: store.settings.widget_visible,
        initial_widget_pos: store.widget,
    };

    let handle = std::thread::Builder::new()
        .name("sundial-engine".into())
        .spawn(move || run(store, path, snapshot, events_rx, cmd_rx))
        .expect("spawn engine thread");

    (
        engine,
        EngineThread {
            commands: cmd_tx,
            handle,
        },
    )
}

fn run(
    mut store: Store,
    path: PathBuf,
    snapshot: Arc<Mutex<Snapshot>>,
    events: Receiver<SystemEvent>,
    commands: Receiver<Command>,
) {
    let idle_timeout = Duration::from_secs(store.settings.idle_timeout_secs);
    let mut tracker = Tracker::new(idle_timeout);
    let origin = Instant::now();
    let mut last_flush = Instant::now();
    let mut next_tick = Instant::now() + TICK;
    let mut dirty = false;

    loop {
        // Waiting on the channel rather than sleeping applies commands - above
        // all Quit - as they arrive, instead of up to a tick late.
        match commands.recv_timeout(next_tick.saturating_duration_since(Instant::now())) {
            Ok(Command::Quit) | Err(RecvTimeoutError::Disconnected) => {
                let _ = store.save(&path);
                return;
            }
            Ok(Command::SetWidgetPos(x, y)) => {
                store.widget = Some(WidgetPos { x, y });
                dirty = true;
                continue;
            }
            Ok(Command::SetWidgetVisible(v)) => {
                store.settings.widget_visible = v;
                dirty = true;
                continue;
            }
            Ok(Command::SetAutostart(on)) => {
                let _ = platform::autostart::set(on);
                continue;
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        next_tick = Instant::now() + TICK;

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

        let now = Local::now();
        for (date, amount) in
            tracker.tick(origin.elapsed(), now, platform::idle::since_last_input())
        {
            store.add(date, amount);
            dirty = true;
        }

        {
            let mut snap = snapshot.lock().unwrap();
            snap.state = tracker.state();
            snap.summary = stats::summarize(&store.days, now.date_naive());
        }

        if dirty && last_flush.elapsed() >= FLUSH_EVERY {
            let _ = store.save(&path);
            last_flush = Instant::now();
            dirty = false;
        }
    }
}

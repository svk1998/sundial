use crate::platform::{self, SystemEvent};
use crate::stats::{self, Summary};
use crate::store::{Store, ThemeMode, WidgetPos, WidgetSize};
use crate::tracker::{DerivedState, Tracker};
use chrono::Local;
use std::path::{Path, PathBuf};
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
    SetTheme(ThemeMode),
    SetWidgetSize(WidgetSize),
    /// Saves and exits the process; it does not return.
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
    pub initial_theme: ThemeMode,
    pub initial_size: WidgetSize,
}

/// Owned by `main`, which outlives the UI: the process exits as soon as `main`
/// returns, so the final save has to be waited for rather than requested.
pub struct EngineThread {
    commands: Sender<Command>,
    handle: JoinHandle<()>,
}

impl EngineThread {
    /// Saves and ends the process (see `save_and_exit`). Called when the UI
    /// ends on its own, e.g. a WM_CLOSE to the visible widget.
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
        initial_theme: store.settings.theme,
        initial_size: store.settings.widget_size,
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
    let quit_signal = platform::quit_signal::QuitSignal::create();

    loop {
        // Waiting on the channel rather than sleeping applies commands - above
        // all Quit - as they arrive, instead of up to a tick late.
        match commands.recv_timeout(next_tick.saturating_duration_since(Instant::now())) {
            Ok(Command::Quit) | Err(RecvTimeoutError::Disconnected) => save_and_exit(&store, &path),
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
            Ok(Command::SetTheme(mode)) => {
                store.settings.theme = mode;
                dirty = true;
                continue;
            }
            Ok(Command::SetWidgetSize(size)) => {
                store.settings.widget_size = size;
                dirty = true;
                continue;
            }
            Ok(Command::SetAutostart(on)) => {
                let _ = platform::autostart::set(on);
                continue;
            }
            Err(RecvTimeoutError::Timeout) => {}
        }
        if quit_signal.as_ref().is_some_and(|q| q.is_set()) {
            save_and_exit(&store, &path);
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

/// Ends the whole process from the engine thread once the data is on disk.
/// Exiting here, rather than returning and letting `main` unwind, doesn't
/// depend on the UI: with the widget hidden, eframe never runs the frame that
/// would complete its own close, so a Quit would leave the process running.
fn save_and_exit(store: &Store, path: &Path) -> ! {
    let _ = store.save(path);
    std::process::exit(0);
}

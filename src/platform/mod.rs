pub mod autostart;
pub mod idle;
pub mod monitor;
pub mod session;
pub mod single_instance;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemEvent {
    Locked,
    Unlocked,
    Suspend,
    Resume,
}

use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE};
use winreg::RegKey;

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const VALUE: &str = "Sundial";

fn exe_command() -> std::io::Result<String> {
    let exe = std::env::current_exe()?;
    Ok(format!("\"{}\"", exe.display()))
}

/// Reads live registry state rather than trusting settings, so a moved or
/// deleted executable self-corrects on next launch.
pub fn is_enabled() -> bool {
    let want = match exe_command() {
        Ok(c) => c,
        Err(_) => return false,
    };
    RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey_with_flags(RUN_KEY, KEY_READ)
        .and_then(|k| k.get_value::<String, _>(VALUE))
        .map(|current| current.eq_ignore_ascii_case(&want))
        .unwrap_or(false)
}

pub fn set(enabled: bool) -> std::io::Result<()> {
    let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN_KEY, KEY_WRITE)?;
    if enabled {
        key.set_value(VALUE, &exe_command()?)
    } else {
        match key.delete_value(VALUE) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        }
    }
}

//! Tiny persisted config: just the chosen UI language for now.
//!
//! Stored at `$XDG_CONFIG_HOME/oebb-monitor/config` (or `~/.config/...`) as a
//! single `language = de|en` line. Uses only std, no extra dependencies.

use std::path::PathBuf;

use crate::lang::Lang;

/// Path to the config file, honoring `$XDG_CONFIG_HOME` then `$HOME/.config`.
fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("oebb-monitor").join("config"))
}

/// Load the saved language, or `None` if unset / unreadable.
pub fn load_language() -> Option<Lang> {
    let content = std::fs::read_to_string(config_path()?).ok()?;
    content.lines().find_map(|line| {
        let value = line.trim().strip_prefix("language")?.trim_start();
        let value = value.strip_prefix('=')?.trim();
        Lang::from_code(value)
    })
}

/// Persist the chosen language, best-effort (errors are ignored).
pub fn save_language(lang: Lang) {
    let Some(path) = config_path() else { return };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let _ = std::fs::write(path, format!("language = {}\n", lang.code()));
}

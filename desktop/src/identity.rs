//! Keep development windows and tray actions distinct from the installed app.
#[cfg(debug_assertions)]
pub const NAME: &str = "Nauctions - Local";
#[cfg(not(debug_assertions))]
pub const NAME: &str = "Nauctions";

//! Read-only process detection and a grace period for the game's final saved variables.
use serde::Serialize;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GameState {
    Running,
    Closed,
    Unknown,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureMode {
    Playing,
    FinalSave,
    Standby,
    DetectionUnknown,
}

pub struct CaptureGate {
    until: u64,
    was_active: bool,
}
impl CaptureGate {
    // Check an uncollected save even when the app starts after the game has closed.
    pub fn new(now: u64) -> Self {
        Self {
            until: now.saturating_add(30),
            was_active: false,
        }
    }
    pub fn update(&mut self, state: GameState, now: u64) -> CaptureMode {
        match state {
            GameState::Running => {
                self.was_active = true;
                self.until = now.saturating_add(60);
                CaptureMode::Playing
            }
            GameState::Unknown => {
                self.was_active = true;
                self.until = now.saturating_add(60);
                CaptureMode::DetectionUnknown
            }
            GameState::Closed => {
                if self.was_active {
                    self.until = now.saturating_add(60);
                    self.was_active = false;
                }
                if now <= self.until {
                    CaptureMode::FinalSave
                } else {
                    CaptureMode::Standby
                }
            }
        }
    }
}
fn normalized(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_start_matches("\\\\?\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}
pub fn matches_installation(root: &Path, selected: Option<&Path>, process: &Path) -> bool {
    let root = normalized(root);
    let process = normalized(process);
    if let Some(selected) = selected {
        return normalized(selected) == process && process.starts_with(&format!("{root}\\"));
    }
    process.starts_with(&format!("{root}\\"))
}
#[cfg(any(windows, test))]
fn candidate(name: &str, selected: Option<&Path>) -> bool {
    if let Some(selected) = selected {
        return selected
            .file_name()
            .is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(name));
    }
    let name = name.to_ascii_lowercase();
    name.starts_with("wow") && name.ends_with(".exe")
}
#[cfg(windows)]
pub fn detect(root: &Path, selected: Option<&Path>) -> GameState {
    use windows_sys::Win32::{
        Foundation::*,
        System::{Diagnostics::ToolHelp::*, Threading::*},
    };
    // No memory access, process injection, or privilege escalation. Only process image paths.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return GameState::Unknown;
        }
        let result = (|| {
            let mut entry: PROCESSENTRY32W = std::mem::zeroed();
            entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
            if Process32FirstW(snapshot, &mut entry) == 0 {
                return if std::io::Error::last_os_error().raw_os_error()
                    == Some(ERROR_NO_MORE_FILES as i32)
                {
                    GameState::Closed
                } else {
                    GameState::Unknown
                };
            }
            let mut unknown = false;
            loop {
                let len = entry
                    .szExeFile
                    .iter()
                    .position(|c| *c == 0)
                    .unwrap_or(entry.szExeFile.len());
                if candidate(&String::from_utf16_lossy(&entry.szExeFile[..len]), selected) {
                    let handle =
                        OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, entry.th32ProcessID);
                    if handle.is_null() {
                        unknown = true;
                    } else {
                        let mut image = vec![0u16; 32768];
                        let mut size = image.len() as u32;
                        let success =
                            QueryFullProcessImageNameW(handle, 0, image.as_mut_ptr(), &mut size);
                        CloseHandle(handle);
                        if success == 0 {
                            unknown = true;
                        } else if matches_installation(
                            root,
                            selected,
                            Path::new(&String::from_utf16_lossy(&image[..size as usize])),
                        ) {
                            return GameState::Running;
                        }
                    }
                }
                if Process32NextW(snapshot, &mut entry) == 0 {
                    if std::io::Error::last_os_error().raw_os_error()
                        != Some(ERROR_NO_MORE_FILES as i32)
                    {
                        unknown = true;
                    }
                    break;
                }
            }
            if unknown {
                GameState::Unknown
            } else {
                GameState::Closed
            }
        })();
        CloseHandle(snapshot);
        result
    }
}
#[cfg(not(windows))]
pub fn detect(_root: &Path, _selected: Option<&Path>) -> GameState {
    GameState::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installation_matching_does_not_confuse_another_wow_or_a_sibling_folder() {
        let root = Path::new(r"D:\Games\Forever");
        assert!(matches_installation(
            root,
            None,
            Path::new(r"\\?\d:\games\FOREVER\WoW.exe")
        ));
        assert!(!matches_installation(
            root,
            None,
            Path::new(r"D:\Games\ForeverOther\WoW.exe")
        ));
        assert!(!matches_installation(
            root,
            Some(Path::new(r"D:\Games\Forever\WoW.exe")),
            Path::new(r"D:\Games\Forever\WowOther.exe")
        ));
    }
    #[test]
    fn startup_and_shutdown_allow_final_saves_then_stop_source_reads() {
        let mut gate = CaptureGate::new(100);
        assert_eq!(gate.update(GameState::Closed, 120), CaptureMode::FinalSave);
        assert_eq!(gate.update(GameState::Closed, 131), CaptureMode::Standby);
        assert_eq!(gate.update(GameState::Running, 140), CaptureMode::Playing);
        // An upload can delay the next probe. The first closed observation still gets a full grace period.
        assert_eq!(gate.update(GameState::Closed, 400), CaptureMode::FinalSave);
        assert_eq!(gate.update(GameState::Closed, 461), CaptureMode::Standby);
        assert_eq!(
            gate.update(GameState::Unknown, 470),
            CaptureMode::DetectionUnknown
        );
        assert_eq!(gate.update(GameState::Closed, 500), CaptureMode::FinalSave);
        assert_eq!(gate.update(GameState::Closed, 561), CaptureMode::Standby);
    }
    #[test]
    fn process_name_filter_refuses_launchers() {
        assert!(candidate("WoW.exe", None));
        assert!(!candidate("Battle.net.exe", None));
        assert!(!candidate("Other.exe", Some(Path::new("WoW.exe"))));
    }
    #[cfg(windows)]
    #[test]
    fn detects_the_actual_process_only_in_its_selected_installation() {
        let exe = std::env::current_exe().unwrap();
        assert_eq!(
            detect(exe.parent().unwrap(), Some(&exe)),
            GameState::Running
        );
        assert_eq!(
            detect(Path::new(r"Z:\not-the-selected-installation"), Some(&exe)),
            GameState::Closed
        );
    }
}

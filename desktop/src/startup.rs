//! Current-user startup only. No elevation or machine-wide registration.
#[cfg(any(windows, test))]
fn quoted_executable(path: &std::path::Path) -> Result<String, String> {
    let path = path
        .to_str()
        .ok_or("The application path is not valid Unicode.")?;
    if path.contains('"') || path.chars().any(char::is_control) {
        return Err("The application path cannot be registered for Windows startup.".into());
    }
    Ok(format!("\"{path}\""))
}
#[cfg(windows)]
fn manager() -> Result<auto_launch::AutoLaunch, String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    auto_launch::AutoLaunchBuilder::new()
        .set_app_name(crate::identity::NAME)
        .set_app_path(&quoted_executable(&executable)?)
        .set_args(&["--background"])
        .set_windows_enable_mode(auto_launch::WindowsEnableMode::CurrentUser)
        .build()
        .map_err(|e| e.to_string())
}
#[cfg(windows)]
pub fn set_enabled(enabled: bool) -> Result<(), String> {
    if enabled {
        return manager()?.enable().map_err(|e| e.to_string());
    }
    // auto-launch's generic disable also tries HKLM. Limit removal to our HKCU value.
    use windows_registry::CURRENT_USER;
    let result = CURRENT_USER
        .options()
        .write()
        .open(r"Software\Microsoft\Windows\CurrentVersion\Run")
        .and_then(|key| key.remove_value(crate::identity::NAME));
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.code().0 as u32 == 0x80070002 => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}
#[cfg(windows)]
pub fn is_enabled() -> Result<bool, String> {
    manager()?.is_enabled().map_err(|e| e.to_string())
}
// Task Manager can disable startup while retaining the Run entry. Test presence
// separately so repairing an uninstall/reinstall does not undo that disable.
#[cfg(windows)]
pub fn registration_present() -> Result<bool, String> {
    let result = windows_registry::CURRENT_USER
        .open(r"Software\Microsoft\Windows\CurrentVersion\Run")
        .and_then(|key| key.get_string(crate::identity::NAME));
    match result {
        Ok(_) => Ok(true),
        Err(error) if error.code().0 as u32 == 0x80070002 => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}
#[cfg(not(windows))]
pub fn registration_present() -> Result<bool, String> {
    Ok(false)
}
#[cfg(not(windows))]
pub fn set_enabled(_enabled: bool) -> Result<(), String> {
    Err("Windows startup is available only on Windows.".into())
}
#[cfg(not(windows))]
pub fn is_enabled() -> Result<bool, String> {
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn executable_paths_with_spaces_are_quoted_without_quoting_the_arguments() {
        let path = std::path::Path::new(r"C:\Users\Test User\Apps\Nauctions.exe");
        assert_eq!(
            format!("{} --background", quoted_executable(path).unwrap()),
            r#""C:\Users\Test User\Apps\Nauctions.exe" --background"#
        );
    }
}

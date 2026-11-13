use std::path::Path;
use std::process::Command;

pub fn launch_installer(installer_path: &Path) -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        let ext = installer_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext == "exe" {
            Command::new(installer_path)
                .args(["/SILENT", "/CLOSEAPPLICATIONS", "/RESTARTAPPLICATIONS"])
                .spawn()
                .map_err(|e| format!("Failed to start installer: {e}"))?;
            return Ok(true); // Caller should exit cleanly so installer updates & restarts
        }
        open::that(installer_path).map_err(|e| e.to_string())?;
        Ok(false)
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(installer_path)
            .spawn()
            .map_err(|e| format!("Failed to open installer: {e}"))?;
        Ok(false)
    }

    #[cfg(target_os = "linux")]
    {
        let ext = installer_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if ext == "deb" {
            let status = Command::new("pkexec")
                .args(["apt", "install", "-y"])
                .arg(installer_path)
                .spawn();
            if status.is_ok() {
                return Ok(false);
            }
        }
        open::that(installer_path).map_err(|e| e.to_string())?;
        Ok(false)
    }
}

pub fn set_launch_at_startup(enabled: bool) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let (key, _) = hkcu
            .create_subkey(r"Software\Microsoft\Windows\CurrentVersion\Run")
            .map_err(|e| e.to_string())?;

        if enabled {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let cmd = format!("\"{}\" --minimized", exe.display());
            key.set_value("Langhian", &cmd).map_err(|e| e.to_string())?;
        } else {
            let _ = key.delete_value("Langhian");
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    {
        let home = std::env::var("HOME").map_err(|e| e.to_string())?;
        let dir = std::path::PathBuf::from(home).join(".config/autostart");
        let file = dir.join("langhian.desktop");
        if enabled {
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let content = format!(
                "[Desktop Entry]\nType=Application\nName=Langhian Translation\nExec=\"{}\" --minimized\nX-GNOME-Autostart-enabled=true\n",
                exe.display()
            );
            std::fs::write(file, content).map_err(|e| e.to_string())?;
        } else {
            let _ = std::fs::remove_file(file);
        }
        Ok(())
    }

    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").map_err(|e| e.to_string())?;
        let dir = std::path::PathBuf::from(home).join("Library/LaunchAgents");
        let file = dir.join("io.hamides-dev.langhian.plist");
        if enabled {
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let plist = format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>io.hamides-dev.langhian</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
        <string>--minimized</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"#,
                exe.display()
            );
            std::fs::write(file, plist).map_err(|e| e.to_string())?;
        } else {
            let _ = std::fs::remove_file(file);
        }
        Ok(())
    }
}

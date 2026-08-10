use directories::BaseDirs;

use crate::domain::target::{OperatingSystem, Target};

pub fn default_install_root(target: &Target) -> String {
    let Some(base_dirs) = BaseDirs::new() else {
        return fallback_install_root(target);
    };

    let path = match target.os {
        OperatingSystem::Windows => base_dirs.data_local_dir().join("Programs").join("Codex"),
        OperatingSystem::Macos => base_dirs
            .home_dir()
            .join("Applications")
            .join("ChatGPT.app"),
        _ => return fallback_install_root(target),
    };

    path.to_string_lossy().into_owned()
}

fn fallback_install_root(target: &Target) -> String {
    match target.os {
        OperatingSystem::Windows => "%LOCALAPPDATA%\\Programs\\Codex".to_string(),
        OperatingSystem::Macos => "~/Applications/ChatGPT.app".to_string(),
        _ => "Codex".to_string(),
    }
}

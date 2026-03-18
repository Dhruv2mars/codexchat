use std::path::PathBuf;

use anyhow::{Result, anyhow};

pub struct AppPaths {
    pub bin_dir: PathBuf,
    pub config_path: PathBuf,
    pub empty_workspace_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub root: PathBuf,
    pub threads_dir: PathBuf,
}

impl AppPaths {
    pub fn detect() -> Result<Self> {
        let home = dirs::home_dir().ok_or_else(|| anyhow!("home_directory_unavailable"))?;
        Ok(Self::from_root(home.join(".codexchat")))
    }

    pub fn from_root(root: PathBuf) -> Self {
        Self {
            bin_dir: root.join("bin"),
            config_path: root.join("config.json"),
            empty_workspace_dir: root.join("empty-workspace"),
            logs_dir: root.join("logs"),
            threads_dir: root.join("threads"),
            root,
        }
    }

    pub fn codex_bin_path(&self) -> PathBuf {
        if cfg!(windows) {
            self.bin_dir.join("codex.exe")
        } else {
            self.bin_dir.join("codex")
        }
    }
}

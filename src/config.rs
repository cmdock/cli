use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileConfig {
    pub server_url: Option<String>,
    pub admin_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedConfig {
    pub server_url: Option<String>,
    pub admin_token: Option<String>,
    pub config_path: PathBuf,
    pub no_color: bool,
}

pub fn default_config_path() -> PathBuf {
    if let Some(mut base) = dirs::config_dir() {
        base.push("cmdock-admin");
        base.push("config.toml");
        base
    } else {
        PathBuf::from(".cmdock-admin.toml")
    }
}

pub fn load_file_config(path: &Path) -> Result<FileConfig> {
    if !path.exists() {
        return Ok(FileConfig::default());
    }

    let raw = fs::read_to_string(path)
        .with_context(|| format!("failed to read config file {}", path.display()))?;
    let cfg = toml::from_str::<FileConfig>(&raw)
        .with_context(|| format!("failed to parse config file {}", path.display()))?;
    Ok(cfg)
}

pub fn save_file_config(path: &Path, cfg: &FileConfig) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create config directory {}", parent.display()))?;
    }

    let raw = toml::to_string_pretty(cfg).context("failed to serialize config file")?;
    fs::write(path, raw).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub fn resolve_config(
    path: Option<PathBuf>,
    server_flag: Option<String>,
    token_flag: Option<String>,
    no_color_flag: bool,
) -> Result<ResolvedConfig> {
    let config_path = path.unwrap_or_else(default_config_path);
    let file = load_file_config(&config_path)?;

    let server_url = server_flag
        .or_else(|| env::var("CMDOCK_ADMIN_SERVER").ok())
        .or(file.server_url);

    let admin_token = token_flag
        .or_else(|| env::var("CMDOCK_ADMIN_TOKEN").ok())
        .or(file.admin_token);

    let no_color = no_color_flag || env::var_os("NO_COLOR").is_some();

    Ok(ResolvedConfig {
        server_url,
        admin_token,
        config_path,
        no_color,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn resolve_prefers_flags_over_env_and_file() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("config.toml");
        save_file_config(
            &path,
            &FileConfig {
                server_url: Some("https://file.example.com".into()),
                admin_token: Some("file-token".into()),
            },
        )
        .unwrap();

        let cfg = resolve_config(
            Some(path.clone()),
            Some("https://flag.example.com".into()),
            Some("flag-token".into()),
            false,
        )
        .unwrap();

        assert_eq!(cfg.server_url.as_deref(), Some("https://flag.example.com"));
        assert_eq!(cfg.admin_token.as_deref(), Some("flag-token"));
        assert_eq!(cfg.config_path, path);
    }
}

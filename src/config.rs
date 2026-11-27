use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};

pub const DEFAULT_PORT: u16 = 7476;

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaTools {
    pub mkvmerge: Option<PathBuf>,
    pub mkvextract: Option<PathBuf>,
    pub mkvpropedit: Option<PathBuf>,
    /// Audio technical inspection; an absolute path or automatic discovery.
    pub ffprobe: Option<PathBuf>,
    /// CLI executable, not the Windows GUI application.
    pub mediainfo: Option<PathBuf>,
    pub mediainfo_dll: Option<PathBuf>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    pub port: u16,
    pub media_roots: Vec<PathBuf>,
    pub tools: MediaTools,
    #[serde(default)]
    pub recent_files: Vec<PathBuf>,
    #[serde(default)]
    pub recent_locations: Vec<PathBuf>,
    #[serde(default)]
    pub favourite_locations: Vec<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            port: DEFAULT_PORT,
            media_roots: vec![],
            tools: MediaTools::default(),
            recent_files: vec![],
            recent_locations: vec![],
            favourite_locations: vec![],
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        if self.version != 1 {
            bail!(
                "Unsupported configuration version {}; expected 1",
                self.version
            );
        }
        if self.port == 0 {
            bail!("Port must be between 1 and 65535");
        }
        for root in &self.media_roots {
            if !root.is_absolute() {
                bail!("Media roots must be absolute paths: {}", root.display());
            }
        }
        for path in [
            &self.tools.mkvmerge,
            &self.tools.mkvextract,
            &self.tools.mkvpropedit,
            &self.tools.ffprobe,
            &self.tools.mediainfo,
            &self.tools.mediainfo_dll,
        ]
        .into_iter()
        .flatten()
        {
            if !path.is_absolute() {
                bail!("Tool paths must be absolute: {}", path.display());
            }
        }
        Ok(())
    }

    pub fn load(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir).context("Creating configuration directory")?;
        let path = dir.join("config.json");
        let config = match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<Self>(&bytes).context("Invalid config.json")?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let config = Self::default();
                config.save(dir)?;
                config
            }
            Err(error) => return Err(error).context("Reading configuration"),
        };
        config.validate()?;
        Ok(config)
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        self.validate()?;
        fs::create_dir_all(dir)?;
        let mut temp = tempfile::NamedTempFile::new_in(dir)?;
        serde_json::to_writer_pretty(&mut temp, self)?;
        temp.write_all(b"\n")?;
        temp.as_file().sync_all()?;
        temp.persist(dir.join("config.json"))
            .context("Atomically replacing configuration")?;
        Ok(())
    }
}

pub fn config_dir() -> Result<PathBuf> {
    if let Some(dir) = env::var_os("TRACKSMITH_CONFIG_DIR") {
        let dir = PathBuf::from(dir);
        if !dir.is_absolute() {
            bail!("TRACKSMITH_CONFIG_DIR must be absolute");
        }
        return Ok(dir);
    }
    Ok(PathBuf::from(
        env::var_os("APPDATA").context("APPDATA is missing; set TRACKSMITH_CONFIG_DIR")?,
    )
    .join("Tracksmith"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn config_roundtrip_replacement_and_version_guard() {
        let dir = tempfile::tempdir().unwrap();
        let mut config = Config::load(dir.path()).unwrap();
        config.port = 7576;
        config.save(dir.path()).unwrap();
        assert_eq!(Config::load(dir.path()).unwrap().port, 7576);
        fs::write(
            dir.path().join("config.json"),
            br#"{"version":2,"port":7476,"media_roots":[],"tools":{}}"#,
        )
        .unwrap();
        assert!(Config::load(dir.path()).is_err());
    }
}

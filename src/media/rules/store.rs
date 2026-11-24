use super::{model::RuleLibrary, validate::validate};
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub struct RuleStore {
    dir: PathBuf,
}
impl RuleStore {
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
        }
    }
    pub fn load(&self) -> Result<RuleLibrary> {
        let path = self.dir.join("rules.json");
        let library = match fs::metadata(&path) {
            Ok(metadata) => {
                ensure!(
                    metadata.len() <= 2 * 1024 * 1024,
                    "Rules library exceeds 2 MiB"
                );
                serde_json::from_slice(&fs::read(&path)?).context("Invalid rules.json")?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => RuleLibrary::default(),
            Err(error) => return Err(error.into()),
        };
        validate(&library)?;
        Ok(library)
    }
    pub fn save(&self, mut library: RuleLibrary) -> Result<RuleLibrary> {
        validate(&library)?;
        fs::create_dir_all(&self.dir)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.dir.join("rules.lock"))?;
        lock.try_lock()
            .context("Another Tracksmith instance is saving rules")?;
        let current = self.load()?;
        ensure!(
            library.revision == current.revision,
            "Rules changed since this editor loaded them; reload before saving"
        );
        library.revision = current
            .revision
            .checked_add(1)
            .context("Rules revision overflow")?;
        let mut temp = tempfile::NamedTempFile::new_in(&self.dir)?;
        serde_json::to_writer_pretty(&mut temp, &library)?;
        temp.write_all(b"\n")?;
        temp.as_file().sync_all()?;
        temp.persist(self.dir.join("rules.json"))?;
        Ok(library)
    }
}

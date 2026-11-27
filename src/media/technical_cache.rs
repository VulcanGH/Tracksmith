//! Bounded technical facts cache. Editable metadata always comes from a fresh read.
use super::filesystem::Identity;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    fs,
    hash::{Hash, Hasher},
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

const MAX_RECORDS: usize = 64;
const MAX_BYTES: u64 = 4 * 1024 * 1024;
const MAX_DISK_RECORDS: usize = 4096;
const MAX_DISK_BYTES: u64 = 128 * 1024 * 1024;

#[derive(Clone, Serialize, Deserialize)]
struct Record {
    path: PathBuf,
    stamp: String,
    provider: String,
    facts: Value,
}

pub struct TechnicalCache {
    directory: PathBuf,
    records: Mutex<HashMap<PathBuf, Record>>,
}
impl TechnicalCache {
    pub fn invalidate(&self, path: &Path) -> Result<()> {
        self.records
            .lock()
            .map_err(|_| anyhow::anyhow!("Technical cache unavailable"))?
            .remove(path);
        match fs::remove_file(self.filename(path)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).context("Removing selected file's technical cache"),
        }
    }

    pub fn new(config: &Path) -> Self {
        Self {
            directory: config.join("technical-cache-v1"),
            records: Mutex::new(HashMap::new()),
        }
    }

    fn filename(&self, path: &Path) -> PathBuf {
        let mut hash = DefaultHasher::new();
        path.hash(&mut hash);
        self.directory.join(format!("{:016x}.json", hash.finish()))
    }

    pub fn load(&self, path: &Path, provider: &str) -> Result<Option<Value>> {
        let stamp = Identity::capture(path)?.cache_stamp()?;
        let mut records = self
            .records
            .lock()
            .map_err(|_| anyhow::anyhow!("Technical cache unavailable"))?;
        if let Some(record) = records.get(path)
            && record.stamp == stamp
            && record.provider == provider
        {
            return Ok(Some(record.facts.clone()));
        }
        records.remove(path);
        let filename = self.filename(path);
        let file = match fs::File::open(&filename) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        ensure!(
            file.metadata()?.len() <= MAX_BYTES,
            "Technical cache record exceeds limit"
        );
        let record: Record =
            serde_json::from_reader(file).context("Invalid technical cache record")?;
        if record.path != path || record.stamp != stamp || record.provider != provider {
            return Ok(None);
        }
        let facts = record.facts.clone();
        if records.len() >= MAX_RECORDS {
            records.clear();
        }
        records.insert(path.to_path_buf(), record);
        Ok(Some(facts))
    }

    pub fn store(&self, path: &Path, provider: &str, facts: &Value, stamp: &str) -> Result<()> {
        let record = Record {
            path: path.to_path_buf(),
            stamp: stamp.into(),
            provider: provider.into(),
            facts: facts.clone(),
        };
        let bytes = serde_json::to_vec(&record)?;
        ensure!(
            bytes.len() as u64 <= MAX_BYTES,
            "Technical cache record exceeds limit"
        );
        let mut records = self
            .records
            .lock()
            .map_err(|_| anyhow::anyhow!("Technical cache unavailable"))?;
        if records.len() >= MAX_RECORDS {
            records.clear();
        }
        records.insert(path.to_path_buf(), record);
        drop(records);
        fs::create_dir_all(&self.directory)?;
        let mut temp = tempfile::NamedTempFile::new_in(&self.directory)?;
        temp.write_all(&bytes)?;
        temp.as_file().sync_all()?;
        temp.persist(self.filename(path))?;
        let mut files = fs::read_dir(&self.directory)?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let name = entry.file_name();
                let name = name.to_str()?;
                let stem = name.strip_suffix(".json")?;
                if stem.len() != 16 || !stem.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return None;
                }
                let meta = entry.metadata().ok()?;
                meta.is_file()
                    .then(|| (meta.modified().ok(), entry.path(), meta.len()))
            })
            .collect::<Vec<_>>();
        files.sort_by_key(|(time, _, _)| *time);
        let mut total_bytes: u64 = files.iter().map(|(_, _, bytes)| bytes).sum();
        let mut total_records = files.len();
        for (_, filename, bytes) in &files {
            if total_records <= MAX_DISK_RECORDS && total_bytes <= MAX_DISK_BYTES {
                break;
            }
            fs::remove_file(filename)?;
            total_records -= 1;
            total_bytes = total_bytes.saturating_sub(*bytes);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn persistent_facts_require_matching_file_identity_and_provider() {
        let root = tempfile::tempdir().unwrap();
        let media = root.path().join("fixture.mka");
        fs::write(&media, "original").unwrap();
        let cache = TechnicalCache::new(root.path());
        cache
            .store(
                &media,
                "provider-1",
                &json!({"channels":8}),
                &Identity::capture(&media).unwrap().cache_stamp().unwrap(),
            )
            .unwrap();
        let restarted = TechnicalCache::new(root.path());
        assert_eq!(
            restarted.load(&media, "provider-1").unwrap(),
            Some(json!({"channels":8}))
        );
        assert!(restarted.load(&media, "provider-2").unwrap().is_none());
        fs::write(&media, "different content").unwrap();
        assert!(cache.load(&media, "provider-1").unwrap().is_none());
        fs::remove_file(&media).unwrap();
        fs::write(&media, "original").unwrap();
        assert!(restarted.load(&media, "provider-1").unwrap().is_none());
    }

    #[test]
    fn persistent_cache_retains_batches_larger_than_memory_cache() {
        let root = tempfile::tempdir().unwrap();
        let cache = TechnicalCache::new(root.path());
        let mut paths = Vec::new();
        for index in 0..MAX_RECORDS + 5 {
            let media = root.path().join(format!("{index}.mka"));
            fs::write(&media, "original").unwrap();
            cache
                .store(
                    &media,
                    "provider",
                    &json!({"channels": 8}),
                    &Identity::capture(&media).unwrap().cache_stamp().unwrap(),
                )
                .unwrap();
            paths.push(media);
        }
        let restarted = TechnicalCache::new(root.path());
        for media in &paths {
            assert_eq!(
                restarted.load(media, "provider").unwrap(),
                Some(json!({"channels": 8}))
            );
        }
        assert!(restarted.load(&paths[0], "provider").unwrap().is_some());
        restarted.invalidate(&paths[0]).unwrap();
        assert!(restarted.load(&paths[0], "provider").unwrap().is_none());
        assert!(
            TechnicalCache::new(root.path())
                .load(&paths[0], "provider")
                .unwrap()
                .is_none()
        );
        assert!(restarted.load(&paths[1], "provider").unwrap().is_some());
    }
}

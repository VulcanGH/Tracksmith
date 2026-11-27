use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, File, OpenOptions},
    hash::{DefaultHasher, Hash, Hasher},
    path::{Component, Path, PathBuf},
    time::SystemTime,
};

pub struct Boundary {
    roots: Vec<PathBuf>,
    selected_files: Vec<PathBuf>,
    selected_folders: Vec<PathBuf>,
}
impl Boundary {
    pub fn new(roots: &[PathBuf]) -> Result<Self> {
        let mut resolved = Vec::new();
        for root in roots {
            check_components(root)?;
            reject_links(root)?;
            let canonical = fs::canonicalize(root)
                .with_context(|| format!("Media location unavailable: {}", root.display()))?;
            ensure!(canonical.is_dir(), "Media location must be a folder");
            resolved.push(canonical);
        }
        Ok(Self {
            roots: resolved,
            selected_files: vec![],
            selected_folders: vec![],
        })
    }
    pub fn roots(&self) -> Vec<String> {
        self.roots.iter().map(|p| p.display().to_string()).collect()
    }
    pub fn authorize(&self, path: &Path) -> Result<PathBuf> {
        check_components(path)?;
        reject_links(path)?;
        let canonical = fs::canonicalize(path).context("File is unavailable or inaccessible")?;
        ensure!(
            self.roots
                .iter()
                .chain(&self.selected_folders)
                .any(|root| canonical.starts_with(root))
                || self.selected_files.contains(&canonical),
            "File is outside configured media locations; use Open file to select it directly"
        );
        ensure!(canonical.is_file(), "Select a regular media file");
        // Windows alternate data streams are rejected before canonicalisation.
        Ok(canonical)
    }
    /// Called only with an actual native-picker result, never a path supplied by an HTTP request.
    pub(crate) fn grant_selected_file(&mut self, path: &Path) -> Result<PathBuf> {
        check_components(path)?;
        reject_links(path)?;
        let canonical = fs::canonicalize(path).context("Selected file is unavailable")?;
        ensure!(canonical.is_file(), "Select a regular media file");
        let extension = canonical
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        ensure!(
            extension.eq_ignore_ascii_case("mkv") || extension.eq_ignore_ascii_case("mka"),
            "Select an MKV or MKA file"
        );
        if !self.selected_files.contains(&canonical) {
            if self.selected_files.len() >= 32 {
                self.selected_files.remove(0);
            }
            self.selected_files.push(canonical.clone());
        }
        Ok(canonical)
    }
    pub fn folders(&self) -> Vec<PathBuf> {
        self.roots
            .iter()
            .chain(&self.selected_folders)
            .cloned()
            .collect()
    }
    pub fn known_grant(&self, path: &Path, folder: bool) -> bool {
        self.roots
            .iter()
            .chain(&self.selected_folders)
            .any(|root| path.starts_with(root))
            || (!folder && self.selected_files.iter().any(|file| file == path))
    }
    pub fn keep_folder(&mut self, path: PathBuf) {
        if !self.roots.contains(&path) {
            self.roots.push(path);
        }
    }
    pub fn authorize_directory(&self, path: &Path) -> Result<PathBuf> {
        check_components(path)?;
        reject_links(path)?;
        let canonical = fs::canonicalize(path).context("Folder is unavailable or inaccessible")?;
        ensure!(canonical.is_dir(), "Choose a folder");
        ensure!(
            self.roots
                .iter()
                .chain(&self.selected_folders)
                .any(|root| canonical.starts_with(root)),
            "Folder access has not been granted; use Choose folder"
        );
        Ok(canonical)
    }
    pub(crate) fn grant_selected_folder(&mut self, path: &Path) -> Result<PathBuf> {
        let boundary = Self::new(&[path.to_path_buf()])?;
        let canonical = boundary.roots[0].clone();
        if !self.selected_folders.contains(&canonical) {
            if self.selected_folders.len() >= 16 {
                self.selected_folders.remove(0);
            }
            self.selected_folders.push(canonical.clone());
        }
        Ok(canonical)
    }
}
fn check_components(path: &Path) -> Result<()> {
    ensure!(path.is_absolute(), "An absolute path is required");
    for component in path.components() {
        ensure!(
            !matches!(component, Component::ParentDir),
            "Parent traversal is not allowed"
        );
        if let Component::Normal(name) = component {
            ensure!(
                !name.to_string_lossy().contains([':', '\0']),
                "Alternate data streams and invalid path components are not allowed"
            );
        }
        #[cfg(windows)]
        if let Component::Prefix(prefix) = component {
            use std::path::Prefix;
            ensure!(
                matches!(
                    prefix.kind(),
                    Prefix::Disk(_)
                        | Prefix::UNC(_, _)
                        | Prefix::VerbatimDisk(_)
                        | Prefix::VerbatimUNC(_, _)
                ),
                "Device paths are not supported"
            );
        }
    }
    Ok(())
}
fn reject_links(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor).context("Checking filesystem boundaries")?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "Symbolic links are not supported in media paths"
        );
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            ensure!(
                metadata.file_attributes() & 0x400 == 0,
                "Reparse points are not supported in media paths"
            );
        }
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
pub struct Stamp {
    len: u64,
    modified: SystemTime,
    created: Option<SystemTime>,
}
pub struct Identity {
    _handle: File,
    key: FileKey,
    stamp: Stamp,
}
#[derive(PartialEq, Eq, Hash, serde::Serialize)]
struct FileKey {
    volume: u64,
    id: [u8; 16],
}
#[cfg(windows)]
fn file_key(file: &File) -> Result<FileKey> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ID_INFO, FileIdInfo, GetFileInformationByHandleEx,
    };
    let mut info: FILE_ID_INFO = unsafe { std::mem::zeroed() };
    // SAFETY: live file handle and correctly sized writable FILE_ID_INFO buffer.
    let ok = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileIdInfo,
            std::ptr::from_mut(&mut info).cast(),
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )
    };
    ensure!(
        ok != 0,
        "Filesystem cannot provide a stable 128-bit file identity: {}",
        std::io::Error::last_os_error()
    );
    Ok(FileKey {
        volume: info.VolumeSerialNumber,
        id: info.FileId.Identifier,
    })
}
#[cfg(unix)]
fn file_key(file: &File) -> Result<FileKey> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    Ok(FileKey {
        volume: metadata.dev(),
        id: u128::from(metadata.ino()).to_le_bytes(),
    })
}
impl Identity {
    pub fn cache_stamp(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string(&(&self.key, &self.stamp))?)
    }

    pub fn capture(path: &Path) -> Result<Self> {
        let handle = File::open(path)?;
        let metadata = handle.metadata()?;
        let key = file_key(&handle)?;
        Ok(Self {
            _handle: handle,
            key,
            stamp: Stamp {
                len: metadata.len(),
                modified: metadata.modified()?,
                created: metadata.created().ok(),
            },
        })
    }
    pub fn verify(&self, path: &Path) -> Result<()> {
        let current = Self::capture(path)?;
        ensure!(
            self.key == current.key && self.stamp == current.stamp,
            "File changed or was replaced since inspection; refresh before saving"
        );
        Ok(())
    }
    pub fn same_file(&self, path: &Path) -> Result<bool> {
        Ok(self.key == file_key(&File::open(path)?)?)
    }
    pub fn lock_operation(&self) -> Result<File> {
        let mut hasher = DefaultHasher::new();
        self.key.hash(&mut hasher);
        let folder = std::env::temp_dir().join("Tracksmith-operation-locks");
        fs::create_dir_all(&folder)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(folder.join(format!("{:016x}.lock", hasher.finish())))?;
        file.try_lock()
            .context("Another Tracksmith instance is processing this file")?;
        Ok(file)
    }
}

pub fn hold_file_identity(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Permit the utility's read/write access but deny replacement/deletion during a save.
        options.share_mode(3);
    }
    options
        .open(path)
        .context("Cannot hold file identity during save")
}

pub fn preflight_write(path: &Path) -> Result<File> {
    ensure!(
        !fs::metadata(path)?.permissions().readonly(),
        "File is read-only"
    );
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        // Deny concurrent write/delete handles during the access probe; release before mkvpropedit.
        options.share_mode(1);
    }
    options
        .open(path)
        .context("File is locked or write access is denied")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn boundaries_and_changed_file_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let allowed = root.path().join("allowed");
        fs::create_dir(&allowed).unwrap();
        let file = allowed.join("fixture.mkv");
        fs::write(&file, b"one").unwrap();
        let outside = root.path().join("outside.mkv");
        fs::write(&outside, b"two").unwrap();
        let mut boundary = Boundary::new(&[allowed]).unwrap();
        assert!(boundary.authorize(&file).is_ok());
        assert!(boundary.authorize(&outside).is_err());
        let sibling = root.path().join("unselected.mkv");
        fs::write(&sibling, b"sibling").unwrap();
        boundary.grant_selected_file(&outside).unwrap();
        assert!(boundary.authorize(&outside).is_ok());
        assert!(boundary.authorize(&sibling).is_err());
        let identity = Identity::capture(&file).unwrap();
        let _operation = identity.lock_operation().unwrap();
        assert!(identity.lock_operation().is_err());
        fs::write(&file, b"changed length").unwrap();
        assert!(identity.verify(&file).is_err());
    }
}

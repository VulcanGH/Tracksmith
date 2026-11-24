use super::{filesystem::Boundary, model::*};
use crate::config::Config;
use anyhow::{Context, Result, ensure};
use std::{collections::HashSet, fs, path::Path};

pub fn list(
    boundary: &Boundary,
    config: &Config,
    path: Option<&Path>,
    offset: usize,
    search: &str,
) -> Result<DirectoryListing> {
    ensure!(
        offset <= 5000 && search.len() <= 256,
        "Browser request exceeds its limits"
    );
    let mut locations = vec![];
    let mut seen = HashSet::new();
    for (kind, paths) in [
        ("favourite", config.favourite_locations.clone()),
        ("folder", boundary.folders()),
        ("recent", config.recent_locations.clone()),
    ] {
        for path in paths {
            if seen.insert(path.clone()) {
                locations.push(BrowserLocation {
                    path: path.display().to_string(),
                    kind: kind.into(),
                    authorized: boundary.known_grant(&path, true),
                    favourite: config.favourite_locations.contains(&path),
                });
            }
        }
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::GetLogicalDrives;
        // Only enumerates drive letters; contents still require an explicit folder grant.
        let mask = unsafe { GetLogicalDrives() };
        for index in 0..26 {
            if mask & (1 << index) != 0 {
                let path = format!("{}:\\", char::from(b'A' + index));
                locations.push(BrowserLocation {
                    authorized: boundary.known_grant(Path::new(&format!("\\\\?\\{path}")), true),
                    path,
                    kind: "drive".into(),
                    favourite: false,
                });
            }
        }
    }
    let recent_files = config
        .recent_files
        .iter()
        .map(|path| BrowserLocation {
            path: path.display().to_string(),
            kind: "file".into(),
            authorized: boundary.known_grant(path, false),
            favourite: false,
        })
        .collect();
    let mut result = DirectoryListing {
        offset,
        path: None,
        parent: None,
        locations,
        recent_files,
        entries: vec![],
        next_offset: None,
        total_entries: 0,
        truncated: false,
        notes: vec![],
    };
    let Some(path) = path else {
        return Ok(result);
    };
    let canonical = boundary.authorize_directory(path)?;
    result.path = Some(canonical.display().to_string());
    result.parent = canonical
        .parent()
        .and_then(|parent| boundary.authorize_directory(parent).ok())
        .map(|p| p.display().to_string());
    let query = search.to_lowercase();
    let mut inaccessible = 0;
    for (index, entry) in fs::read_dir(&canonical)
        .context("Reading media folder")?
        .enumerate()
    {
        if index >= 5000 {
            result.truncated = true;
            break;
        }
        let entry = match entry {
            Ok(entry) => entry,
            Err(_) => {
                inaccessible += 1;
                continue;
            }
        };
        let path = entry.path();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(_) => {
                inaccessible += 1;
                continue;
            }
        };
        let reparse = metadata.file_type().is_symlink();
        #[cfg(windows)]
        let reparse = {
            use std::os::windows::fs::MetadataExt;
            reparse || metadata.file_attributes() & 0x400 != 0
        };
        if reparse {
            inaccessible += 1;
            continue;
        }
        let folder = metadata.is_dir();
        let media = path
            .extension()
            .and_then(|v| v.to_str())
            .is_some_and(|v| v.eq_ignore_ascii_case("mkv") || v.eq_ignore_ascii_case("mka"));
        if !folder && !(metadata.is_file() && media) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.to_lowercase().contains(&query) {
            continue;
        }
        result.entries.push(BrowserEntry {
            name,
            path: path.display().to_string(),
            kind: if folder { "folder" } else { "file" }.into(),
            size_bytes: (!folder).then(|| metadata.len().to_string()),
        });
    }
    ensure!(
        boundary.authorize_directory(path)? == canonical,
        "Folder changed during browsing; reopen it"
    );
    result.entries.sort_by(|a, b| {
        (a.kind != "folder", a.name.to_lowercase())
            .cmp(&(b.kind != "folder", b.name.to_lowercase()))
    });
    result.total_entries = result.entries.len();
    result.next_offset =
        (offset.saturating_add(200) < result.total_entries).then_some(offset + 200);
    result.entries = result.entries.into_iter().skip(offset).take(200).collect();
    if inaccessible > 0 {
        result.notes.push(format!(
            "{inaccessible} inaccessible entries or links were excluded"
        ));
    }
    if result.truncated {
        result.notes.push(
            "Only the first 5,000 directory entries were scanned; use Open file for other files"
                .into(),
        );
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_lists_only_granted_folders_and_media() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("media");
        fs::create_dir(&folder).unwrap();
        fs::create_dir(folder.join("Season 1")).unwrap();
        fs::write(folder.join("Média.mkv"), b"fixture placeholder").unwrap();
        fs::write(folder.join("ignore.txt"), b"ignored").unwrap();
        let mut boundary = Boundary::new(&[]).unwrap();
        assert!(list(&boundary, &Config::default(), Some(&folder), 0, "").is_err());
        boundary.grant_selected_folder(&folder).unwrap();
        let page = list(&boundary, &Config::default(), Some(&folder), 0, "").unwrap();
        assert_eq!(page.total_entries, 2);
        assert_eq!(page.entries[0].kind, "folder");
        assert_eq!(page.entries[1].name, "Média.mkv");
        assert!(page.parent.is_none());
        assert_eq!(
            list(&boundary, &Config::default(), Some(&folder), 1, "")
                .unwrap()
                .entries
                .len(),
            1
        );
        assert_eq!(
            list(&boundary, &Config::default(), Some(&folder), 0, "MÉDIA")
                .unwrap()
                .total_entries,
            1
        );
        assert!(list(&boundary, &Config::default(), Some(root.path()), 0, "").is_err());
    }
}

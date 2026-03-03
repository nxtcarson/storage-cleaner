use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use chrono::{TimeZone, Utc};

const SKIP_DIRS: [&str; 4] = [
    "$RECYCLE.BIN",
    "System Volume Information",
    "pagefile.sys",
    "hiberfil.sys",
];

fn should_skip(path: &Path) -> bool {
    path.components()
        .any(|c| SKIP_DIRS.contains(&c.as_os_str().to_string_lossy().as_ref()))
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FileCategory {
    Documents,
    Media,
    Archives,
    Executables,
    System,
    Temp,
    DevBuild,
    Other,
}

impl FileCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            FileCategory::Documents => "Documents",
            FileCategory::Media => "Media",
            FileCategory::Archives => "Archives",
            FileCategory::Executables => "Executables",
            FileCategory::System => "System",
            FileCategory::Temp => "Temp",
            FileCategory::DevBuild => "Dev/Build",
            FileCategory::Other => "Other",
        }
    }
}

fn classify_file(path: &Path) -> FileCategory {
    let path_str = path.to_string_lossy().to_lowercase();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if path_str.contains("\\temp\\")
        || path_str.contains("/temp/")
        || path_str.contains("\\tmp\\")
        || path_str.contains("\\appdata\\local\\temp")
        || path_str.contains("\\cache\\")
    {
        return FileCategory::Temp;
    }
    if path_str.contains("\\windows\\")
        || path_str.contains("\\program files")
        || path_str.contains("\\programdata\\")
    {
        return FileCategory::System;
    }
    if path_str.contains("\\node_modules\\")
        || path_str.contains("\\.git\\")
        || path_str.contains("\\target\\")
        || path_str.contains("\\build\\")
        || path_str.contains("\\obj\\")
        || path_str.contains("\\.next\\")
        || path_str.contains("\\dist\\")
    {
        return FileCategory::DevBuild;
    }

    match ext.as_str() {
        "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "txt" | "rtf" | "odt" => {
            FileCategory::Documents
        }
        "mp4" | "mkv" | "avi" | "mov" | "wmv" | "mp3" | "flac" | "wav" | "jpg" | "jpeg" | "png"
        | "gif" | "webp" | "bmp" | "raw" => FileCategory::Media,
        "zip" | "rar" | "7z" | "tar" | "gz" | "bz2" | "xz" | "iso" => FileCategory::Archives,
        "exe" | "dll" | "msi" => FileCategory::Executables,
        _ => FileCategory::Other,
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct FileEntry {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub extension: String,
    pub last_modified: Option<chrono::DateTime<chrono::Utc>>,
    pub category: FileCategory,
}

#[derive(Clone, Default, Debug)]
pub struct ScanState {
    pub files_scanned: u64,
    pub is_done: bool,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ExtensionStats {
    pub extension: String,
    pub total_bytes: u64,
    pub file_count: u64,
}

#[derive(Clone, Debug, Default)]
pub struct FolderStats {
    pub path: PathBuf,
    pub total_bytes: u64,
    pub file_count: u64,
}

#[derive(Clone, Debug)]
pub struct CategoryStats {
    pub category: FileCategory,
    pub total_bytes: u64,
    pub file_count: u64,
}

#[derive(Clone, Debug)]
pub struct ScanResult {
    pub drive: PathBuf,
    pub entries: Vec<FileEntry>,
    pub by_extension: Vec<ExtensionStats>,
    pub by_folder: Vec<FolderStats>,
    pub by_category: Vec<CategoryStats>,
    pub largest_files: Vec<FileEntry>,
    pub stale_files: Vec<FileEntry>,
    pub total_bytes: u64,
}

const STALE_DAYS: i64 = 180;

pub(crate) fn compute_insights(entries: Vec<FileEntry>, drive: PathBuf) -> ScanResult {
    let total_bytes: u64 = entries.iter().map(|e| e.size_bytes).sum();
    let cutoff = Utc::now() - chrono::Duration::days(STALE_DAYS);

    let mut by_ext: HashMap<String, (u64, u64)> = HashMap::new();
    let mut by_folder: HashMap<PathBuf, (u64, u64)> = HashMap::new();
    let mut by_cat: HashMap<FileCategory, (u64, u64)> = HashMap::new();

    for e in &entries {
        let (size, count) = by_ext.get(&e.extension).copied().unwrap_or((0, 0));
        by_ext.insert(e.extension.clone(), (size + e.size_bytes, count + 1));

        let top_folder = e
            .path
            .parent()
            .and_then(|p| {
                p.strip_prefix(&drive)
                    .ok()
                    .and_then(|rest| rest.components().next())
                    .map(|c| drive.join(c.as_os_str()))
            })
            .unwrap_or_else(|| drive.clone());
        let (s, c) = by_folder.get(&top_folder).copied().unwrap_or((0, 0));
        by_folder.insert(top_folder, (s + e.size_bytes, c + 1));

        let (s, c) = by_cat.get(&e.category).copied().unwrap_or((0, 0));
        by_cat.insert(e.category.clone(), (s + e.size_bytes, c + 1));
    }

    let by_extension: Vec<ExtensionStats> = by_ext
        .into_iter()
        .map(|(ext, (total_bytes, file_count))| ExtensionStats {
            extension: ext,
            total_bytes,
            file_count,
        })
        .filter(|s| !s.extension.is_empty())
        .collect();
    let mut by_extension = by_extension;
    by_extension.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes));

    let by_folder: Vec<FolderStats> = by_folder
        .into_iter()
        .map(|(path, (total_bytes, file_count))| FolderStats {
            path,
            total_bytes,
            file_count,
        })
        .collect();
    let mut by_folder = by_folder;
    by_folder.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes));

    let by_category: Vec<CategoryStats> = by_cat
        .into_iter()
        .map(|(category, (total_bytes, file_count))| CategoryStats {
            category,
            total_bytes,
            file_count,
        })
        .collect();
    let mut by_category = by_category;
    by_category.sort_by(|a, b| b.total_bytes.cmp(&a.total_bytes));

    let mut largest: Vec<FileEntry> = entries.clone();
    largest.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    let largest_files = largest.into_iter().take(200).collect::<Vec<_>>();

    let mut stale: Vec<FileEntry> = entries
        .into_iter()
        .filter(|e| e.last_modified.map_or(true, |t| t < cutoff))
        .collect();
    stale.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    let stale_files = stale.into_iter().take(500).collect::<Vec<_>>();

    ScanResult {
        drive,
        entries: largest_files.clone(),
        by_extension,
        by_folder,
        by_category,
        largest_files,
        stale_files,
        total_bytes,
    }
}

#[cfg(windows)]
fn get_drive_letter(path: &Path) -> Option<char> {
    let s = path.to_string_lossy();
    let s = s.trim_end_matches(|c| c == '\\' || c == '/');
    if s.len() >= 2 && s.ends_with(':') {
        s.chars().next()
    } else {
        None
    }
}

#[cfg(windows)]
pub(crate) fn scan_drive_mft(
    drive_letter: char,
    state: &Arc<Mutex<ScanState>>,
) -> Option<Vec<FileEntry>> {
    use std::ffi::OsString;

    let volume = usn_journal_rs::volume::Volume::from_drive_letter(drive_letter).ok()?;
    let mft = usn_journal_rs::mft::Mft::new(&volume);
    let drive_root = format!("{}:\\", drive_letter);

    let mut entries_map: HashMap<u64, (u64, OsString)> = HashMap::new();
    let mut file_fids: Vec<u64> = Vec::new();
    let mut count = 0u64;

    for result in mft.iter() {
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };
        let name = entry.file_name.clone();
        let fid = entry.fid;
        let parent_fid = entry.parent_fid;
        let name_str = name.to_string_lossy();
        if name_str == "." || name_str == ".." {
            continue;
        }
        entries_map.insert(fid, (parent_fid, name));
        if !entry.is_dir() {
            file_fids.push(fid);
            count += 1;
        }
        if count % 50000 == 0 {
            if let Ok(mut s) = state.lock() {
                s.files_scanned = count;
            }
        }
    }

    let mut path_cache: HashMap<u64, PathBuf> = HashMap::new();
    path_cache.insert(5, PathBuf::from(&drive_root));

    fn resolve_path(
        fid: u64,
        entries: &HashMap<u64, (u64, std::ffi::OsString)>,
        cache: &mut HashMap<u64, PathBuf>,
        drive_root: &str,
    ) -> Option<PathBuf> {
        if let Some(cached) = cache.get(&fid) {
            return Some(cached.clone());
        }
        let (parent_fid, name) = entries.get(&fid)?;
        let parent_path = if *parent_fid == 5 || *parent_fid == 0 {
            PathBuf::from(drive_root)
        } else {
            resolve_path(*parent_fid, entries, cache, drive_root)?
        };
        let full = parent_path.join(&*name);
        cache.insert(fid, full.clone());
        Some(full)
    }

    let mut fid_paths: Vec<(u64, PathBuf)> = file_fids
        .iter()
        .filter_map(|&fid| {
            resolve_path(fid, &entries_map, &mut path_cache, &drive_root)
                .filter(|p| !should_skip(p))
                .map(|path| (fid, path))
        })
        .collect();

    const RECORD_NUM_MASK: u64 = 0x0000_FFFF_FFFF_FFFF;
    fid_paths.sort_by_key(|(fid, _)| fid & RECORD_NUM_MASK);

    let volume_path = format!(r"\\.\{}:", drive_letter);
    let mut file = std::fs::File::open(&volume_path).ok()?;
    let ntfs = ntfs::Ntfs::new(&mut file).ok()?;

    let mut entries = Vec::with_capacity(fid_paths.len());
    for (i, (fid, path)) in fid_paths.into_iter().enumerate() {
        let record_num = fid & RECORD_NUM_MASK;
        let ntfs_file = match ntfs.file(&mut file, record_num) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let size = ntfs_file.data_size() as u64;
        const NT_EPOCH_OFFSET: i64 = 11644473600;
        let last_modified = ntfs_file
            .info()
            .ok()
            .and_then(|info| {
                let nt = info.modification_time().nt_timestamp();
                let unix_secs = (nt as i64 / 10_000_000) - NT_EPOCH_OFFSET;
                Utc.timestamp_opt(unix_secs, 0).single()
            });
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_string();
        let category = classify_file(&path);
        entries.push(FileEntry {
            path,
            size_bytes: size,
            extension,
            last_modified,
            category,
        });
        if (i + 1) % 50000 == 0 {
            if let Ok(mut s) = state.lock() {
                s.files_scanned = (i + 1) as u64;
            }
        }
    }

    if let Ok(mut s) = state.lock() {
        s.files_scanned = entries.len() as u64;
    }

    Some(entries)
}

#[cfg(windows)]
fn scan_via_service(
    drive: char,
    state: &Arc<Mutex<ScanState>>,
) -> Option<ScanResult> {
    use crate::service_ipc::windows::{connect_client, recv_response, send_request, Request, Response};

    let mut stream = connect_client().ok()?;
    send_request(&mut stream, &Request::Scan { drive }).ok()?;

    let mut entries = Vec::new();
    loop {
        let resp = recv_response(&mut stream).ok()?;
        match resp {
            Response::File(wire) => {
                entries.push(wire.to_entry());
                if entries.len() % 50000 == 0 {
                    if let Ok(mut s) = state.lock() {
                        s.files_scanned = entries.len() as u64;
                    }
                }
            }
            Response::Done => break,
            Response::Error(_) => return None,
            Response::Ok => {}
        }
    }

    if let Ok(mut s) = state.lock() {
        s.files_scanned = entries.len() as u64;
    }
    let drive_path = PathBuf::from(format!("{}:\\", drive));
    Some(compute_insights(entries, drive_path))
}

pub fn scan_drive(
    root: &Path,
    state: Arc<Mutex<ScanState>>,
) -> Option<ScanResult> {
    #[cfg(windows)]
    if let Some(drive) = get_drive_letter(root) {
        if let Some(result) = scan_via_service(drive, &state) {
            state.lock().unwrap().is_done = true;
            return Some(result);
        }
        if let Some(entries) = scan_drive_mft(drive, &state) {
            let drive_path = PathBuf::from(format!("{}:\\", drive));
            let result = compute_insights(entries, drive_path);
            state.lock().unwrap().is_done = true;
            return Some(result);
        }
    }

    let mut entries = Vec::new();
    let walk = jwalk::WalkDir::new(root)
        .parallelism(jwalk::Parallelism::RayonDefaultPool {
            busy_timeout: std::time::Duration::from_secs(1),
        })
        .skip_hidden(false);

    for entry in walk {
        if let Ok(e) = entry {
            let path = e.path();
            if path.is_file() && !should_skip(&path) {
                if let Ok(meta) = std::fs::metadata(&path) {
                    let size = meta.len();
                    let last_modified = meta.modified().ok().and_then(|t| {
                        let d = t.duration_since(std::time::UNIX_EPOCH).ok()?;
                        Utc.timestamp_opt(d.as_secs() as i64, d.subsec_nanos())
                            .single()
                    });
                    let extension = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_string();
                    let category = classify_file(&path);
                    entries.push(FileEntry {
                        path,
                        size_bytes: size,
                        extension,
                        last_modified,
                        category,
                    });
                }
                if let Ok(mut s) = state.lock() {
                    s.files_scanned = entries.len() as u64;
                }
            }
        }
    }

    state.lock().unwrap().is_done = true;
    Some(compute_insights(entries, root.to_path_buf()))
}

#[derive(Clone)]
pub struct BigFileEntry {
    pub path: PathBuf,
    pub size_bytes: u64,
}

pub fn scan_big_files(
    root: &Path,
    min_size_mb: u64,
    state: Arc<Mutex<ScanState>>,
) -> Vec<BigFileEntry> {
    if let Some(result) = scan_drive(root, state) {
        let min_bytes = min_size_mb * 1024 * 1024;
        result
            .largest_files
            .into_iter()
            .filter(|e| e.size_bytes >= min_bytes)
            .map(|e| BigFileEntry {
                path: e.path,
                size_bytes: e.size_bytes,
            })
            .collect()
    } else {
        vec![]
    }
}

const SKIP_EXE_PATHS: [&str; 3] = ["Windows", "Program Files", "Program Files (x86)"];

fn should_skip_exe_path(path: &Path) -> bool {
    path.components()
        .any(|c| SKIP_EXE_PATHS.contains(&c.as_os_str().to_string_lossy().as_ref()))
}

#[derive(Clone)]
pub struct ExeEntry {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub last_modified: Option<chrono::DateTime<chrono::Utc>>,
    pub last_run: Option<chrono::DateTime<chrono::Utc>>,
}

pub fn scan_executables(
    root: &Path,
    prefetch_map: &HashMap<String, chrono::DateTime<chrono::Utc>>,
) -> Vec<ExeEntry> {
    let mut results = Vec::new();

    let walk = jwalk::WalkDir::new(root)
        .parallelism(jwalk::Parallelism::RayonDefaultPool {
            busy_timeout: std::time::Duration::from_secs(1),
        })
        .skip_hidden(false);

    for entry in walk {
        if let Ok(e) = entry {
            let path = e.path();
            if path.is_file() {
                if should_skip(&path) || should_skip_exe_path(&path) {
                    continue;
                }
                if path.extension().map_or(false, |e| {
                    e.to_str().map_or(false, |s| s.eq_ignore_ascii_case("exe"))
                }) {
                    if let Ok(meta) = std::fs::metadata(&path) {
                        let size = meta.len();
                        let last_modified = meta.modified().ok().and_then(|t| {
                            let d = t.duration_since(std::time::UNIX_EPOCH).ok()?;
                            Utc.timestamp_opt(d.as_secs() as i64, d.subsec_nanos())
                                .single()
                        });
                        let exe_name = path
                            .file_stem()
                            .map(|s| s.to_string_lossy().to_uppercase())
                            .unwrap_or_default();
                        let last_run = prefetch_map.get(&exe_name).copied();
                        results.push(ExeEntry {
                            path,
                            size_bytes: size,
                            last_modified,
                            last_run,
                        });
                    }
                }
            }
        }
    }

    results
}

pub fn get_drives() -> Vec<PathBuf> {
    let mut drives = Vec::new();
    #[cfg(windows)]
    {
        for letter in b'A'..=b'Z' {
            let drive = format!("{}:\\", letter as char);
            let path = PathBuf::from(&drive);
            if path.exists() {
                drives.push(path);
            }
        }
    }
    #[cfg(not(windows))]
    {
        if let Ok(entries) = std::fs::read_dir("/") {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    drives.push(path);
                }
            }
        }
    }
    drives
}

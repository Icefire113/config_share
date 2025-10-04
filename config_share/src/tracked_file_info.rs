use std::path::PathBuf;

pub struct FileInfo {
    file_path: PathBuf,
    // xxhash3 hash of the local file contents
    hash: u64,
}
struct share_settings {
    tracked_files: Vec<PathBuf>,
}

impl Default for share_settings {
    fn default() -> Self {
        Self {
            tracked_files: Vec::new(),
        }
    }
}

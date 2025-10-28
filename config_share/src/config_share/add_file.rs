use std::{error::Error, fmt::Display, path::Path};

use util::dprint;

use crate::config_share::ConfigShare;

#[derive(Debug)]
struct FileNotFoundError {
    file_name: String,
}

impl Display for FileNotFoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "The file {} could not be found, or is not a file",
            self.file_name
        )
    }
}

impl Error for FileNotFoundError {}
impl FileNotFoundError {
    fn new(file_name: &str) -> Self {
        Self {
            file_name: file_name.to_string(),
        }
    }
}

impl ConfigShare {
    pub fn add_file(&mut self, file_path: &String) -> Result<(), Box<dyn std::error::Error>> {
        dprint(format!("[add_file.add_file] Adding file: {}", file_path));
        let path = Path::new(file_path);

        if !path.is_file() {
            return Err(Box::new(FileNotFoundError::new(file_path)));
        }

        Ok(())
    }
}

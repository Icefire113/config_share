use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter};

#[derive(Serialize, Deserialize, EnumIter, Display, PartialEq, Debug)]
pub enum FileDifferentationMethod {
    #[strum(to_string = "File Name")]
    FileName,
    #[strum(to_string = "Custom Name")]
    CustomName(String),
}

impl FileDifferentationMethod {
    /// Returns the custom name if it exists
    pub fn get_custom_name(&self) -> Option<&String> {
        match self {
            FileDifferentationMethod::CustomName(name) => Some(name),
            _ => None,
        }
    }
}

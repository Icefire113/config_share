use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter};

#[derive(Serialize, Deserialize, EnumIter, Display, PartialEq)]
pub enum FileDifferentationMethod {
    #[strum(to_string = "File Name")]
    FileName,
    #[strum(to_string = "Custom Name")]
    CustomName,
}

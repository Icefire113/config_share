use std::fmt::Display;

#[derive(Debug)]
pub(crate) enum FnError {
    WithMessage(String),
    Empty,
}

impl FnError {
    pub fn message<T: Into<String>>(msg: T) -> Self {
        Self::WithMessage(msg.into())
    }
}

impl Display for FnError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FnError::WithMessage(msg) => write!(f, "{}", msg),
            FnError::Empty => write!(f, ""),
        }
    }
}

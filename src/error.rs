use thiserror::Error;

#[derive(Debug, Error)]
pub enum SkillError {
    #[error("{0}")]
    Parse(#[from] ParseError),
    #[error("{0}")]
    Validation(#[from] ValidationError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{message}")]
    Other { message: String },
}

pub type Result<T> = std::result::Result<T, SkillError>;

#[derive(Debug, Error)]
#[error("{message}")]
pub struct ParseError {
    pub message: String,
}

impl ParseError {
    pub fn new(message: String) -> Self {
        Self { message }
    }
}

#[derive(Debug, Error)]
#[error("{message}")]
pub struct ValidationError {
    pub message: String,
    pub errors: Vec<String>,
}

impl ValidationError {
    pub fn single(message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            errors: vec![message.clone()],
            message,
        }
    }

    pub fn multiple(errors: Vec<String>) -> Self {
        let message = errors.join("; ");
        Self { message, errors }
    }
}

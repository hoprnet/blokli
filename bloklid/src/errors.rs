use validator::ValidationErrors;

#[derive(Debug, thiserror::Error)]
pub enum BloklidError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    ConfigError(#[from] ConfigError),

    #[error("Unspecified error: {0}")]
    NonSpecificError(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("no configuration provided")]
    NoConfiguration,
    #[error("failed to parse config file: {0}")]
    ParseError(String),
    #[error("failed to validate config: {0}")]
    ValidationError(ValidationErrors),
}

pub type Result<T> = std::result::Result<T, BloklidError>;

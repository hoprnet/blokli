use validator::ValidationErrors;

#[derive(Debug, thiserror::Error)]
pub enum BloklidError {
    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Config(#[from] ConfigError),

    #[error("Unspecified error: {0}")]
    NonSpecific(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("no configuration provided")]
    NoConfiguration,
    #[error("failed to parse config file: {0}")]
    Parse(String),
    #[error("failed to validate config: {0}")]
    Validation(ValidationErrors),
}

pub type Result<T> = std::result::Result<T, BloklidError>;

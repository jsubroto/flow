use std::{fmt, io, path::PathBuf};

use crate::model::Board;

#[derive(Debug)]
pub enum ProviderError {
    NotFound {
        id: String,
    },
    Parse {
        msg: String,
    },
    Io {
        op: String,
        path: PathBuf,
        source: io::Error,
    },
}

impl fmt::Display for ProviderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProviderError::NotFound { id } => write!(f, "not found: {id}"),
            ProviderError::Parse { msg } => write!(f, "parse error: {msg}"),
            ProviderError::Io { op, path, source } => {
                write!(f, "{op} failed for {}: {source}", path.display())
            }
        }
    }
}

impl std::error::Error for ProviderError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ProviderError::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

pub trait Provider {
    fn load_board(&mut self) -> Result<Board, ProviderError>;
    fn move_card(&mut self, card_id: &str, to_col_id: &str) -> Result<(), ProviderError>;
}

pub fn from_env() -> Box<dyn Provider> {
    match std::env::var("FLOW_PROVIDER").ok().as_deref() {
        Some("jira") => Box::new(crate::provider_jira::JiraProvider::from_env()),
        _ => Box::new(crate::provider_local::LocalProvider::from_env()),
    }
}

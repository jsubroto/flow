use std::{
    io,
    path::{Path, PathBuf},
};

use crate::{
    model::Board,
    provider::{Provider, ProviderError},
    store_fs,
};

pub struct LocalProvider {
    root: PathBuf,
}

impl LocalProvider {
    pub fn from_env() -> Self {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

        if let Ok(p) = std::env::var("FLOW_BOARD_PATH") {
            return Self {
                root: PathBuf::from(p),
            };
        }

        if std::env::var("FLOW_PROVIDER").ok().as_deref() == Some("local") {
            if let Ok(p) = std::env::var("FLOW_LOCAL_PATH") {
                return Self {
                    root: PathBuf::from(p),
                };
            }
            if let Ok(home) = std::env::var("HOME") {
                return Self {
                    root: PathBuf::from(home).join(".config/flow/boards/default"),
                };
            }
        }

        Self {
            root: manifest_dir.join("boards/demo"),
        }
    }
}

impl Provider for LocalProvider {
    fn load_board(&mut self) -> Result<Board, ProviderError> {
        store_fs::load_board(&self.root).map_err(|e| map_load_err("load_board", &self.root, e))
    }

    fn move_card(&mut self, card_id: &str, to_col_id: &str) -> Result<(), ProviderError> {
        store_fs::move_card(&self.root, card_id, to_col_id)
            .map_err(|e| map_move_err(card_id, &self.root, e))
    }

    fn create_card(&mut self, to_col_id: &str) -> Result<String, ProviderError> {
        store_fs::create_card(&self.root, to_col_id).map_err(|err| ProviderError::Io {
            op: "create_card".to_string(),
            path: self.root.clone(),
            source: err,
        })
    }

    fn card_path(&self, card_id: &str) -> Result<PathBuf, ProviderError> {
        store_fs::card_path(&self.root, card_id).map_err(|err| match err.kind() {
            io::ErrorKind::NotFound => ProviderError::NotFound {
                id: card_id.to_string(),
            },
            _ => ProviderError::Io {
                op: "card_path".to_string(),
                path: self.root.clone(),
                source: err,
            },
        })
    }
}

fn map_load_err(op: &str, root: &Path, err: io::Error) -> ProviderError {
    match err.kind() {
        io::ErrorKind::InvalidData => ProviderError::Parse {
            msg: err.to_string(),
        },
        _ => ProviderError::Io {
            op: op.to_string(),
            path: root.to_path_buf(),
            source: err,
        },
    }
}

fn map_move_err(card_id: &str, root: &Path, err: io::Error) -> ProviderError {
    match err.kind() {
        io::ErrorKind::NotFound => ProviderError::NotFound {
            id: card_id.to_string(),
        },
        io::ErrorKind::InvalidData => ProviderError::Parse {
            msg: err.to_string(),
        },
        _ => ProviderError::Io {
            op: "move_card".to_string(),
            path: root.to_path_buf(),
            source: err,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::Path,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn tmp_root() -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("flow-provider-test-{n}"))
    }

    fn write(p: &Path, s: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, s).unwrap();
    }

    #[test]
    fn map_load_err_returns_parse_for_invalid_data() {
        let root = PathBuf::from("/tmp/flow-test");
        let err = map_load_err(
            "load_board",
            &root,
            io::Error::new(io::ErrorKind::InvalidData, "bad"),
        );

        assert!(matches!(err, ProviderError::Parse { .. }));
    }

    #[test]
    fn move_card_returns_not_found() {
        let root = tmp_root();
        write(&root.join("board.txt"), "col todo\n");

        let mut provider = LocalProvider { root: root.clone() };
        let err = provider.move_card("X-1", "todo").unwrap_err();

        match err {
            ProviderError::NotFound { id } => assert_eq!(id, "X-1"),
            _ => panic!("expected NotFound error"),
        }

        fs::remove_dir_all(root).unwrap();
    }
}

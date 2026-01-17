use std::{collections::HashMap, io, path::PathBuf};

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use crate::{
    model::{Board, Card, Column},
    provider::{Provider, ProviderError},
};

pub struct JiraProvider {
    client: Client,
    base_url: String,
    email: String,
    api_token: String,
    board_id: Option<String>,
    err: Option<String>,
}

impl JiraProvider {
    pub fn from_env() -> Self {
        let base_url = std::env::var("JIRA_BASE_URL").ok();
        let email = std::env::var("JIRA_EMAIL").ok();
        let api_token = std::env::var("JIRA_API_TOKEN").ok();
        let board_id = std::env::var("JIRA_BOARD_ID").ok();

        Self::from_parts(base_url, email, api_token, board_id)
    }

    fn from_parts(
        base_url: Option<String>,
        email: Option<String>,
        api_token: Option<String>,
        board_id: Option<String>,
    ) -> Self {
        let mut missing = Vec::new();

        let base_url = match base_url {
            Some(v) if !v.trim().is_empty() => v.trim_end_matches('/').to_string(),
            _ => {
                missing.push("JIRA_BASE_URL");
                String::new()
            }
        };

        let email = match email {
            Some(v) if !v.trim().is_empty() => v,
            _ => {
                missing.push("JIRA_EMAIL");
                String::new()
            }
        };

        let api_token = match api_token {
            Some(v) if !v.trim().is_empty() => v,
            _ => {
                missing.push("JIRA_API_TOKEN");
                String::new()
            }
        };

        let board_id = board_id.and_then(|v| {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        });
        if board_id.is_none() {
            missing.push("JIRA_BOARD_ID");
        }

        let err = if missing.is_empty() {
            None
        } else {
            Some(format!("missing {}", missing.join(", ")))
        };

        Self {
            client: Client::new(),
            base_url,
            email,
            api_token,
            board_id,
            err,
        }
    }

    fn map_err(&self, op: &str, err: impl ToString) -> ProviderError {
        ProviderError::Io {
            op: op.to_string(),
            path: PathBuf::from(&self.base_url),
            source: io::Error::new(io::ErrorKind::Other, err.to_string()),
        }
    }

    fn transitions(&self, issue_key: &str) -> Result<Vec<Transition>, ProviderError> {
        let url = format!("{}/rest/api/3/issue/{issue_key}/transitions", self.base_url);
        let resp = self
            .client
            .get(url)
            .basic_auth(&self.email, Some(&self.api_token))
            .send()
            .map_err(|e| self.map_err("jira_transitions", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(self.map_err("jira_transitions", format!("status {status}: {body}")));
        }

        let data: TransitionsResponse = resp
            .json()
            .map_err(|e| self.map_err("jira_transitions", e))?;
        Ok(data.transitions)
    }

    fn board_config(&self, board_id: &str) -> Result<BoardConfigResponse, ProviderError> {
        let url = format!(
            "{}/rest/agile/1.0/board/{board_id}/configuration",
            self.base_url
        );
        let resp = self
            .client
            .get(url)
            .basic_auth(&self.email, Some(&self.api_token))
            .send()
            .map_err(|e| self.map_err("jira_board_config", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(self.map_err("jira_board_config", format!("status {status}: {body}")));
        }

        let body = resp
            .text()
            .map_err(|e| self.map_err("jira_board_config", e))?;
        let data: BoardConfigResponse =
            serde_json::from_str(&body).map_err(|e| self.map_err("jira_board_config", e))?;

        Ok(data)
    }
}

impl Provider for JiraProvider {
    fn load_board(&mut self) -> Result<Board, ProviderError> {
        if let Some(msg) = &self.err {
            return Err(ProviderError::Parse {
                msg: format!("jira misconfigured: {msg}"),
            });
        }

        let board_id = self
            .board_id
            .as_deref()
            .ok_or_else(|| ProviderError::Parse {
                msg: "jira misconfigured: missing JIRA_BOARD_ID".to_string(),
            })?;
        let cfg = self.board_config(board_id)?;
        let config_map = Some(board_config_map(&cfg));
        let mut status_to_column = HashMap::new();
        if let Some(map) = &config_map {
            for (column, status_ids) in &map.column_to_status {
                for id in status_ids {
                    status_to_column.insert(id.clone(), column.clone());
                }
            }
        }
        let jql = format!(
            "filter={} AND assignee = currentUser() AND sprint in openSprints()",
            cfg.filter.id
        );

        let url = format!("{}/rest/api/3/search/jql", self.base_url);
        let resp = self
            .client
            .post(url)
            .basic_auth(&self.email, Some(&self.api_token))
            .json(&SearchRequest {
                jql,
                fields: vec![
                    "summary".to_string(),
                    "description".to_string(),
                    "status".to_string(),
                ],
                max_results: 200,
            })
            .send()
            .map_err(|e| self.map_err("jira_search", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(self.map_err("jira_search", format!("status {status}: {body}")));
        }

        let data: SearchResponse = resp.json().map_err(|e| self.map_err("jira_search", e))?;

        let mut columns = HashMap::<String, Vec<Card>>::new();
        let mut order = Vec::new();

        for issue in data.issues {
            let status_name = issue.fields.status.name;
            let status_id = issue.fields.status.id.clone();

            let column_name = status_to_column
                .get(&status_id)
                .cloned()
                .unwrap_or(status_name);

            if !columns.contains_key(&column_name) {
                columns.insert(column_name.clone(), Vec::new());
                order.push(column_name.clone());
            }

            let desc = match issue.fields.description {
                Some(serde_json::Value::String(s)) => s,
                _ => String::new(),
            };

            columns.get_mut(&column_name).unwrap().push(Card {
                id: issue.key,
                title: issue.fields.summary,
                description: desc,
            });
        }

        let mut col_order = Vec::new();
        if let Some(map) = config_map {
            for name in map.order {
                if !col_order.iter().any(|s: &String| s == &name) {
                    col_order.push(name);
                }
            }
        }

        for name in order {
            if !col_order.iter().any(|s: &String| s == &name) {
                col_order.push(name);
            }
        }

        let mut cols = Vec::new();
        for name in col_order {
            let cards = columns.remove(&name).unwrap_or_default();
            cols.push(Column {
                id: name.clone(),
                title: name,
                cards,
            });
        }

        Ok(Board { columns: cols })
    }

    fn move_card(&mut self, card_id: &str, to_col_id: &str) -> Result<(), ProviderError> {
        if let Some(msg) = &self.err {
            return Err(ProviderError::Parse {
                msg: format!("jira misconfigured: {msg}"),
            });
        }

        let transitions = self.transitions(card_id)?;
        let mut transition_id = None;
        if let Some(board_id) = &self.board_id {
            let cfg = self.board_config(board_id)?;
            let map = board_config_map(&cfg);
            if let Some(status_ids) = map.column_to_status.get(to_col_id) {
                if let Some(t) = pick_transition_for_column(&transitions, to_col_id, status_ids) {
                    transition_id = Some(t.id.clone());
                }
            }
        }
        let transition_id = if let Some(id) = transition_id {
            id
        } else if let Some(t) = transitions.into_iter().find(|t| t.to.name == to_col_id) {
            t.id
        } else {
            return Err(ProviderError::NotFound {
                id: to_col_id.to_string(),
            });
        };

        let url = format!("{}/rest/api/3/issue/{card_id}/transitions", self.base_url);
        let resp = self
            .client
            .post(url)
            .basic_auth(&self.email, Some(&self.api_token))
            .json(&TransitionRequest {
                transition: IdOnly { id: transition_id },
            })
            .send()
            .map_err(|e| self.map_err("jira_transition", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            return Err(self.map_err("jira_transition", format!("status {status}: {body}")));
        }

        Ok(())
    }
}

#[derive(Deserialize)]
struct SearchResponse {
    issues: Vec<Issue>,
}

#[derive(Deserialize)]
struct Issue {
    key: String,
    fields: IssueFields,
}

#[derive(Deserialize)]
struct IssueFields {
    summary: String,
    description: Option<serde_json::Value>,
    status: Status,
}

#[derive(Deserialize)]
struct Status {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct TransitionsResponse {
    transitions: Vec<Transition>,
}

#[derive(Deserialize)]
struct Transition {
    id: String,
    to: Status,
}

#[derive(Serialize)]
struct TransitionRequest {
    transition: IdOnly,
}

#[derive(Deserialize, Serialize)]
struct IdOnly {
    id: String,
}

#[derive(Deserialize)]
struct BoardConfigResponse {
    #[serde(rename = "columnConfig")]
    column_config: ColumnConfig,
    filter: BoardFilter,
}

#[derive(Deserialize)]
struct BoardFilter {
    id: String,
}

#[derive(Deserialize)]
struct ColumnConfig {
    columns: Vec<BoardColumn>,
}

#[derive(Deserialize)]
struct BoardColumn {
    name: String,
    statuses: Vec<IdOnly>,
}

#[derive(serde::Serialize)]
struct SearchRequest {
    jql: String,
    fields: Vec<String>,
    #[serde(rename = "maxResults")]
    max_results: u32,
}

struct BoardConfigMap {
    order: Vec<String>,
    column_to_status: HashMap<String, Vec<String>>,
}

fn board_config_map(cfg: &BoardConfigResponse) -> BoardConfigMap {
    let mut order = Vec::new();
    let mut column_to_status = HashMap::<String, Vec<String>>::new();

    for col in &cfg.column_config.columns {
        if !order.iter().any(|s: &String| s == &col.name) {
            order.push(col.name.clone());
        }
        let entry = column_to_status.entry(col.name.clone()).or_default();
        for status in &col.statuses {
            if !entry.iter().any(|id| id == &status.id) {
                entry.push(status.id.clone());
            }
        }
    }

    BoardConfigMap {
        order,
        column_to_status,
    }
}

fn pick_transition_for_column<'a>(
    transitions: &'a [Transition],
    column_name: &str,
    status_ids: &[String],
) -> Option<&'a Transition> {
    let col = column_name.to_lowercase();
    let prefs: &[&str] = if col.contains("todo") || col.contains("to do") {
        &["open", "backlog"]
    } else if col.contains("progress") {
        &["in progress"]
    } else if col.contains("review") {
        &["in review", "review"]
    } else if col.contains("test") || col.contains("qa") {
        &["in testing", "testing", "qa"]
    } else if col.contains("done") {
        &["done", "resolved", "closed", "verified"]
    } else {
        &[]
    };

    let mut first_match = None;
    for t in transitions {
        if !status_ids.iter().any(|id| id == &t.to.id) {
            continue;
        }
        let name = t.to.name.to_lowercase();
        if !prefs.is_empty() && prefs.iter().any(|p| name.contains(p)) {
            return Some(t);
        }
        if first_match.is_none() {
            first_match = Some(t);
        }
    }

    first_match
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_board_returns_parse_error_when_missing_env() {
        let mut provider = JiraProvider::from_parts(None, None, None, None);
        let err = match provider.load_board() {
            Ok(_) => panic!("expected load_board to fail"),
            Err(e) => e,
        };

        assert!(matches!(err, ProviderError::Parse { .. }));
    }

    #[test]
    fn column_order_from_config_preserves_board_order() {
        let cfg = BoardConfigResponse {
            column_config: ColumnConfig {
                columns: vec![
                    BoardColumn {
                        name: "To Do".to_string(),
                        statuses: vec![IdOnly {
                            id: "1".to_string(),
                        }],
                    },
                    BoardColumn {
                        name: "In Progress".to_string(),
                        statuses: vec![
                            IdOnly {
                                id: "3".to_string(),
                            },
                            IdOnly {
                                id: "4".to_string(),
                            },
                        ],
                    },
                ],
            },
            filter: BoardFilter {
                id: "123".to_string(),
            },
        };

        let map = board_config_map(&cfg);
        assert_eq!(map.order, vec!["To Do", "In Progress"]);
        assert_eq!(map.column_to_status["To Do"], vec!["1"]);
        assert_eq!(map.column_to_status["In Progress"], vec!["3", "4"]);
    }

    #[test]
    fn pick_transition_prefers_open_for_todo() {
        let transitions = vec![
            Transition {
                id: "2".to_string(),
                to: Status {
                    id: "2".to_string(),
                    name: "Selected for Development".to_string(),
                },
            },
            Transition {
                id: "1".to_string(),
                to: Status {
                    id: "1".to_string(),
                    name: "Open".to_string(),
                },
            },
        ];

        let status_ids = vec!["1".to_string(), "2".to_string()];
        let t = pick_transition_for_column(&transitions, "To Do", &status_ids).unwrap();

        assert_eq!(t.to.name, "Open");
    }
}

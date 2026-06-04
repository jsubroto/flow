use std::{cmp::Ordering, collections::HashMap};

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::{
    model::{Board, Card, Column},
    provider::{self, Provider, ProviderError},
};

const ENDPOINT: &str = "https://api.linear.app/graphql";

const BOARD_QUERY: &str = "\
query Board($teamKey: String!) {
  workflowStates(filter: { team: { key: { eq: $teamKey } } }, first: 100) {
    nodes { id name type position }
  }
  issues(
    filter: {
      team: { key: { eq: $teamKey } }
      assignee: { isMe: { eq: true } }
      cycle: { isActive: { eq: true } }
    }
    first: 250
  ) {
    nodes { identifier title description state { id } }
  }
}";

const MOVE_MUTATION: &str = "\
mutation Move($id: String!, $stateId: String!) {
  issueUpdate(id: $id, input: { stateId: $stateId }) { success }
}";

pub struct LinearProvider {
    client: Client,
    api_key: String,
    team_key: String,
    statuses: Option<Vec<String>>,
    err: Option<String>,
}

impl LinearProvider {
    pub fn from_env() -> Self {
        let api_key = std::env::var("LINEAR_API_KEY").ok();
        let team_key = std::env::var("LINEAR_TEAM_KEY").ok();
        let statuses = std::env::var("LINEAR_STATUSES").ok();

        Self::from_parts(api_key, team_key, statuses)
    }

    fn from_parts(
        api_key: Option<String>,
        team_key: Option<String>,
        statuses: Option<String>,
    ) -> Self {
        let mut missing = Vec::new();

        let api_key = provider::required(&mut missing, api_key, "LINEAR_API_KEY");
        let team_key = provider::required(&mut missing, team_key, "LINEAR_TEAM_KEY");
        let statuses = parse_statuses(statuses);

        let err = (!missing.is_empty()).then(|| format!("missing {}", missing.join(", ")));

        Self {
            client: Client::new(),
            api_key,
            team_key,
            statuses,
            err,
        }
    }

    fn map_err(&self, op: &str, err: impl ToString) -> ProviderError {
        provider::io_err(op, ENDPOINT, err)
    }

    fn graphql<T: DeserializeOwned>(
        &self,
        op: &str,
        query: &str,
        variables: serde_json::Value,
    ) -> Result<T, ProviderError> {
        let resp = self
            .client
            .post(ENDPOINT)
            .header(reqwest::header::AUTHORIZATION, self.api_key.as_str())
            .json(&GraphQlRequest { query, variables })
            .send()
            .map_err(|e| self.map_err(op, e))?;
        let resp = provider::ensure_success(resp, op, ENDPOINT)?;

        let envelope: GraphQlResponse<T> = resp.json().map_err(|e| self.map_err(op, e))?;

        if let Some(errors) = envelope.errors
            && !errors.is_empty()
        {
            let msg = errors
                .into_iter()
                .map(|e| e.message)
                .collect::<Vec<_>>()
                .join("; ");
            return Err(self.map_err(op, msg));
        }

        envelope
            .data
            .ok_or_else(|| self.map_err(op, "response missing data"))
    }
}

impl Provider for LinearProvider {
    fn load_board(&mut self) -> Result<Board, ProviderError> {
        provider::config_check(&self.err, "linear")?;

        let data: BoardData = self.graphql(
            "linear_board",
            BOARD_QUERY,
            serde_json::json!({ "teamKey": self.team_key }),
        )?;

        Ok(build_board(
            data.workflow_states.nodes,
            data.issues.nodes,
            self.statuses.as_deref(),
        ))
    }

    fn move_card(&mut self, card_id: &str, to_col_id: &str) -> Result<(), ProviderError> {
        provider::config_check(&self.err, "linear")?;

        let data: MoveData = self.graphql(
            "linear_move",
            MOVE_MUTATION,
            serde_json::json!({ "id": card_id, "stateId": to_col_id }),
        )?;

        if data.issue_update.success {
            Ok(())
        } else {
            Err(self.map_err("linear_move", "issueUpdate returned success=false"))
        }
    }
}

#[derive(Deserialize)]
struct BoardData {
    #[serde(rename = "workflowStates")]
    workflow_states: Connection<StateNode>,
    issues: Connection<IssueNode>,
}

#[derive(Deserialize)]
struct Connection<T> {
    nodes: Vec<T>,
}

#[derive(Deserialize)]
struct StateNode {
    id: String,
    name: String,
    #[serde(rename = "type")]
    state_type: String,
    position: f64,
}

#[derive(Deserialize)]
struct IssueNode {
    identifier: String,
    title: String,
    description: Option<String>,
    state: StateRef,
}

#[derive(Deserialize)]
struct StateRef {
    id: String,
}

#[derive(Deserialize)]
struct MoveData {
    #[serde(rename = "issueUpdate")]
    issue_update: MoveResult,
}

#[derive(Deserialize)]
struct MoveResult {
    success: bool,
}

#[derive(Serialize)]
struct GraphQlRequest<'a> {
    query: &'a str,
    variables: serde_json::Value,
}

#[derive(Deserialize)]
struct GraphQlResponse<T> {
    data: Option<T>,
    errors: Option<Vec<GraphQlError>>,
}

#[derive(Deserialize)]
struct GraphQlError {
    message: String,
}

fn state_type_rank(state_type: &str) -> u8 {
    match state_type {
        "triage" => 0,
        "backlog" => 1,
        "unstarted" => 2,
        "started" => 3,
        "completed" => 4,
        "canceled" => 5,
        _ => 6,
    }
}

fn parse_statuses(raw: Option<String>) -> Option<Vec<String>> {
    let states: Vec<String> = raw?
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();

    (!states.is_empty()).then_some(states)
}

fn order_states(states: Vec<StateNode>, preferred: Option<&[String]>) -> Vec<StateNode> {
    let Some(preferred) = preferred else {
        let mut states = states;
        states.sort_by(|a, b| {
            state_type_rank(&a.state_type)
                .cmp(&state_type_rank(&b.state_type))
                .then(
                    a.position
                        .partial_cmp(&b.position)
                        .unwrap_or(Ordering::Equal),
                )
        });
        return states;
    };

    let mut by_name: HashMap<String, StateNode> = states
        .into_iter()
        .map(|s| (s.name.to_lowercase(), s))
        .collect();

    preferred
        .iter()
        .filter_map(|name| by_name.remove(&name.to_lowercase()))
        .collect()
}

fn build_board(
    states: Vec<StateNode>,
    issues: Vec<IssueNode>,
    preferred: Option<&[String]>,
) -> Board {
    let states = order_states(states, preferred);

    let mut index = HashMap::new();
    let mut columns = Vec::with_capacity(states.len());
    for state in states {
        index.insert(state.id.clone(), columns.len());
        columns.push(Column {
            id: state.id,
            title: state.name,
            cards: Vec::new(),
        });
    }

    for issue in issues {
        let Some(&col) = index.get(&issue.state.id) else {
            continue;
        };
        columns[col].cards.push(Card {
            id: issue.identifier,
            title: issue.title,
            description: issue.description.unwrap_or_default(),
        });
    }

    Board { columns }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_board_returns_parse_error_when_missing_env() {
        let mut provider = LinearProvider::from_parts(None, None, None);
        let err = match provider.load_board() {
            Ok(_) => panic!("expected load_board to fail"),
            Err(e) => e,
        };

        assert!(matches!(err, ProviderError::Parse { .. }));
    }

    #[test]
    fn build_board_orders_states_by_type_then_position() {
        let states = vec![
            StateNode {
                id: "done".to_string(),
                name: "Done".to_string(),
                state_type: "completed".to_string(),
                position: 0.0,
            },
            StateNode {
                id: "todo".to_string(),
                name: "Todo".to_string(),
                state_type: "unstarted".to_string(),
                position: 1.0,
            },
            StateNode {
                id: "backlog".to_string(),
                name: "Backlog".to_string(),
                state_type: "backlog".to_string(),
                position: 0.0,
            },
            StateNode {
                id: "doing".to_string(),
                name: "In Progress".to_string(),
                state_type: "started".to_string(),
                position: 0.0,
            },
        ];

        let board = build_board(states, vec![], None);
        let order: Vec<&str> = board.columns.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(order, vec!["backlog", "todo", "doing", "done"]);
    }

    #[test]
    fn build_board_filters_and_orders_by_preferred_statuses() {
        let states = vec![
            StateNode {
                id: "todo".to_string(),
                name: "Todo".to_string(),
                state_type: "unstarted".to_string(),
                position: 0.0,
            },
            StateNode {
                id: "backlog".to_string(),
                name: "Backlog".to_string(),
                state_type: "backlog".to_string(),
                position: 0.0,
            },
            StateNode {
                id: "done".to_string(),
                name: "Done".to_string(),
                state_type: "completed".to_string(),
                position: 0.0,
            },
        ];
        let preferred = vec!["done".to_string(), "todo".to_string()];

        let board = build_board(states, vec![], Some(&preferred));
        let order: Vec<&str> = board.columns.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(order, vec!["done", "todo"]);
    }

    #[test]
    fn parse_statuses_trims_and_drops_empties() {
        assert_eq!(parse_statuses(None), None);
        assert_eq!(parse_statuses(Some("  ,  ".to_string())), None);
        assert_eq!(
            parse_statuses(Some("Todo, In Progress ,,In Review".to_string())),
            Some(vec![
                "Todo".to_string(),
                "In Progress".to_string(),
                "In Review".to_string(),
            ])
        );
    }

    #[test]
    fn build_board_places_issues_in_their_state_column() {
        let states = vec![
            StateNode {
                id: "todo".to_string(),
                name: "Todo".to_string(),
                state_type: "unstarted".to_string(),
                position: 0.0,
            },
            StateNode {
                id: "doing".to_string(),
                name: "In Progress".to_string(),
                state_type: "started".to_string(),
                position: 1.0,
            },
        ];
        let issues = vec![
            IssueNode {
                identifier: "ENG-1".to_string(),
                title: "First".to_string(),
                description: Some("with a https://example.com link".to_string()),
                state: StateRef {
                    id: "doing".to_string(),
                },
            },
            IssueNode {
                identifier: "ENG-2".to_string(),
                title: "Second".to_string(),
                description: None,
                state: StateRef {
                    id: "todo".to_string(),
                },
            },
        ];

        let board = build_board(states, issues, None);
        assert_eq!(board.columns[0].cards.len(), 1);
        assert_eq!(board.columns[0].cards[0].id, "ENG-2");
        assert_eq!(board.columns[0].cards[0].description, "");
        assert_eq!(board.columns[1].cards.len(), 1);
        assert_eq!(board.columns[1].cards[0].id, "ENG-1");
        assert_eq!(
            board.columns[1].cards[0].description,
            "with a https://example.com link"
        );
    }
}

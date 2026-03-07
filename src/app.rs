use crate::model::Board;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Quit,
    CloseOrQuit,
    FocusLeft,
    FocusRight,
    SelectUp,
    SelectDown,
    MoveLeft,
    MoveRight,
    ToggleDetail,
    Refresh,
    Delete,
}

pub struct App {
    pub board: Board,
    pub col: usize,
    pub row: usize,
    pub detail_open: bool,
    pub banner: Option<String>,
}

impl App {
    pub fn new(board: Board) -> Self {
        Self {
            board,
            col: 0,
            row: 0,
            detail_open: false,
            banner: None,
        }
    }

    fn reset_cursor(&mut self) {
        (self.col, self.row) = (0, 0);
    }

    fn clamp_index(idx: usize, delta: isize, max: usize) -> usize {
        if delta < 0 {
            idx.saturating_sub((-delta) as usize)
        } else {
            (idx + delta as usize).min(max)
        }
    }

    fn col_len(&self) -> usize {
        self.board
            .columns
            .get(self.col)
            .map(|c| c.cards.len())
            .unwrap_or(0)
    }

    fn clamp_row(&mut self) {
        let len = self.col_len();
        self.row = if len == 0 { 0 } else { self.row.min(len - 1) };
    }

    fn next_non_empty_col(&self, step: isize) -> Option<usize> {
        if step == 0 {
            return None;
        }

        let mut idx = self.col as isize;
        let len = self.board.columns.len() as isize;

        loop {
            idx += step;
            if idx < 0 || idx >= len {
                return None;
            }
            let col = &self.board.columns[idx as usize];
            if !col.cards.is_empty() {
                return Some(idx as usize);
            }
        }
    }

    fn dst_col(&self, dir: isize) -> Option<usize> {
        let dst = self.col as isize + dir;
        if dst < 0 {
            return None;
        }
        let dst = dst as usize;
        (dst < self.board.columns.len()).then_some(dst)
    }

    pub fn clamp(&mut self) {
        if self.board.columns.is_empty() {
            self.reset_cursor();
            return;
        }

        self.col = self.col.min(self.board.columns.len() - 1);
        self.clamp_row();
    }

    pub fn focus(&mut self, dir: isize) {
        if self.board.columns.is_empty() {
            self.reset_cursor();
            return;
        }

        let dir = dir.signum();
        if let Some(next) = self.next_non_empty_col(dir) {
            self.col = next;
            self.clamp_row();
        }
    }

    pub fn select(&mut self, delta: isize) {
        let len = self.col_len();
        if len == 0 {
            self.row = 0;
            return;
        }

        self.row = Self::clamp_index(self.row, delta, len - 1);
    }

    pub fn apply(&mut self, a: Action) -> bool {
        match a {
            Action::Quit => return true,
            Action::CloseOrQuit => {
                if self.detail_open {
                    self.detail_open = false;
                } else {
                    return true;
                }
            }
            Action::FocusLeft => self.focus(-1),
            Action::FocusRight => self.focus(1),
            Action::SelectUp => self.select(-1),
            Action::SelectDown => self.select(1),
            Action::ToggleDetail => self.detail_open = !self.detail_open,
            Action::Refresh | Action::MoveLeft | Action::MoveRight => {}
            Action::Delete => { self.delete_current_card(); },
        }
        false
    }

    pub fn focus_first_non_empty(&mut self) {
        (self.col, self.row) = (first_non_empty_column(&self.board).unwrap_or(0), 0);
    }

    pub fn optimistic_move(&mut self, dir: isize) -> Option<(String, String)> {
        if self.board.columns.is_empty() {
            return None;
        }

        self.clamp();

        let dst = self.dst_col(dir)?;
        let src = self.col;
        if self.board.columns[src].cards.is_empty() {
            return None;
        }

        let card = self.board.columns[src].cards.remove(self.row);
        let card_id = card.id.clone();
        let to_col_id = self.board.columns[dst].id.clone();

        self.board.columns[dst].cards.push(card);

        self.col = dst;
        self.row = self.board.columns[dst].cards.len() - 1;

        Some((card_id, to_col_id))
    }

    pub fn delete_current_card(&mut self) -> Option<String> {
        if self.board.columns.is_empty() {
            return None;
        }

        self.clamp();

        if self.board.columns[self.col].cards.is_empty() {
            return None;
        }

        let card = self.board.columns[self.col].cards.remove(self.row);
        self.clamp_row();

        Some(card.id)
    }
}

fn first_non_empty_column(board: &Board) -> Option<usize> {
    for (i, col) in board.columns.iter().enumerate() {
        if !col.cards.is_empty() {
            return Some(i);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Board, Card, Column};

    fn board_two_cols() -> Board {
        Board {
            columns: vec![
                Column {
                    id: "a".into(),
                    title: "A".into(),
                    cards: vec![
                        Card {
                            id: "1".into(),
                            title: "t1".into(),
                            description: "d".into(),
                        },
                        Card {
                            id: "2".into(),
                            title: "t2".into(),
                            description: "d".into(),
                        },
                    ],
                },
                Column {
                    id: "b".into(),
                    title: "B".into(),
                    cards: vec![],
                },
            ],
        }
    }

    #[test]
    fn clamp_bounds_indices() {
        let mut app = App::new(board_two_cols());
        (app.col, app.row) = (9, 9);
        app.clamp();

        assert_eq!((app.col, app.row), (1, 0));
    }

    #[test]
    fn focus_skips_empty_columns() {
        let mut app = App::new(board_two_cols());

        app.focus(-1);
        assert_eq!(app.col, 0);

        app.focus(10);
        assert_eq!(app.col, 0);

        app.board.columns[1].cards.push(Card {
            id: "3".into(),
            title: "t3".into(),
            description: "d".into(),
        });
        app.focus(1);
        assert_eq!(app.col, 1);
    }

    #[test]
    fn select_clamps_rows_and_handles_empty_column() {
        let mut app = App::new(board_two_cols());

        app.select(10);
        assert_eq!(app.row, 1);

        app.select(-10);
        assert_eq!(app.row, 0);

        (app.col, app.row) = (1, 9);
        app.select(1);
        assert_eq!(app.row, 0);
    }

    #[test]
    fn move_right_moves_card_and_updates_focus_to_new_card() {
        let mut app = App::new(board_two_cols());

        let (id, dst) = app.optimistic_move(1).unwrap();

        assert_eq!(id, "1");
        assert_eq!(dst, "b");
        assert_eq!((app.col, app.row), (1, 0));
        assert_eq!(app.board.columns[1].cards.len(), 1);
        assert_eq!(app.board.columns[1].cards[0].id, "1");
        assert_eq!(app.board.columns[0].cards.len(), 1);
    }

    #[test]
    fn move_out_of_bounds_is_none() {
        let mut app = App::new(board_two_cols());

        assert!(app.optimistic_move(-1).is_none());
        assert!(app.optimistic_move(10).is_none());
    }

    #[test]
    fn move_with_empty_board_is_none_and_does_not_panic() {
        let mut app = App::new(Board { columns: vec![] });

        assert!(app.optimistic_move(1).is_none());
        assert_eq!((app.col, app.row), (0, 0));
    }

    #[test]
    fn move_from_empty_column_is_none() {
        let mut app = App::new(board_two_cols());
        (app.col, app.row) = (1, 0);

        assert!(app.optimistic_move(-1).is_none());
    }

    #[test]
    fn focus_first_non_empty_picks_first_column_with_cards() {
        let mut app = App::new(board_two_cols());

        app.board.columns[0].cards.clear();
        app.board.columns[1].cards.push(Card {
            id: "2".to_string(),
            title: "t2".to_string(),
            description: "d".to_string(),
        });
        app.focus_first_non_empty();

        assert_eq!((app.col, app.row), (1, 0));
    }

    #[test]
    fn close_or_quit_closes_detail_first_then_quits() {
        let mut app = App::new(board_two_cols());

        app.detail_open = true;
        assert!(!app.apply(Action::CloseOrQuit));
        assert!(!app.detail_open);

        assert!(app.apply(Action::CloseOrQuit));
    }
}

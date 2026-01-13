use std::{io, path::PathBuf, time::Duration};

use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

mod app;
mod model;
mod store_fs;

use app::{Action, App};

fn help_text() -> &'static str {
    "h/l or ←/→ focus  j/k or ↑/↓ select  H/L or Shift+←/→ move  Enter detail  r refresh  Esc close/quit  q quit"
}

fn action_from_key(event: KeyEvent) -> Option<Action> {
    if event.modifiers.contains(KeyModifiers::SHIFT) {
        match event.code {
            KeyCode::Left => return Some(Action::MoveLeft),
            KeyCode::Right => return Some(Action::MoveRight),
            _ => {}
        }
    }

    Some(match event.code {
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Esc => Action::CloseOrQuit,

        KeyCode::Char('h') | KeyCode::Left => Action::FocusLeft,
        KeyCode::Char('l') | KeyCode::Right => Action::FocusRight,

        KeyCode::Char('j') | KeyCode::Down => Action::SelectDown,
        KeyCode::Char('k') | KeyCode::Up => Action::SelectUp,

        KeyCode::Char('H') => Action::MoveLeft,
        KeyCode::Char('L') => Action::MoveRight,

        KeyCode::Enter => Action::ToggleDetail,
        KeyCode::Char('r') => Action::Refresh,

        _ => return None,
    })
}

fn board_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    if std::env::var("FLOW_PROVIDER").ok().as_deref() == Some("local") {
        if let Ok(p) = std::env::var("FLOW_LOCAL_PATH") {
            return PathBuf::from(p);
        }
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(".config/flow/boards/default");
        }
    }

    if let Ok(p) = std::env::var("FLOW_BOARD_PATH") {
        return PathBuf::from(p);
    }

    manifest_dir.join("boards/demo")
}

fn main() -> io::Result<()> {
    let root = board_root();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run(&mut terminal, root);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    res
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, root: PathBuf) -> io::Result<()> {
    let board = match store_fs::load_board(&root) {
        Ok(b) => b,
        Err(e) => {
            let mut app = App::new(model::Board { columns: vec![] });
            app.banner = Some(format!("Load failed: {e}"));
            loop {
                terminal.draw(|f| render(f, &app))?;
                if event::poll(Duration::from_millis(50))? {
                    if let Event::Key(k) = event::read()? {
                        if k.kind == KeyEventKind::Press
                            && matches!(k.code, KeyCode::Char('q') | KeyCode::Esc)
                        {
                            break;
                        }
                    }
                }
            }
            return Ok(());
        }
    };

    let mut app = App::new(board);

    loop {
        terminal.draw(|f| render(f, &app))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(k) = event::read()? {
                if k.kind == KeyEventKind::Press {
                    if let Some(a) = action_from_key(k) {
                        match a {
                            Action::MoveLeft => {
                                if let Some((card, dst)) = app.optimistic_move(-1) {
                                    if let Err(e) = store_fs::move_card(&root, &card, &dst) {
                                        app.banner = Some(format!("Move failed: {e}"));
                                    }
                                }
                            }
                            Action::MoveRight => {
                                if let Some((card, dst)) = app.optimistic_move(1) {
                                    if let Err(e) = store_fs::move_card(&root, &card, &dst) {
                                        app.banner = Some(format!("Move failed: {e}"));
                                    }
                                }
                            }
                            Action::Refresh => match store_fs::load_board(&root) {
                                Ok(b) => {
                                    app.board = b;
                                    app.col = 0;
                                    app.row = 0;
                                    app.banner = None;
                                }
                                Err(e) => app.banner = Some(format!("Refresh failed: {e}")),
                            },
                            _ => {
                                if app.apply(a) {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn render(f: &mut Frame, app: &App) {
    let chunks = if app.banner.is_some() {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(1),
                Constraint::Length(2),
            ])
            .split(f.area())
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(1), Constraint::Length(2)])
            .split(f.area())
    };

    let (banner_area, main, help) = if app.banner.is_some() {
        (Some(chunks[0]), chunks[1], chunks[2])
    } else {
        (None, chunks[0], chunks[1])
    };

    if let (Some(a), Some(text)) = (banner_area, app.banner.as_deref()) {
        f.render_widget(
            Paragraph::new(Span::styled(text, Style::default().fg(Color::Yellow))),
            a,
        );
    }

    if app.board.columns.is_empty() {
        f.render_widget(
            Paragraph::new("No columns found. Check board.txt.")
                .block(Block::default().borders(Borders::ALL)),
            main,
        );
    } else {
        let rects = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(vec![
                Constraint::Ratio(1, app.board.columns.len() as u32);
                app.board.columns.len()
            ])
            .split(main);

        for (i, r) in rects.iter().enumerate() {
            draw_col(f, app, i, *r);
        }
    }

    f.render_widget(
        Paragraph::new(help_text()).block(Block::default().borders(Borders::TOP)),
        help,
    );

    if app.detail {
        let Some(col) = app.board.columns.get(app.col) else {
            return;
        };
        let Some(card) = col.cards.get(app.row) else {
            return;
        };

        let area = centered(70, 45, f.area());
        f.render_widget(Clear, area);

        let mut lines = Vec::new();
        lines.push(Line::from(Span::styled(
            &card.id,
            Style::default().add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(card.title.clone()));
        lines.push(Line::from(""));

        if card.description.trim().is_empty() {
            lines.push(Line::from(Span::styled(
                "No description",
                Style::default().fg(Color::DarkGray),
            )));
        } else {
            for l in card.description.lines() {
                lines.push(Line::from(l.to_string()));
            }
        }

        f.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }).block(
                Block::default()
                    .title("Detail")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray)),
            ),
            area,
        );
    }
}

fn draw_col(f: &mut Frame, app: &App, idx: usize, rect: Rect) {
    let col = &app.board.columns[idx];
    let focused = idx == app.col;

    let border = if focused { Color::Cyan } else { Color::Gray };

    let items: Vec<ListItem> = col
        .cards
        .iter()
        .map(|c| {
            ListItem::new(Line::from(vec![
                Span::styled(&c.id, Style::default().add_modifier(Modifier::BOLD)),
                Span::raw(" "),
                Span::raw(c.title.clone()),
            ]))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(format!("{} ({})", col.title, col.cards.len()))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border)),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    let mut state = ListState::default();
    if focused && !col.cards.is_empty() {
        state.select(Some(app.row.min(col.cards.len() - 1)));
    }

    f.render_stateful_widget(list, rect, &mut state);
}

fn centered(px: u16, py: u16, r: Rect) -> Rect {
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - py) / 2),
            Constraint::Percentage(py),
            Constraint::Percentage((100 - py) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - px) / 2),
            Constraint::Percentage(px),
            Constraint::Percentage((100 - px) / 2),
        ])
        .split(v[1])[1]
}

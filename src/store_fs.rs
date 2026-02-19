use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::model::{Board, Card, Column};

pub fn load_board(root: &Path) -> io::Result<Board> {
    let txt = fs::read_to_string(root.join("board.txt"))?;
    let mut cols = Vec::new();

    for line in txt.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let Some(rest) = line.strip_prefix("col ") else {
            continue;
        };
        let (id, title) = parse_col(rest)?;
        let cards = load_cards(root, &id)?;
        cols.push(Column { id, title, cards });
    }

    Ok(Board { columns: cols })
}

fn parse_col(rest: &str) -> io::Result<(String, String)> {
    let mut it = rest.splitn(2, ' ');
    let Some(id) = it.next() else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "missing column id",
        ));
    };
    let title = it.next().unwrap_or(id).trim().trim_matches('"');
    Ok((id.to_string(), title.to_string()))
}

fn load_cards(root: &Path, col_id: &str) -> io::Result<Vec<Card>> {
    let dir = root.join("cols").join(col_id);
    let order_path = dir.join("order.txt");
    if !order_path.exists() {
        return Ok(vec![]);
    }

    let order = fs::read_to_string(order_path)?;
    let mut cards = Vec::new();

    for id in order.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let raw = fs::read_to_string(dir.join(format!("{id}.md")))?;
        let (title, desc) = parse_md(&raw, id);
        cards.push(Card {
            id: id.to_string(),
            title,
            description: desc,
        });
    }

    Ok(cards)
}

fn parse_md(raw: &str, fallback: &str) -> (String, String) {
    let mut lines = raw.lines();
    let first = lines.next().unwrap_or("");
    let title = first.strip_prefix("# ").unwrap_or(first).trim();
    let title = if title.is_empty() { fallback } else { title };

    let rest = raw[first.len()..].trim().to_string();
    (title.to_string(), rest)
}

pub fn move_card(root: &Path, card_id: &str, to_col_id: &str) -> io::Result<()> {
    let col_ids = list_columns(root)?;
    let src = find_card_column(root, &col_ids, card_id)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "card not found"))?;

    if src == to_col_id {
        return Ok(());
    }

    let src_dir = root.join("cols").join(&src);
    let dst_dir = root.join("cols").join(to_col_id);
    fs::create_dir_all(&dst_dir)?;

    fs::rename(
        src_dir.join(format!("{card_id}.md")),
        dst_dir.join(format!("{card_id}.md")),
    )?;

    order_remove(&src_dir.join("order.txt"), card_id)?;
    order_append(&dst_dir.join("order.txt"), card_id)?;

    Ok(())
}

pub fn create_card(root: &Path, to_col_id: &str) -> io::Result<String> {
    let id = format!("CARD-{}", now_millis());
    let dir = root.join("cols").join(to_col_id);
    fs::create_dir_all(&dir)?;
    fs::write(dir.join(format!("{id}.md")), "# New card\n\n")?;
    order_append(&dir.join("order.txt"), &id)?;
    Ok(id)
}

pub fn card_path(root: &Path, card_id: &str) -> io::Result<PathBuf> {
    let col_ids = list_columns(root)?;
    let src = find_card_column(root, &col_ids, card_id)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "card not found"))?;
    Ok(root.join("cols").join(src).join(format!("{card_id}.md")))
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

fn list_columns(root: &Path) -> io::Result<Vec<String>> {
    let txt = fs::read_to_string(root.join("board.txt"))?;
    Ok(txt
        .lines()
        .filter_map(|l| l.trim().strip_prefix("col "))
        .filter_map(|rest| rest.split_whitespace().next())
        .map(|s| s.to_string())
        .collect())
}

fn find_card_column(root: &Path, cols: &[String], card_id: &str) -> io::Result<Option<String>> {
    for c in cols {
        if root
            .join("cols")
            .join(c)
            .join(format!("{card_id}.md"))
            .exists()
        {
            return Ok(Some(c.clone()));
        }
    }
    Ok(None)
}

fn order_remove(path: &Path, id: &str) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let cur = fs::read_to_string(path)?;
    let mut out = Vec::new();
    for l in cur.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if l != id {
            out.push(l);
        }
    }
    let mut s = out.join("\n");
    s.push('\n');
    fs::write(path, s)
}

fn order_append(path: &Path, id: &str) -> io::Result<()> {
    let mut lines = if path.exists() {
        fs::read_to_string(path)?
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    } else {
        vec![]
    };

    if !lines.iter().any(|x| x == id) {
        lines.push(id.to_string());
    }

    let mut s = lines.join("\n");
    s.push('\n');
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(path, s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn tmp_root() -> PathBuf {
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("flow-test-{n}"))
    }

    fn write(p: &Path, s: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, s).unwrap();
    }

    #[test]
    fn load_and_move_persists() {
        let root = tmp_root();
        fs::create_dir_all(root.join("cols")).unwrap();

        write(
            &root.join("board.txt"),
            "col todo \"TO DO\"\ncol done \"DONE\"\n",
        );
        write(&root.join("cols/todo/order.txt"), "A-1\n");
        write(&root.join("cols/todo/A-1.md"), "# Title\n\nBody\n");
        write(&root.join("cols/done/order.txt"), "");

        let b = load_board(&root).unwrap();
        assert_eq!(b.columns[0].cards.len(), 1);

        move_card(&root, "A-1", "done").unwrap();

        let b2 = load_board(&root).unwrap();
        assert_eq!(b2.columns[1].cards.len(), 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn create_card_persists_file_and_order() {
        let root = tmp_root();
        write(&root.join("board.txt"), "col todo\n");

        let id = create_card(&root, "todo").unwrap();
        assert!(
            root.join("cols")
                .join("todo")
                .join(format!("{id}.md"))
                .exists()
        );

        let order = fs::read_to_string(root.join("cols/todo/order.txt")).unwrap();
        assert!(order.lines().any(|l| l == id));

        fs::remove_dir_all(root).unwrap();
    }
}

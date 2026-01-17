# flow

A keyboard-first Kanban board in your terminal.

Move work between states with a single keystroke and peek at issue descriptions without opening a browser.

![Demo](./demo.gif)

## Why
Opening a browser just to move an issue is slow and breaks focus.  
`flow` keeps the common actions fast, local, and keyboard-driven.

This project is intentionally minimal and opinionated.

## Features
- Keyboard-first Kanban board
- Columns and cards loaded from disk (no hardcoded data)
- One-keystroke transitions (`H` / `L`)
- Toggle issue description (`Enter`)
- `hjkl` **and** arrow-key navigation
- Clean, terminal-native visuals
- Immediate persistence on move (local mode)

## Demo / Local mode
`flow` runs in **demo mode by default**.

Demo data is loaded from files on disk and can be edited directly.
This makes the demo representative of real usage, not a hardcoded example.

Default demo board location:

```
boards/demo/
```

To use a persistent local board, point `FLOW_BOARD_PATH` at the board directory:

```bash
FLOW_BOARD_PATH=/path/to/board cargo run
```

Local boards default to:

```
~/.config/flow/boards/default
```

If you want to use the default local board path, set:

```bash
FLOW_PROVIDER=local cargo run
```

## Jira mode
To load issues from Jira, set:

```bash
FLOW_PROVIDER=jira
JIRA_BASE_URL=https://your-site.atlassian.net
JIRA_EMAIL=you@example.com
JIRA_API_TOKEN=your_token
```

Set board ID to load column order from Jira and infer the board's filter:

```bash
JIRA_BOARD_ID=123
```

Flow will only show issues assigned to the current user in open sprints.


## Board format
Boards are plain files:

- `board.txt` — column definitions and order
- `cols/<column>/order.txt` — card ordering per column
- `cols/<column>/<ID>.md` — card content (Markdown)

Example:

```
boards/demo/
  board.txt
  cols/
    todo/
      order.txt
      FLOW-1.md
      FLOW-2.md
```

This format is:
- human-editable
- diff-friendly
- resilient to partial edits

## Keybindings
- `h` / `l` **or** `←` / `→` — focus column
- `j` / `k` **or** `↑` / `↓` — select card
- `H` / `L` — move card left / right
- `Enter` — toggle description
- `r` — reload board from disk
- `Esc` — close description / quit
- `q` — quit

## Run

```bash
cargo run
```

## Status
Early, but usable.

The focus is on a solid interaction model, simple persistence, and a testable core.
Expect breaking changes as features are added.

## License
MIT

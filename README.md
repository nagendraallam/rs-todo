# Terminal Todo

> A fast, offline-first terminal TODO app with a TUI — built in Rust. Optional TickTick cloud sync.

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/built%20with-Rust-orange.svg)](https://www.rust-lang.org)
[![Release](https://img.shields.io/github/v/release/nagendraallam/rs-todo)](https://github.com/nagendraallam/rs-todo/releases)
[![Crates.io](https://img.shields.io/crates/v/rs-todo.svg)](https://crates.io/crates/rs-todo)

![Terminal Todo TUI](./image.png)

---

If you live in the terminal and don't want to context-switch into another app just to track tasks, this tool is for you. Add tasks in one command, browse and edit them in a clean TUI, and optionally sync everything to TickTick cloud.

---

## Features

- **Instant task entry** — `todo Buy groceries` from anywhere in your shell
- **Full TUI** — navigate, read, edit, complete, and delete tasks without leaving the terminal
- **Offline-first** — all data lives locally; no account required
- **TickTick cloud sync** — connect once and your tasks sync across devices
- **Project switching** — switch between TickTick projects from inside the TUI
- **Clean rendering** — handles unicode/emoji widths for crisp TUI borders on any font

---

## Install

### From source (requires Rust)

```bash
cargo install --git https://github.com/nagendraallam/rs-todo
```

Or clone and install locally:

```bash
git clone https://github.com/nagendraallam/rs-todo
cd rs-todo
cargo install --path .
```

The `todo` binary lands in `~/.cargo/bin` — add it to your `PATH` if it isn't already.

> Don't have Rust? Install it first:
> ```bash
> curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
> ```

---

## Usage

```bash
# Add a task instantly
todo Buy groceries
todo "Review production logs"

# Open the TUI
todo

# TickTick cloud sync
todo ticktick connect          # OAuth connect (one-time)
todo ticktick projects         # list your TickTick projects
todo ticktick use <project>    # set default project
todo ticktick sync             # push queued changes + pull latest
todo sync                      # alias
todo refresh                   # alias
```

Inside the TUI, press `h` to toggle the full key-binding reference.

---

## TUI Key Bindings

| Key | Action |
|-----|--------|
| `↑` / `↓` or `j` / `k` | Navigate tasks |
| `Enter` | Open task detail |
| `a` | Add new task |
| `e` | Edit selected task |
| `d` | Toggle done/undone |
| `x` | Delete task |
| `p` | Switch project |
| `r` | Refresh / sync |
| `q` | Quit |
| `h` | Toggle help |

---

## TickTick Cloud Sync

Connect once:

```bash
todo ticktick connect
```

After that, tasks are synced to TickTick and visible across all your devices and apps. Local changes made offline are queued and pushed on the next sync.

---

## Tip: Keep it always open in tmux

If you use `tmux`, dedicate a pane or session to `todo` so your task list is always one keybind away.

```bash
tmux new-session -d -s todo -c ~ 'todo'
```

---

## License

[MIT](LICENSE)

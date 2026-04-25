# todo

a tiny cli todo app written in rust. no electron, no web server, no database — just a single binary and a file in your home directory.

i built this because every other todo app i tried was either too heavy, required an account, or had some "pro plan" gate in front of the features i actually wanted. this one does exactly what i need and nothing else.

---

## what it looks like

```
╭────────────────────────────────────────────────────╮
│ Tasks                                  Tab → Done  │
├────────────────────────────────────────────────────┤
│  [ ]  1.  Buy groceries                        ·  │
│  [ ]  2.  Read the Rust book                      │
│  [ ]  3.  Write some tests                        │
│                                                    │
╰────────────────────────────────────────────────────╯
│  ↑↓/jk: move  Enter/1-9: open  d: done  q: quit  │
╰────────────────────────────────────────────────────╯
```

---

## install

you need rust. if you don't have it:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

then clone and install:

```bash
git clone <your-repo-url>
cd learn-rust
cargo install --path .
```

that's it. the `todo` binary ends up in `~/.cargo/bin/` which rustup already adds to your PATH.

to update later just pull and run `cargo install --path .` again.

---

## usage

**add a task** — just type it, no flags, no quotes needed:

```bash
todo Buy groceries
todo Fix that bug in the auth service
todo Call mom back
```

**open the viewer:**

```bash
todo
```

---

## inside the viewer

### list view

| key | what it does |
|---|---|
| `↑` / `↓` or `j` / `k` | move up and down |
| `Enter` | open a task |
| `1` – `9` | jump straight to that task number |
| `d` | mark done (or undo it) |
| `x` | delete |
| `Tab` | switch between All and Done |
| `q` / `Esc` | quit |

### task detail view

press `Enter` or a number from the list to get here. you'll see the title, whether it's done, and the description.

| key | what it does |
|---|---|
| `e` | edit the description |
| `d` | toggle done / pending |
| `x` | delete the task |
| `Esc` / `q` | back to list |

### editing a description

a little inline text editor opens up. nothing fancy, just type. supports `←` `→`, `Home`, `End`, `Backspace`, `Delete`. press `Enter` to save, `Esc` to cancel.

the `·` dot next to a task in the list means it has a description — handy if you've added notes to something.

---

## where does it store stuff

everything goes to `~/.todo_store`. it's not plain JSON — the file is XOR-encrypted so it's not immediately readable if someone opens it. nothing fancy, but it's not sitting there in plaintext either.

don't delete that file unless you want to wipe your tasks.

---

## works on

- macOS
- Linux
- Windows (the terminal control uses crossterm, no ncurses)

---

## dependencies

kept intentionally small:

- [`crossterm`](https://github.com/crossterm-rs/crossterm) — cross-platform terminal control
- [`serde`](https://serde.rs) + [`serde_json`](https://github.com/serde-rs/json) — serialisation
- [`dirs`](https://github.com/dirs-dev/dirs-rs) — finds the home directory on any OS

no async runtime, no database, no config files.

---

## building from source (without installing)

```bash
cargo build --release
./target/release/todo Buy something
./target/release/todo
```

---

## a few things i want to add eventually

- [ ] due dates
- [ ] priorities
- [ ] search / filter by keyword
- [ ] maybe export to markdown

no promises on timeline though.

# Terminal Todo v0.1.0

A terminal TODO application built on (T)RUST.

This release adds TickTick cloud sync, project switching from the TUI, better offline behavior, and a cleaner TUI status experience.

## Highlights

- TickTick OAuth connect flow from terminal (`todo ticktick connect`)
- Stable redirect URI support for OAuth callback
- Project listing and default project selection
- In-TUI project switcher (`p`) with immediate project task loading
- Refresh flow (`r`) to sync and pull latest tasks
- Offline-first queueing: local actions are stored and synced when available
- Better footer/status behavior (no task-row obstruction)
- Improved unicode/emoji width handling for clean TUI borders
- Refreshed README with updated usage and screenshot

## Commands

```bash
todo "Buy groceries"          # quick add
todo                          # open TUI
todo ticktick connect         # connect TickTick
todo ticktick projects        # list projects
todo ticktick use <project>   # switch default project
todo ticktick sync            # sync queue + pull default project
todo sync                     # alias
todo refresh                  # alias
```

## Notes

- Local data remains available offline.
- When connected, changes are synced to TickTick cloud.
- Tokens and local data are stored on disk with lightweight protection.

## Install

```bash
cargo install --path .
```

use std::io::{self, Write};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers},
    execute, queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{self, ClearType},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{
    app_state::{AppState, ProjectRef},
    commands, storage,
    todo::Todo,
};

// ── Colours ───────────────────────────────────────────────────────────────────

const C_BORDER: Color = Color::DarkCyan;
const C_TITLE: Color = Color::Cyan;
const C_DIM: Color = Color::DarkGrey;
const C_BODY: Color = Color::White;
const C_DONE: Color = Color::Green;
const C_ACCENT: Color = Color::DarkCyan;
const C_SEL_BG: Color = Color::DarkBlue;

// ── State ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    List,
    Detail,
    Edit,
    Add,
    ProjectSelect,
}

struct State<'a> {
    app: &'a mut AppState,
    screen: Screen,
    selected: usize,
    show_done: bool,
    cur_id: u32,
    edit_buf: String,
    edit_cur: usize,
    project_items: Vec<ProjectRef>,
    project_selected: usize,
    flash: Option<String>,
    show_help: bool,
}

impl<'a> State<'a> {
    fn new(app: &'a mut AppState) -> Self {
        Self {
            app,
            screen: Screen::List,
            selected: 0,
            show_done: false,
            cur_id: 0,
            edit_buf: String::new(),
            edit_cur: 0,
            project_items: Vec::new(),
            project_selected: 0,
            flash: None,
            show_help: false,
        }
    }

    fn visible_ids(&self) -> Vec<u32> {
        self.app
            .list
            .items
            .iter()
            .filter(|t| if self.show_done { t.done } else { true })
            .map(|t| t.id)
            .collect()
    }

    fn clamp(&mut self) {
        let n = self.visible_ids().len();
        if n == 0 {
            self.selected = 0;
        } else if self.selected >= n {
            self.selected = n - 1;
        }
    }

    fn find(&self, id: u32) -> Option<&Todo> {
        self.app.list.items.iter().find(|t| t.id == id)
    }

    fn set_flash(&mut self, msg: impl Into<String>) {
        self.flash = Some(msg.into());
    }
}

// ── Public entry ──────────────────────────────────────────────────────────────

pub fn run(app: &mut AppState) -> io::Result<()> {
    let mut out = io::stdout();
    terminal::enable_raw_mode()?;
    execute!(out, terminal::EnterAlternateScreen, cursor::Hide)?;

    let result = event_loop(&mut out, &mut State::new(app));

    execute!(out, cursor::Show, terminal::LeaveAlternateScreen)?;
    terminal::disable_raw_mode()?;
    result
}

// ── Event loop ────────────────────────────────────────────────────────────────

fn event_loop(out: &mut impl Write, s: &mut State) -> io::Result<()> {
    loop {
        let (cols, rows) = terminal::size()?;
        draw(out, s, cols, rows)?;

        match event::read()? {
            Event::Resize(_, _) => continue,
            Event::Key(k) => {
                if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
                    break;
                }
                match s.screen {
                    Screen::List => {
                        if key_list(s, k.code) {
                            break;
                        }
                    }
                    Screen::Detail => key_detail(s, k.code),
                    Screen::Edit => key_edit(s, k.code, k.modifiers),
                    Screen::Add => key_add(s, k.code, k.modifiers),
                    Screen::ProjectSelect => key_project_select(s, k.code),
                }
            }
            _ => {}
        }
    }
    Ok(())
}

// ── Key handlers ──────────────────────────────────────────────────────────────

fn key_list(s: &mut State, code: KeyCode) -> bool {
    let ids = s.visible_ids();
    match code {
        KeyCode::Char('q') | KeyCode::Esc => return true,

        KeyCode::Up | KeyCode::Char('k') => {
            if s.selected > 0 {
                s.selected -= 1;
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if s.selected + 1 < ids.len() {
                s.selected += 1;
            }
        }

        KeyCode::Enter => {
            if let Some(&id) = ids.get(s.selected) {
                s.cur_id = id;
                s.screen = Screen::Detail;
            }
        }

        // Jump-open by number
        KeyCode::Char(c @ '1'..='9') => {
            let idx = (c as usize) - ('1' as usize);
            if let Some(&id) = ids.get(idx) {
                s.cur_id = id;
                s.screen = Screen::Detail;
            }
        }

        KeyCode::Char('d') => {
            if let Some(&id) = ids.get(s.selected) {
                s.app.toggle_done(id);
                persist_and_sync(s);
                s.clamp();
            }
        }

        KeyCode::Char('x') | KeyCode::Delete => {
            if let Some(&id) = ids.get(s.selected) {
                s.app.delete(id);
                persist_and_sync(s);
                s.clamp();
            }
        }

        KeyCode::Tab => {
            s.show_done = !s.show_done;
            s.selected = 0;
        }

        KeyCode::Char('n') => {
            s.edit_buf = String::new();
            s.edit_cur = 0;
            s.screen = Screen::Add;
        }
        KeyCode::Char('h') => {
            s.show_help = !s.show_help;
        }
        KeyCode::Char('r') => match commands::sync_and_pull_default(s.app, false) {
            Ok((_queued, _pulled)) => {
                storage::save(s.app).ok();
                s.selected = 0;
                s.show_done = false;
                s.set_flash("Refreshed from TickTick.");
            }
            Err(e) => s.set_flash(format!("Refresh failed: {e}")),
        },
        KeyCode::Char('p') => open_project_picker(s),

        _ => {}
    }
    false
}

fn key_detail(s: &mut State, code: KeyCode) {
    match code {
        KeyCode::Char('h') => {
            s.show_help = !s.show_help;
        }
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Backspace => {
            s.screen = Screen::List;
        }
        KeyCode::Char('d') => {
            s.app.toggle_done(s.cur_id);
            persist_and_sync(s);
        }
        KeyCode::Char('e') => {
            let desc = s
                .find(s.cur_id)
                .map(|t| t.description.clone())
                .unwrap_or_default();
            s.edit_cur = desc.chars().count();
            s.edit_buf = desc;
            s.screen = Screen::Edit;
        }
        KeyCode::Char('x') | KeyCode::Delete => {
            s.app.delete(s.cur_id);
            persist_and_sync(s);
            s.clamp();
            s.screen = Screen::List;
        }
        KeyCode::Char('r') => match commands::sync_and_pull_default(s.app, false) {
            Ok((_queued, _pulled)) => {
                storage::save(s.app).ok();
                s.selected = 0;
                s.show_done = false;
                s.screen = Screen::List;
                s.set_flash("Refreshed from TickTick.");
            }
            Err(e) => s.set_flash(format!("Refresh failed: {e}")),
        },
        _ => {}
    }
}

fn key_edit(s: &mut State, code: KeyCode, mods: KeyModifiers) {
    let plain = !mods.contains(KeyModifiers::CONTROL) && !mods.contains(KeyModifiers::ALT);
    match code {
        KeyCode::Esc => {
            s.screen = Screen::Detail;
        }
        KeyCode::Enter => {
            let desc = s.edit_buf.clone();
            s.app.set_description(s.cur_id, desc);
            persist_and_sync(s);
            s.screen = Screen::Detail;
        }
        KeyCode::Char(c) if plain => {
            let bp = byte_pos(&s.edit_buf, s.edit_cur);
            s.edit_buf.insert(bp, c);
            s.edit_cur += 1;
        }
        KeyCode::Backspace if s.edit_cur > 0 => {
            let bp = byte_pos(&s.edit_buf, s.edit_cur - 1);
            s.edit_buf.remove(bp);
            s.edit_cur -= 1;
        }
        KeyCode::Delete => {
            let len = s.edit_buf.chars().count();
            if s.edit_cur < len {
                let bp = byte_pos(&s.edit_buf, s.edit_cur);
                s.edit_buf.remove(bp);
            }
        }
        KeyCode::Left if s.edit_cur > 0 => {
            s.edit_cur -= 1;
        }
        KeyCode::Right => {
            if s.edit_cur < s.edit_buf.chars().count() {
                s.edit_cur += 1;
            }
        }
        KeyCode::Home => {
            s.edit_cur = 0;
        }
        KeyCode::End => {
            s.edit_cur = s.edit_buf.chars().count();
        }
        _ => {}
    }
}

fn key_add(s: &mut State, code: KeyCode, mods: KeyModifiers) {
    let plain = !mods.contains(KeyModifiers::CONTROL) && !mods.contains(KeyModifiers::ALT);
    match code {
        KeyCode::Esc => {
            s.screen = Screen::List;
        }
        KeyCode::Enter => {
            let title = s.edit_buf.trim().to_string();
            if !title.is_empty() {
                s.app.add_task(title);
                persist_and_sync(s);
                // select the newly added item
                let ids = s.app.list.items.iter().map(|t| t.id).collect::<Vec<_>>();
                if let Some(&new_id) = ids.last() {
                    s.cur_id = new_id;
                    // find its visible index
                    let visible = s
                        .app
                        .list
                        .items
                        .iter()
                        .filter(|t| if s.show_done { t.done } else { true })
                        .map(|t| t.id)
                        .collect::<Vec<_>>();
                    s.selected = visible.iter().position(|&id| id == new_id).unwrap_or(0);
                }
            }
            s.screen = Screen::List;
        }
        KeyCode::Char(c) if plain => {
            let bp = byte_pos(&s.edit_buf, s.edit_cur);
            s.edit_buf.insert(bp, c);
            s.edit_cur += 1;
        }
        KeyCode::Backspace if s.edit_cur > 0 => {
            let bp = byte_pos(&s.edit_buf, s.edit_cur - 1);
            s.edit_buf.remove(bp);
            s.edit_cur -= 1;
        }
        KeyCode::Delete => {
            let len = s.edit_buf.chars().count();
            if s.edit_cur < len {
                let bp = byte_pos(&s.edit_buf, s.edit_cur);
                s.edit_buf.remove(bp);
            }
        }
        KeyCode::Left if s.edit_cur > 0 => {
            s.edit_cur -= 1;
        }
        KeyCode::Right => {
            if s.edit_cur < s.edit_buf.chars().count() {
                s.edit_cur += 1;
            }
        }
        KeyCode::Home => {
            s.edit_cur = 0;
        }
        KeyCode::End => {
            s.edit_cur = s.edit_buf.chars().count();
        }
        _ => {}
    }
}

fn key_project_select(s: &mut State, code: KeyCode) {
    match code {
        KeyCode::Char('h') => {
            s.show_help = !s.show_help;
        }
        KeyCode::Esc | KeyCode::Char('q') => {
            s.screen = Screen::List;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if s.project_selected > 0 {
                s.project_selected -= 1;
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if s.project_selected + 1 < s.project_items.len() {
                s.project_selected += 1;
            }
        }
        KeyCode::Char('r') => open_project_picker(s),
        KeyCode::Enter => {
            if let Some(p) = s.project_items.get(s.project_selected).cloned() {
                match commands::switch_project_and_pull(s.app, &p.id) {
                    Ok(_n) => {
                        s.selected = 0;
                        s.show_done = false;
                        s.screen = Screen::List;
                        s.set_flash(format!("Switched to '{}'.", p.name));
                    }
                    Err(e) => s.set_flash(format!("Project switch failed: {e}")),
                }
            }
        }
        _ => {}
    }
}

fn open_project_picker(s: &mut State) {
    match commands::fetch_projects(s.app) {
        Ok(projects) => {
            s.project_items = projects
                .into_iter()
                .map(|p| ProjectRef {
                    id: p.id,
                    name: p.name,
                })
                .collect();
            s.project_selected = 0;
            if s.project_items.is_empty() {
                s.set_flash("No TickTick projects found.");
            } else {
                s.screen = Screen::ProjectSelect;
            }
        }
        Err(e) => s.set_flash(format!("Cannot load projects: {e}")),
    }
}

fn byte_pos(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(b, _)| b)
        .unwrap_or(s.len())
}

fn persist_and_sync(s: &mut State) {
    storage::save(s.app).ok();
    commands::sync_pending(s.app, false).ok();
    storage::save(s.app).ok();
}

// ── Top-level draw ────────────────────────────────────────────────────────────

fn draw(out: &mut impl Write, s: &State, cols: u16, rows: u16) -> io::Result<()> {
    queue!(out, terminal::Clear(ClearType::All), cursor::Hide)?;

    if cols < 24 || rows < 10 {
        queue!(
            out,
            cursor::MoveTo(0, 0),
            SetForegroundColor(Color::Red),
            Print("Terminal too small!"),
            ResetColor
        )?;
        return out.flush();
    }

    match s.screen {
        Screen::List => draw_list(out, s, cols, rows)?,
        Screen::Detail => draw_detail(out, s, cols, rows)?,
        Screen::Edit => draw_edit(out, s, cols, rows)?,
        Screen::Add => draw_add(out, s, cols, rows)?,
        Screen::ProjectSelect => draw_project_select(out, s, cols, rows)?,
    }
    out.flush()
}

// ── Chrome helpers ────────────────────────────────────────────────────────────

fn top_bar(out: &mut impl Write, title: &str, hint: &str, cols: u16) -> io::Result<()> {
    let w = cols as usize;

    // Row 0: ╭─╮
    queue!(out, cursor::MoveTo(0, 0), SetForegroundColor(C_BORDER))?;
    queue!(out, Print("╭"), Print("─".repeat(w - 2)), Print("╮"))?;

    // Row 1: │ Title          hint │
    queue!(out, cursor::MoveTo(0, 1), Print("│ "))?;
    let title_max = w.saturating_sub(6);
    let title_text = trunc_display(title, title_max);
    queue!(
        out,
        SetForegroundColor(C_TITLE),
        Print(&title_text),
        ResetColor
    )?;

    let right = if hint.is_empty() {
        String::new()
    } else {
        format!(" {} ", trunc_display(hint, w.saturating_sub(8)))
    };
    // w = 2("│ ") + title + mid + right + 1("│")
    let title_w = display_width(&title_text);
    let right_w = display_width(&right);
    let mid = w.saturating_sub(3 + title_w + right_w);
    queue!(out, Print(" ".repeat(mid)))?;
    if !right.is_empty() {
        queue!(out, SetForegroundColor(C_DIM), Print(&right), ResetColor)?;
    }
    queue!(out, SetForegroundColor(C_BORDER), Print("│"), ResetColor)?;

    // Row 2: ├─┤
    queue!(out, cursor::MoveTo(0, 2), SetForegroundColor(C_BORDER))?;
    queue!(
        out,
        Print("├"),
        Print("─".repeat(w - 2)),
        Print("┤"),
        ResetColor
    )?;

    Ok(())
}

fn bot_bar(out: &mut impl Write, help: &str, cols: u16, rows: u16) -> io::Result<()> {
    let w = cols as usize;

    // Row rows-3: ├─┤
    queue!(
        out,
        cursor::MoveTo(0, rows - 3),
        SetForegroundColor(C_BORDER)
    )?;
    queue!(out, Print("├"), Print("─".repeat(w - 2)), Print("┤"))?;

    // Row rows-2: │ help │
    queue!(out, cursor::MoveTo(0, rows - 2), Print("│ "))?;
    queue!(out, SetForegroundColor(C_DIM))?;
    let h = trunc_display(help, w - 4);
    // 2("│ ") + h + pad + 1("│") = w  → pad = w - 3 - h.len()
    let pad = w.saturating_sub(display_width(&h) + 3);
    queue!(out, Print(&h), Print(" ".repeat(pad)))?;
    queue!(out, SetForegroundColor(C_BORDER), Print("│"))?;

    // Row rows-1: ╰─╯
    queue!(out, cursor::MoveTo(0, rows - 1))?;
    queue!(
        out,
        Print("╰"),
        Print("─".repeat(w - 2)),
        Print("╯"),
        ResetColor
    )?;

    Ok(())
}

/// Draw "│" at col 0 and col cols-1 for rows [from, to).
fn side_borders(out: &mut impl Write, from: u16, to: u16, cols: u16) -> io::Result<()> {
    for r in from..to {
        queue!(
            out,
            cursor::MoveTo(0, r),
            SetForegroundColor(C_BORDER),
            Print("│")
        )?;
        queue!(out, cursor::MoveTo(cols - 1, r), Print("│"), ResetColor)?;
    }
    Ok(())
}

// ── List screen ───────────────────────────────────────────────────────────────

fn draw_list(out: &mut impl Write, s: &State, cols: u16, rows: u16) -> io::Result<()> {
    let (title, hint) = if s.show_done {
        ("Done", "Tab → All")
    } else {
        ("Tasks", "Tab → Done")
    };
    top_bar(out, title, hint, cols)?;
    let base_help =
        "↑↓/jk: move  Enter/1-9: open  n: new  d: done  x: delete  p: project  r: refresh  Tab: switch  q: quit";
    let help_line = footer_line(s, base_help);
    bot_bar(out, &help_line, cols, rows)?;

    let c_start = 3u16;
    let c_end = rows - 3;
    side_borders(out, c_start, c_end, cols)?;

    let ids = s.visible_ids();
    let height = (c_end - c_start) as usize;
    let scroll = if s.selected >= height {
        s.selected - height + 1
    } else {
        0
    };

    for (draw_i, (list_i, &id)) in ids.iter().enumerate().skip(scroll).take(height).enumerate() {
        if let Some(todo) = s.find(id) {
            draw_row(
                out,
                todo,
                list_i,
                list_i == s.selected,
                cols,
                c_start + draw_i as u16,
            )?;
        }
    }

    if ids.is_empty() {
        queue!(
            out,
            cursor::MoveTo(4, c_start + 1),
            SetForegroundColor(C_DIM)
        )?;
        let msg = if s.show_done {
            "No completed tasks yet."
        } else {
            "No tasks — press n to add one, or run: todo <title>"
        };
        queue!(out, Print(msg), ResetColor)?;
    }

    Ok(())
}

fn draw_row(
    out: &mut impl Write,
    t: &Todo,
    idx: usize,
    sel: bool,
    cols: u16,
    row: u16,
) -> io::Result<()> {
    // Start just after the left border (which side_borders already drew at col 0).
    queue!(out, cursor::MoveTo(1, row))?;

    let bg = if sel { Some(C_SEL_BG) } else { None };
    if let Some(bg) = bg {
        queue!(out, SetBackgroundColor(bg))?;
    }

    // Checkbox
    let (cb, cb_c) = if t.done {
        ("[✓]", C_DONE)
    } else {
        ("[ ]", C_DIM)
    };
    queue!(out, Print(" "), SetForegroundColor(cb_c), Print(cb))?;

    // Index number
    if let Some(bg) = bg {
        queue!(out, SetBackgroundColor(bg))?;
    }
    queue!(
        out,
        SetForegroundColor(C_DIM),
        Print(format!(" {:>2}. ", idx + 1))
    )?;

    // Title
    if let Some(bg) = bg {
        queue!(out, SetBackgroundColor(bg))?;
    }
    let max_t = (cols as usize).saturating_sub(16);
    let title = trunc_display(&t.title, max_t);
    let title_c = if t.done { C_DIM } else { C_BODY };
    queue!(out, SetForegroundColor(title_c), Print(&title))?;

    // Description dot indicator
    let has_desc = !t.description.is_empty();
    if has_desc {
        if let Some(bg) = bg {
            queue!(out, SetBackgroundColor(bg))?;
        }
        queue!(out, SetForegroundColor(C_ACCENT), Print(" ·"))?;
    }

    // Padding to fill the row up to the right border
    // inner width = cols - 2 (excluding both "│" chars)
    // written so far from col 1: 1(sp) + 3(cb) + 5(num) + title + opt 2(dot)
    let used = 1 + 3 + 5 + display_width(&title) + if has_desc { 2 } else { 0 };
    let inner = (cols as usize) - 2;
    let pad = inner.saturating_sub(used);
    if let Some(bg) = bg {
        queue!(out, SetBackgroundColor(bg))?;
    }
    queue!(out, Print(" ".repeat(pad)), ResetColor)?;

    Ok(())
}

// ── Detail screen ─────────────────────────────────────────────────────────────

fn draw_detail(out: &mut impl Write, s: &State, cols: u16, rows: u16) -> io::Result<()> {
    let todo = match s.find(s.cur_id) {
        Some(t) => t.clone(),
        None => return Ok(()),
    };

    top_bar(out, "Task", "", cols)?;
    bot_bar(
        out,
        &footer_line(s, "e: edit desc  d: toggle done  x: delete  Esc/q: back"),
        cols,
        rows,
    )?;

    let c_start = 3u16;
    let c_end = rows - 3;
    let w = cols as usize;
    side_borders(out, c_start, c_end, cols)?;

    // Title
    queue!(
        out,
        cursor::MoveTo(2, c_start + 1),
        SetForegroundColor(C_TITLE)
    )?;
    let max_t = w.saturating_sub(14);
    queue!(out, Print(trunc(&todo.title, max_t)), ResetColor)?;

    // Status badge (right-aligned)
    let (badge, badge_bg) = if todo.done {
        (" DONE ", Color::Green)
    } else {
        (" PENDING ", Color::Yellow)
    };
    let bc = cols.saturating_sub(badge.len() as u16 + 2);
    queue!(out, cursor::MoveTo(bc, c_start + 1))?;
    queue!(
        out,
        SetForegroundColor(Color::Black),
        SetBackgroundColor(badge_bg)
    )?;
    queue!(out, Print(badge), ResetColor)?;

    // Divider
    queue!(
        out,
        cursor::MoveTo(2, c_start + 2),
        SetForegroundColor(C_DIM)
    )?;
    queue!(out, Print("─".repeat(w - 4)), ResetColor)?;

    // Description
    queue!(
        out,
        cursor::MoveTo(2, c_start + 4),
        SetForegroundColor(C_DIM)
    )?;
    queue!(out, Print("Description"), ResetColor)?;

    if todo.description.is_empty() {
        queue!(
            out,
            cursor::MoveTo(4, c_start + 5),
            SetForegroundColor(C_DIM)
        )?;
        queue!(out, Print("(empty — press e to add one)"), ResetColor)?;
    } else {
        let lines = word_wrap(&todo.description, w - 6);
        for (i, line) in lines.iter().enumerate() {
            let r = c_start + 5 + i as u16;
            if r >= c_end {
                break;
            }
            queue!(
                out,
                cursor::MoveTo(4, r),
                SetForegroundColor(C_BODY),
                Print(line),
                ResetColor
            )?;
        }
    }

    Ok(())
}

// ── Edit screen ───────────────────────────────────────────────────────────────

fn draw_edit(out: &mut impl Write, s: &State, cols: u16, rows: u16) -> io::Result<()> {
    let todo = match s.find(s.cur_id) {
        Some(t) => t.clone(),
        None => return Ok(()),
    };

    top_bar(out, "Edit Description", "", cols)?;
    bot_bar(
        out,
        &footer_line(s, "Enter: save  Esc: cancel  ←→: cursor  Backspace: delete"),
        cols,
        rows,
    )?;

    let c_start = 3u16;
    let c_end = rows - 3;
    let w = cols as usize;
    side_borders(out, c_start, c_end, cols)?;

    // Task title as context
    queue!(
        out,
        cursor::MoveTo(2, c_start + 1),
        SetForegroundColor(C_TITLE)
    )?;
    queue!(out, Print(trunc(&todo.title, w - 4)), ResetColor)?;

    // Divider
    queue!(
        out,
        cursor::MoveTo(2, c_start + 2),
        SetForegroundColor(C_DIM)
    )?;
    queue!(out, Print("─".repeat(w - 4)), ResetColor)?;

    // Prompt
    queue!(
        out,
        cursor::MoveTo(2, c_start + 4),
        SetForegroundColor(C_DIM)
    )?;
    queue!(
        out,
        Print("Description (Enter to save, Esc to cancel):"),
        ResetColor
    )?;

    // Input box
    // Box occupies cols 2..=(w-3), inner text width = w - 8
    let box_w = (w as isize - 8).max(4) as usize;
    let box_row = c_start + 6;

    queue!(
        out,
        cursor::MoveTo(2, box_row - 1),
        SetForegroundColor(C_DIM)
    )?;
    queue!(
        out,
        Print("┌"),
        Print("─".repeat(box_w + 2)),
        Print("┐"),
        ResetColor
    )?;

    queue!(
        out,
        cursor::MoveTo(2, box_row),
        SetForegroundColor(C_DIM),
        Print("│ "),
        ResetColor
    )?;

    // Scrollable text with virtual viewport
    let chars: Vec<char> = s.edit_buf.chars().collect();
    let len = chars.len();
    let start = if s.edit_cur >= box_w {
        s.edit_cur + 1 - box_w
    } else {
        0
    };
    let end = (start + box_w).min(len);
    let vis: String = chars[start..end].iter().collect();
    let vis_cur = s.edit_cur - start;

    queue!(out, SetForegroundColor(C_BODY), Print(&vis))?;
    queue!(out, Print(" ".repeat(box_w.saturating_sub(vis.len()))))?;
    queue!(out, SetForegroundColor(C_DIM), Print(" │"), ResetColor)?;

    queue!(
        out,
        cursor::MoveTo(2, box_row + 1),
        SetForegroundColor(C_DIM)
    )?;
    queue!(
        out,
        Print("└"),
        Print("─".repeat(box_w + 2)),
        Print("┘"),
        ResetColor
    )?;

    // Show the text cursor inside the box
    queue!(
        out,
        cursor::MoveTo(4 + vis_cur as u16, box_row),
        cursor::Show
    )?;

    Ok(())
}

// ── Add screen ────────────────────────────────────────────────────────────────

fn draw_add(out: &mut impl Write, s: &State, cols: u16, rows: u16) -> io::Result<()> {
    top_bar(out, "New Task", "", cols)?;
    bot_bar(
        out,
        &footer_line(
            s,
            "Enter: add task  Esc: cancel  ←→: cursor  Backspace: delete",
        ),
        cols,
        rows,
    )?;

    let c_start = 3u16;
    let c_end = rows - 3;
    let w = cols as usize;
    side_borders(out, c_start, c_end, cols)?;

    // Prompt
    queue!(
        out,
        cursor::MoveTo(2, c_start + 2),
        SetForegroundColor(C_DIM)
    )?;
    queue!(
        out,
        Print("Task title (Enter to add, Esc to cancel):"),
        ResetColor
    )?;

    // Input box
    let box_w = (w as isize - 8).max(4) as usize;
    let box_row = c_start + 4;

    queue!(
        out,
        cursor::MoveTo(2, box_row - 1),
        SetForegroundColor(C_DIM)
    )?;
    queue!(
        out,
        Print("┌"),
        Print("─".repeat(box_w + 2)),
        Print("┐"),
        ResetColor
    )?;

    queue!(
        out,
        cursor::MoveTo(2, box_row),
        SetForegroundColor(C_DIM),
        Print("│ "),
        ResetColor
    )?;

    let chars: Vec<char> = s.edit_buf.chars().collect();
    let len = chars.len();
    let start = if s.edit_cur >= box_w {
        s.edit_cur + 1 - box_w
    } else {
        0
    };
    let end = (start + box_w).min(len);
    let vis: String = chars[start..end].iter().collect();
    let vis_cur = s.edit_cur - start;

    queue!(out, SetForegroundColor(C_BODY), Print(&vis))?;
    queue!(out, Print(" ".repeat(box_w.saturating_sub(vis.len()))))?;
    queue!(out, SetForegroundColor(C_DIM), Print(" │"), ResetColor)?;

    queue!(
        out,
        cursor::MoveTo(2, box_row + 1),
        SetForegroundColor(C_DIM)
    )?;
    queue!(
        out,
        Print("└"),
        Print("─".repeat(box_w + 2)),
        Print("┘"),
        ResetColor
    )?;

    // Show the text cursor inside the box
    queue!(
        out,
        cursor::MoveTo(4 + vis_cur as u16, box_row),
        cursor::Show
    )?;

    Ok(())
}

fn draw_project_select(out: &mut impl Write, s: &State, cols: u16, rows: u16) -> io::Result<()> {
    top_bar(out, "TickTick Projects", "Enter → switch", cols)?;
    bot_bar(
        out,
        &footer_line(
            s,
            "↑↓/jk: move  Enter: use project  r: reload projects  Esc/q: back",
        ),
        cols,
        rows,
    )?;

    let c_start = 3u16;
    let c_end = rows - 3;
    side_borders(out, c_start, c_end, cols)?;

    let height = (c_end - c_start) as usize;
    let scroll = if s.project_selected >= height {
        s.project_selected - height + 1
    } else {
        0
    };

    for (draw_i, (idx, proj)) in s
        .project_items
        .iter()
        .enumerate()
        .skip(scroll)
        .take(height)
        .enumerate()
    {
        queue!(out, cursor::MoveTo(1, c_start + draw_i as u16))?;
        let is_sel = idx == s.project_selected;
        if is_sel {
            queue!(out, SetBackgroundColor(C_SEL_BG))?;
        }
        let marker = if s.app.ticktick.default_project_id.as_deref() == Some(proj.id.as_str()) {
            " *"
        } else {
            ""
        };
        let text = format!(" {:>2}. {}{} ({})", idx + 1, proj.name, marker, proj.id);
        let row_text = trunc_display(&text, (cols as usize).saturating_sub(3));
        queue!(
            out,
            SetForegroundColor(C_BODY),
            Print(&row_text),
            Print(" ".repeat((cols as usize).saturating_sub(2 + display_width(&row_text)))),
            ResetColor
        )?;
    }

    if s.project_items.is_empty() {
        queue!(
            out,
            cursor::MoveTo(4, c_start + 1),
            SetForegroundColor(C_DIM),
            Print("No projects loaded. Press r to reload."),
            ResetColor
        )?;
    }

    Ok(())
}

// ── String helpers ────────────────────────────────────────────────────────────

fn trunc(s: &str, max: usize) -> String {
    let v: Vec<char> = s.chars().collect();
    if v.len() <= max {
        s.to_string()
    } else if max > 1 {
        v[..max - 1].iter().collect::<String>() + "…"
    } else {
        v[..max].iter().collect()
    }
}

fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

fn trunc_display(s: &str, max: usize) -> String {
    if display_width(s) <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }

    let mut out = String::new();
    let mut used = 0usize;
    let ellipsis = "…";
    let ellipsis_w = display_width(ellipsis);
    let limit = max.saturating_sub(ellipsis_w);

    for ch in s.chars() {
        let ch_w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + ch_w > limit {
            break;
        }
        out.push(ch);
        used += ch_w;
    }

    out.push_str(ellipsis);
    out
}

fn word_wrap(s: &str, max_w: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        if cur.is_empty() {
            cur = word.to_string();
        } else if cur.len() + 1 + word.len() <= max_w {
            cur.push(' ');
            cur.push_str(word);
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = word.to_string();
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn footer_line(s: &State, help_text: &str) -> String {
    if s.show_help {
        return help_text.to_string();
    }

    let project_name = s
        .app
        .ticktick
        .default_project_name
        .as_deref()
        .unwrap_or("offline");
    let task_count = s.app.list.items.len();
    let pending_sync = s.app.ticktick.queue.len();
    let mut status =
        format!("Project: {project_name}  Tasks: {task_count}  Pending Sync: {pending_sync}");

    if let Some(msg) = &s.flash {
        status.push_str("  |  ");
        status.push_str(msg);
    }
    status
}

use std::{
    env,
    io::{self, Write},
};

use crate::{
    app_state::{AppState, ProjectRef, SyncEvent},
    storage,
    ticktick::{self, TickTickApi, TickTickProject, TickTickTask},
    todo::{Todo, TodoList},
};

pub enum CommandOutcome {
    OpenUi,
    Exit(i32),
}

pub fn handle_args(args: &[String], state: &mut AppState) -> CommandOutcome {
    if args.is_empty() {
        return CommandOutcome::OpenUi;
    }

    match args[0].as_str() {
        "ticktick" => {
            if let Err(e) = handle_ticktick(&args[1..], state) {
                eprintln!("TickTick error: {e}");
                return CommandOutcome::Exit(1);
            }
            CommandOutcome::Exit(0)
        }
        "sync" | "refresh" => {
            match sync_and_pull_default(state, true) {
                Ok((queued, pulled)) => {
                    println!(
                        "✓ Sync complete. Processed {queued} queued event(s), loaded {pulled} project task(s)."
                    );
                }
                Err(e) => eprintln!("Sync paused: {e}"),
            }
            if let Err(e) = storage::save(state) {
                eprintln!("Failed to save after sync: {e}");
                return CommandOutcome::Exit(1);
            }
            CommandOutcome::Exit(0)
        }
        _ => {
            let title = args.join(" ");
            let id = state.add_task(title.clone());
            if let Err(e) = storage::save(state) {
                eprintln!("Failed to save: {e}");
                return CommandOutcome::Exit(1);
            }
            println!("✓  Added: {title}");
            if state.ticktick.connected {
                if let Err(e) = sync_pending(state, false) {
                    eprintln!("TickTick sync queued for later: {e}");
                } else if let Some(todo) = state.find_task(id) {
                    if let Some(remote_id) = &todo.ticktick_task_id {
                        println!("   Synced to TickTick task id: {remote_id}");
                    }
                }
                let _ = storage::save(state);
            }
            CommandOutcome::Exit(0)
        }
    }
}

fn handle_ticktick(args: &[String], state: &mut AppState) -> Result<(), String> {
    match args.first().map(|s| s.as_str()) {
        Some("connect") => connect_ticktick(state),
        Some("projects") => list_projects(state),
        Some("use") => {
            let project_id = args
                .get(1)
                .ok_or_else(|| "usage: todo ticktick use <project-id>".to_string())?;
            use_project(state, project_id)
        }
        Some("sync") => {
            let (queued, pulled) = sync_and_pull_default(state, true)?;
            println!(
                "✓ Sync complete. Processed {queued} queued event(s), loaded {pulled} project task(s)."
            );
            storage::save(state).map_err(|e| e.to_string())?;
            Ok(())
        }
        Some("status") | None => {
            print_status(state);
            Ok(())
        }
        Some(other) => Err(format!("unknown ticktick subcommand: {other}")),
    }
}

fn connect_ticktick(state: &mut AppState) -> Result<(), String> {
    print_connect_guide()?;
    let proceed = prompt(
        "Press Enter when ready to continue (or type 'cancel'): ",
        Some(""),
    )?;
    if proceed.eq_ignore_ascii_case("cancel") {
        return Err("cancelled by user".into());
    }

    let client_id = env::var("TICKTICK_CLIENT_ID")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or(prompt("TickTick client id: ", None)?);
    let client_secret = env::var("TICKTICK_CLIENT_SECRET")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or(prompt("TickTick client secret: ", None)?);

    let token = ticktick::oauth_connect(&client_id, &client_secret)?;
    ticktick::save_token(&token)?;

    state.ticktick.connected = true;
    state.ticktick.client_id = Some(client_id.clone());
    state.ticktick.client_secret = Some(client_secret.clone());

    let mut api = TickTickApi::new(client_id, client_secret, token)?;
    let projects = api.list_projects()?;
    state.ticktick.projects_cache = projects
        .iter()
        .map(|p| ProjectRef {
            id: p.id.clone(),
            name: p.name.clone(),
        })
        .collect();
    ticktick::save_token(api.token())?;

    choose_default_project_interactively(state, &mut api, &projects)?;
    let pulled = refresh_default_project_from_remote_with_api(state, &mut api)?;
    println!("Loaded {pulled} task(s) from default project.");
    storage::save(state).map_err(|e| e.to_string())?;
    println!("✓ TickTick connected.");
    Ok(())
}

fn print_connect_guide() -> Result<(), String> {
    println!("TickTick setup guide");
    println!("1) Open developer console: https://developer.ticktick.com/manage");
    println!("2) Create an app and copy client id + client secret.");
    println!("3) In app redirect URIs, add exactly: http://127.0.0.1:56610/callback");
    println!("4) Keep credentials ready for this terminal prompt.");
    println!("5) Optional env vars: TICKTICK_CLIENT_ID / TICKTICK_CLIENT_SECRET");
    io::stdout().flush().map_err(|e| e.to_string())
}

fn choose_default_project_interactively(
    state: &mut AppState,
    api: &mut TickTickApi,
    projects: &[TickTickProject],
) -> Result<(), String> {
    println!();
    println!("TickTick projects:");
    if projects.is_empty() {
        println!("  (none)");
    } else {
        for (i, p) in projects.iter().enumerate() {
            println!("  {}. {} ({})", i + 1, p.name, p.id);
        }
    }

    let create_new = prompt(
        "Create a new default project for todo sync? [Y/n]: ",
        Some("y"),
    )?;
    let project = if create_new.trim().is_empty()
        || create_new.eq_ignore_ascii_case("y")
        || create_new.eq_ignore_ascii_case("yes")
    {
        let name = prompt("Project name: ", Some("todo cli synced tasks"))?;
        api.create_project(name.trim())?
    } else if projects.is_empty() {
        let name = prompt(
            "No existing project available. Enter new project name: ",
            Some("todo cli synced tasks"),
        )?;
        api.create_project(name.trim())?
    } else {
        let idx_raw = prompt("Select project number to use as default: ", Some("1"))?;
        let idx = idx_raw
            .trim()
            .parse::<usize>()
            .map_err(|_| "invalid project number".to_string())?;
        let selected = projects
            .get(idx.saturating_sub(1))
            .ok_or_else(|| "project number out of range".to_string())?;
        selected.clone()
    };

    state.set_default_project(project.id.clone(), project.name.clone());
    state.ticktick.projects_cache = api
        .list_projects()?
        .into_iter()
        .map(|p| ProjectRef {
            id: p.id,
            name: p.name,
        })
        .collect();

    println!(
        "Default TickTick project: {} ({})",
        project.name, project.id
    );
    Ok(())
}

fn list_projects(state: &mut AppState) -> Result<(), String> {
    let projects = fetch_projects(state)?;

    println!("TickTick projects:");
    for p in projects {
        let marker = if state.ticktick.default_project_id.as_deref() == Some(p.id.as_str()) {
            " (default)"
        } else {
            ""
        };
        println!("- {} ({}){}", p.name, p.id, marker);
    }
    Ok(())
}

fn use_project(state: &mut AppState, project_id: &str) -> Result<(), String> {
    let pulled = switch_project_and_pull(state, project_id)?;
    println!("Loaded {pulled} task(s) from selected project.");
    Ok(())
}

pub fn switch_project_and_pull(state: &mut AppState, project_id: &str) -> Result<usize, String> {
    let mut api = build_api(state)?;
    let projects = api.list_projects()?;
    let chosen = projects
        .iter()
        .find(|p| p.id == project_id)
        .cloned()
        .ok_or_else(|| format!("project id not found: {project_id}"))?;

    state.set_default_project(chosen.id.clone(), chosen.name.clone());
    state.ticktick.projects_cache = projects
        .into_iter()
        .map(|p| ProjectRef {
            id: p.id,
            name: p.name,
        })
        .collect();
    let pulled = refresh_default_project_from_remote_with_api(state, &mut api)?;
    ticktick::save_token(api.token())?;
    storage::save(state).map_err(|e| e.to_string())?;
    println!("✓ Default project set to {} ({})", chosen.name, chosen.id);
    Ok(pulled)
}

pub fn fetch_projects(state: &mut AppState) -> Result<Vec<TickTickProject>, String> {
    let mut api = build_api(state)?;
    let projects = api.list_projects()?;
    state.ticktick.projects_cache = projects
        .iter()
        .map(|p| ProjectRef {
            id: p.id.clone(),
            name: p.name.clone(),
        })
        .collect();
    ticktick::save_token(api.token())?;
    storage::save(state).map_err(|e| e.to_string())?;
    Ok(projects)
}

pub fn refresh_default_project_from_remote(state: &mut AppState) -> Result<usize, String> {
    let mut api = build_api(state)?;
    let pulled = refresh_default_project_from_remote_with_api(state, &mut api)?;
    ticktick::save_token(api.token())?;
    storage::save(state).map_err(|e| e.to_string())?;
    Ok(pulled)
}

fn refresh_default_project_from_remote_with_api(
    state: &mut AppState,
    api: &mut TickTickApi,
) -> Result<usize, String> {
    let project_id = state
        .ticktick
        .default_project_id
        .clone()
        .ok_or_else(|| "no default TickTick project set".to_string())?;
    let tasks = api.project_tasks(&project_id)?;
    replace_local_tasks(state, tasks);
    Ok(state.list.items.len())
}

fn replace_local_tasks(state: &mut AppState, tasks: Vec<TickTickTask>) {
    let mut items: Vec<Todo> = tasks
        .into_iter()
        .enumerate()
        .map(|(idx, remote)| Todo {
            id: idx as u32,
            title: remote.title,
            description: remote.desc,
            done: remote.status == 2,
            ticktick_task_id: Some(remote.id),
            ticktick_project_id: Some(remote.project_id),
        })
        .collect();
    items.sort_by_key(|t| (t.done, t.id));
    state.list = TodoList {
        next_id: items.len() as u32,
        items,
    };
    state.ticktick.queue.clear();
}

fn print_status(state: &AppState) {
    println!(
        "TickTick: {}",
        if state.ticktick.connected {
            "connected"
        } else {
            "not connected"
        }
    );
    if let (Some(id), Some(name)) = (
        &state.ticktick.default_project_id,
        &state.ticktick.default_project_name,
    ) {
        println!("Default project: {name} ({id})");
    } else {
        println!("Default project: not set");
    }
    println!("Queued sync events: {}", state.ticktick.queue.len());
}

fn prompt(label: &str, default: Option<&str>) -> Result<String, String> {
    print!("{label}");
    io::stdout().flush().map_err(|e| e.to_string())?;
    let mut buf = String::new();
    io::stdin().read_line(&mut buf).map_err(|e| e.to_string())?;
    let value = buf.trim().to_string();
    if value.is_empty() {
        Ok(default.unwrap_or("").to_string())
    } else {
        Ok(value)
    }
}

fn build_api(state: &AppState) -> Result<TickTickApi, String> {
    if !state.ticktick.connected {
        return Err("TickTick is not connected. Run: todo ticktick connect".into());
    }
    let client_id = state
        .ticktick
        .client_id
        .clone()
        .ok_or_else(|| "missing TickTick client id; reconnect required".to_string())?;
    let client_secret = state
        .ticktick
        .client_secret
        .clone()
        .ok_or_else(|| "missing TickTick client secret; reconnect required".to_string())?;
    let token = ticktick::load_token().ok_or_else(|| {
        "TickTick token not found on disk. Run: todo ticktick connect".to_string()
    })?;
    TickTickApi::new(client_id, client_secret, token)
}

pub fn sync_pending(state: &mut AppState, verbose: bool) -> Result<usize, String> {
    if !state.ticktick.connected || state.ticktick.queue.is_empty() {
        return Ok(0);
    }
    let mut api = build_api(state)?;
    let mut processed = 0usize;

    loop {
        let Some(event) = state.ticktick.queue.first().cloned() else {
            break;
        };

        if verbose {
            println!("Syncing: {:?}", event);
        }

        match process_event(state, &mut api, event.clone()) {
            Ok(()) => {
                state.ticktick.queue.remove(0);
                processed += 1;
            }
            Err(e) => {
                ticktick::save_token(api.token())?;
                return Err(e);
            }
        }
    }

    ticktick::save_token(api.token())?;
    storage::save(state).map_err(|e| e.to_string())?;
    Ok(processed)
}

pub fn sync_and_pull_default(
    state: &mut AppState,
    verbose: bool,
) -> Result<(usize, usize), String> {
    let queued = sync_pending(state, verbose)?;
    if !state.ticktick.connected {
        return Ok((queued, state.list.items.len()));
    }
    let pulled = refresh_default_project_from_remote(state)?;
    Ok((queued, pulled))
}

fn process_event(
    state: &mut AppState,
    api: &mut TickTickApi,
    event: SyncEvent,
) -> Result<(), String> {
    match event {
        SyncEvent::CreateTask { local_id } => {
            let Some(todo) = state.find_task(local_id).cloned() else {
                return Ok(());
            };
            if todo.ticktick_task_id.is_some() && todo.ticktick_project_id.is_some() {
                return Ok(());
            }
            let project_id = todo
                .ticktick_project_id
                .clone()
                .or_else(|| state.ticktick.default_project_id.clone())
                .ok_or_else(|| "no default TickTick project set".to_string())?;
            let remote = api.create_task(&project_id, &todo.title, &todo.description, todo.done)?;
            if let Some(t) = state.find_task_mut(local_id) {
                t.ticktick_task_id = Some(remote.id);
                t.ticktick_project_id = Some(remote.project_id);
            }
            Ok(())
        }
        SyncEvent::UpdateTask { local_id } => {
            let Some(todo) = state.find_task(local_id).cloned() else {
                return Ok(());
            };
            let (project_id, task_id) = ensure_remote_task(state, api, local_id, &todo)?;
            api.update_task(
                &project_id,
                &task_id,
                &todo.title,
                &todo.description,
                todo.done,
            )
        }
        SyncEvent::CompleteTask { local_id } => {
            let Some(todo) = state.find_task(local_id).cloned() else {
                return Ok(());
            };
            let (project_id, task_id) = ensure_remote_task(state, api, local_id, &todo)?;
            api.complete_task(&project_id, &task_id)
        }
        SyncEvent::ReopenTask { local_id } => {
            let Some(todo) = state.find_task(local_id).cloned() else {
                return Ok(());
            };
            let (project_id, task_id) = ensure_remote_task(state, api, local_id, &todo)?;
            api.update_task(&project_id, &task_id, &todo.title, &todo.description, false)
        }
        SyncEvent::DeleteTask {
            remote_project_id,
            remote_task_id,
        } => api.delete_task(&remote_project_id, &remote_task_id),
    }
}

fn ensure_remote_task(
    state: &mut AppState,
    api: &mut TickTickApi,
    local_id: u32,
    local_snapshot: &crate::todo::Todo,
) -> Result<(String, String), String> {
    if let (Some(project_id), Some(task_id)) = (
        local_snapshot.ticktick_project_id.clone(),
        local_snapshot.ticktick_task_id.clone(),
    ) {
        return Ok((project_id, task_id));
    }

    let project_id = state
        .ticktick
        .default_project_id
        .clone()
        .ok_or_else(|| "no default TickTick project set".to_string())?;
    let remote = api.create_task(
        &project_id,
        &local_snapshot.title,
        &local_snapshot.description,
        local_snapshot.done,
    )?;
    if let Some(t) = state.find_task_mut(local_id) {
        t.ticktick_task_id = Some(remote.id.clone());
        t.ticktick_project_id = Some(remote.project_id.clone());
    }
    Ok((remote.project_id, remote.id))
}

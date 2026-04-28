use serde::{Deserialize, Serialize};

use crate::todo::{Todo, TodoList};

#[derive(Serialize, Deserialize, Default)]
pub struct AppState {
    #[serde(default)]
    pub list: TodoList,
    #[serde(default)]
    pub ticktick: TickTickState,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct TickTickState {
    pub connected: bool,
    pub client_id: Option<String>,
    pub client_secret: Option<String>,
    pub default_project_id: Option<String>,
    pub default_project_name: Option<String>,
    #[serde(default)]
    pub projects_cache: Vec<ProjectRef>,
    #[serde(default)]
    pub queue: Vec<SyncEvent>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TickTickToken {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at_unix: Option<u64>,
    pub scope: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ProjectRef {
    pub id: String,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum SyncEvent {
    CreateTask {
        local_id: u32,
    },
    UpdateTask {
        local_id: u32,
    },
    CompleteTask {
        local_id: u32,
    },
    ReopenTask {
        local_id: u32,
    },
    DeleteTask {
        remote_project_id: String,
        remote_task_id: String,
    },
}

impl AppState {
    pub fn add_task(&mut self, title: String) -> u32 {
        self.list.add(title);
        let id = self.list.next_id.saturating_sub(1);
        if self.ticktick.connected {
            self.push_event(SyncEvent::CreateTask { local_id: id });
        }
        id
    }

    pub fn toggle_done(&mut self, id: u32) {
        self.list.toggle_done(id);
        let new_done = self.list.items.iter().find(|t| t.id == id).map(|t| t.done);

        if self.ticktick.connected {
            if let Some(done) = new_done {
                if done {
                    self.push_event(SyncEvent::CompleteTask { local_id: id });
                } else {
                    self.push_event(SyncEvent::ReopenTask { local_id: id });
                }
            }
        }
    }

    pub fn set_description(&mut self, id: u32, desc: String) {
        self.list.set_description(id, desc);
        if self.ticktick.connected {
            self.push_event(SyncEvent::UpdateTask { local_id: id });
        }
    }

    pub fn delete(&mut self, id: u32) {
        if let Some(todo) = self.list.items.iter().find(|t| t.id == id).cloned() {
            self.ticktick.queue.retain(|ev| match ev {
                SyncEvent::CreateTask { local_id }
                | SyncEvent::UpdateTask { local_id }
                | SyncEvent::CompleteTask { local_id }
                | SyncEvent::ReopenTask { local_id } => *local_id != id,
                SyncEvent::DeleteTask { .. } => true,
            });

            if let (Some(remote_project_id), Some(remote_task_id)) =
                (todo.ticktick_project_id, todo.ticktick_task_id)
            {
                self.push_event(SyncEvent::DeleteTask {
                    remote_project_id,
                    remote_task_id,
                });
            }
        }
        self.list.delete(id);
    }

    pub fn find_task(&self, id: u32) -> Option<&Todo> {
        self.list.items.iter().find(|t| t.id == id)
    }

    pub fn find_task_mut(&mut self, id: u32) -> Option<&mut Todo> {
        self.list.items.iter_mut().find(|t| t.id == id)
    }

    pub fn set_default_project(&mut self, id: String, name: String) {
        self.ticktick.default_project_id = Some(id);
        self.ticktick.default_project_name = Some(name);
    }

    pub fn push_event(&mut self, event: SyncEvent) {
        self.ticktick.queue.push(event);
    }
}

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Todo {
    pub id:          u32,
    pub title:       String,
    pub description: String,
    pub done:        bool,
}

impl Todo {
    pub fn new(id: u32, title: String) -> Self {
        Self { id, title, description: String::new(), done: false }
    }
}

#[derive(Serialize, Deserialize, Default)]
pub struct TodoList {
    pub items:   Vec<Todo>,
    pub next_id: u32,
}

impl TodoList {
    pub fn add(&mut self, title: String) {
        let id = self.next_id;
        self.next_id += 1;
        self.items.push(Todo::new(id, title));
    }

    pub fn toggle_done(&mut self, id: u32) {
        if let Some(t) = self.items.iter_mut().find(|t| t.id == id) {
            t.done = !t.done;
        }
    }

    pub fn set_description(&mut self, id: u32, desc: String) {
        if let Some(t) = self.items.iter_mut().find(|t| t.id == id) {
            t.description = desc;
        }
    }

    pub fn delete(&mut self, id: u32) {
        self.items.retain(|t| t.id != id);
    }
}

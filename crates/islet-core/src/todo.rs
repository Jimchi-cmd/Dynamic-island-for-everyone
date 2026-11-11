//! Quick notes and to-dos (Islet Pro), kept in todo.json on this computer
//! only. Unfinished items first (oldest first), then finished ones.

use serde::{Deserialize, Serialize};

/// The list holds at most this many items; the oldest finished ones go first.
pub const MAX: usize = 50;
/// The longest item, in characters.
pub const MAX_LEN: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub id: u32,
    pub text: String,
    #[serde(default)]
    pub done: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Todo {
    pub items: Vec<Item>,
}

impl Todo {
    /// Add an item. Returns its id, or an error for an empty line.
    pub fn add(&mut self, text: &str) -> Result<u32, String> {
        let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if text.is_empty() {
            return Err("Write something first.".into());
        }
        let text: String = text.chars().take(MAX_LEN).collect();
        let id = self.items.iter().map(|i| i.id).max().map_or(1, |m| m + 1);
        self.items.push(Item { id, text, done: false });
        while self.items.len() > MAX {
            let drop = self.items.iter().position(|i| i.done).unwrap_or(0);
            self.items.remove(drop);
        }
        Ok(id)
    }

    /// Tick an item off, or back on. False if there is no such item.
    pub fn toggle(&mut self, id: u32) -> bool {
        match self.items.iter_mut().find(|i| i.id == id) {
            Some(i) => {
                i.done = !i.done;
                true
            }
            None => false,
        }
    }

    pub fn remove(&mut self, id: u32) -> bool {
        let before = self.items.len();
        self.items.retain(|i| i.id != id);
        self.items.len() != before
    }

    pub fn clear_done(&mut self) -> usize {
        let before = self.items.len();
        self.items.retain(|i| !i.done);
        before - self.items.len()
    }

    /// How many are still to do.
    pub fn left(&self) -> usize {
        self.items.iter().filter(|i| !i.done).count()
    }

    /// In display order: to do first, then done.
    pub fn ordered(&self) -> Vec<&Item> {
        let (mut todo, done): (Vec<&Item>, Vec<&Item>) = self.items.iter().partition(|i| !i.done);
        todo.extend(done);
        todo
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_ticks_and_orders() {
        let mut t = Todo::default();
        let a = t.add("  Buy   milk ").unwrap();
        let b = t.add("Call mum").unwrap();
        assert!(t.add("   ").is_err());
        assert_eq!(t.items[0].text, "Buy milk");
        assert_eq!(t.left(), 2);
        assert!(t.toggle(a));
        assert!(!t.toggle(99));
        assert_eq!(t.left(), 1);
        assert_eq!(t.ordered().iter().map(|i| i.id).collect::<Vec<_>>(), vec![b, a], "done items go last");
        assert_eq!(t.clear_done(), 1);
        assert!(t.remove(b) && t.items.is_empty());
    }

    #[test]
    fn keeps_ids_unique_and_the_list_bounded() {
        let mut t = Todo::default();
        for i in 0..MAX {
            t.add(&format!("item {i}")).unwrap();
        }
        t.toggle(5);
        t.add("one more").unwrap();
        assert_eq!(t.items.len(), MAX);
        assert!(t.items.iter().all(|i| i.id != 5), "the finished one made room");
        let ids: std::collections::HashSet<u32> = t.items.iter().map(|i| i.id).collect();
        assert_eq!(ids.len(), MAX);
        let long = "x".repeat(500);
        let id = t.add(&long).unwrap();
        assert_eq!(t.items.iter().find(|i| i.id == id).unwrap().text.len(), MAX_LEN);
    }
}

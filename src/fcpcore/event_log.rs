// FCP core event log — ported from Go (fcp-terraform)

#![allow(dead_code)]

use std::collections::HashMap;

/// Marker for checkpoint entries in the event log.
#[derive(Debug, Clone)]
struct CheckpointEntry {
    #[allow(dead_code)]
    name: String,
}

/// Internal entry: either a real event or a checkpoint sentinel.
#[derive(Debug, Clone)]
enum Entry<T: Clone> {
    Event(T),
    Checkpoint(CheckpointEntry),
}

/// Generic cursor-based event log with undo/redo and named checkpoints.
///
/// Events are appended at the cursor position. The cursor always points
/// one past the last applied event. Undo moves the cursor back; redo
/// moves it forward. Appending a new event truncates the redo tail.
///
/// Checkpoint sentinels are stored in the log but skipped during
/// undo/redo traversal.
pub struct EventLog<T: Clone> {
    events: Vec<Entry<T>>,
    cursor: usize,
    checkpoints: HashMap<String, usize>,
}

impl<T: Clone> EventLog<T> {
    /// Creates a new empty EventLog.
    pub fn new() -> Self {
        EventLog {
            events: Vec::new(),
            cursor: 0,
            checkpoints: HashMap::new(),
        }
    }

    /// Appends an event, truncating any redo history beyond the cursor.
    pub fn append(&mut self, event: T) {
        if self.cursor < self.events.len() {
            self.events.truncate(self.cursor);
            // Remove checkpoints pointing beyond new length
            self.checkpoints.retain(|_, idx| *idx <= self.cursor);
        }
        self.events.push(Entry::Event(event));
        self.cursor = self.events.len();
    }

    /// Creates a named checkpoint at the current cursor position.
    pub fn checkpoint(&mut self, name: &str) {
        self.checkpoints.insert(name.to_string(), self.cursor);
        self.events.push(Entry::Checkpoint(CheckpointEntry {
            name: name.to_string(),
        }));
        self.cursor = self.events.len();
    }

    /// Undoes up to `count` non-checkpoint events. Returns events in reverse
    /// order (most recent first) for the caller to reverse-apply.
    pub fn undo(&mut self, count: usize) -> Vec<T> {
        let mut result = Vec::new();
        let mut pos = self.cursor as isize - 1;
        let mut undone = 0;

        while pos >= 0 && undone < count {
            if let Entry::Event(ref ev) = self.events[pos as usize] {
                result.push(ev.clone());
                undone += 1;
            }
            pos -= 1;
        }

        self.cursor = (pos + 1) as usize;
        result
    }

    /// Undoes to a named checkpoint. Returns events in reverse order.
    /// Returns Err if the checkpoint doesn't exist or is at/beyond cursor.
    pub fn undo_to(&mut self, name: &str) -> Result<Vec<T>, String> {
        let target = match self.checkpoints.get(name) {
            Some(&t) if t < self.cursor => t,
            _ => return Err(format!("cannot undo to {:?}", name)),
        };

        let mut result = Vec::new();
        for i in (target..self.cursor).rev() {
            if let Entry::Event(ref ev) = self.events[i] {
                result.push(ev.clone());
            }
        }
        self.cursor = target;
        Ok(result)
    }

    /// Redoes up to `count` non-checkpoint events. Returns events in forward
    /// order for the caller to re-apply.
    pub fn redo(&mut self, count: usize) -> Vec<T> {
        let mut result = Vec::new();
        let mut pos = self.cursor;
        let mut redone = 0;

        while pos < self.events.len() && redone < count {
            if let Entry::Event(ref ev) = self.events[pos] {
                result.push(ev.clone());
                redone += 1;
            }
            pos += 1;
        }

        self.cursor = pos;
        result
    }

    /// Returns the last `count` non-checkpoint events (up to cursor) in
    /// chronological order (oldest first). If count is 0, returns all.
    pub fn recent(&self, count: usize) -> Vec<T> {
        let limit = if count == 0 { self.cursor } else { count };
        let mut result = Vec::new();
        let mut i = self.cursor as isize - 1;
        while i >= 0 && result.len() < limit {
            if let Entry::Event(ref ev) = self.events[i as usize] {
                result.push(ev.clone());
            }
            i -= 1;
        }
        result.reverse();
        result
    }

    /// Returns the current cursor position (one past last applied event).
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Returns the total number of entries in the log (including checkpoints).
    pub fn length(&self) -> usize {
        self.events.len()
    }

    /// Returns whether there are events before the cursor that can be undone.
    pub fn can_undo(&self) -> bool {
        for i in (0..self.cursor).rev() {
            if matches!(self.events[i], Entry::Event(_)) {
                return true;
            }
        }
        false
    }

    /// Returns whether there are events after the cursor that can be redone.
    pub fn can_redo(&self) -> bool {
        self.cursor < self.events.len()
    }
}

impl<T: Clone> Default for EventLog<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_append_and_cursor() {
        let mut log = EventLog::new();
        log.append("a");
        log.append("b");
        assert_eq!(log.cursor(), 2);
        assert_eq!(log.length(), 2);
    }

    #[test]
    fn test_recent() {
        let mut log = EventLog::new();
        log.append("a");
        log.append("b");
        log.append("c");

        let got = log.recent(2);
        assert_eq!(got, vec!["b", "c"]);

        let all = log.recent(0);
        assert_eq!(all, vec!["a", "b", "c"]);
    }

    #[test]
    fn test_undo_most_recent() {
        let mut log = EventLog::new();
        log.append("a");
        log.append("b");
        let undone = log.undo(1);
        assert_eq!(undone, vec!["b"]);
        assert_eq!(log.cursor(), 1);
    }

    #[test]
    fn test_undo_multiple() {
        let mut log = EventLog::new();
        log.append("a");
        log.append("b");
        log.append("c");
        let undone = log.undo(2);
        assert_eq!(undone, vec!["c", "b"]);
        assert_eq!(log.cursor(), 1);
    }

    #[test]
    fn test_undo_empty() {
        let mut log: EventLog<&str> = EventLog::new();
        let undone = log.undo(1);
        assert!(undone.is_empty());
    }

    #[test]
    fn test_undo_skips_checkpoints() {
        let mut log = EventLog::new();
        log.append("a");
        log.checkpoint("cp1");
        log.append("b");
        let undone = log.undo(2);
        assert_eq!(undone, vec!["b", "a"]);
    }

    #[test]
    fn test_redo() {
        let mut log = EventLog::new();
        log.append("a");
        log.append("b");
        log.undo(2);
        let redone = log.redo(2);
        assert_eq!(redone, vec!["a", "b"]);
        assert_eq!(log.cursor(), 2);
    }

    #[test]
    fn test_redo_empty() {
        let mut log = EventLog::new();
        log.append("a");
        let redone = log.redo(1);
        assert!(redone.is_empty());
    }

    #[test]
    fn test_redo_skips_checkpoints() {
        let mut log = EventLog::new();
        log.append("a");
        log.checkpoint("cp1");
        log.append("b");
        log.undo(2);
        let redone = log.redo(2);
        assert_eq!(redone, vec!["a", "b"]);
    }

    #[test]
    fn test_truncate_on_append() {
        let mut log = EventLog::new();
        log.append("a");
        log.append("b");
        log.undo(1); // cursor at 1, "b" in redo tail
        log.append("c"); // should truncate "b"
        assert_eq!(log.length(), 2);
        let redone = log.redo(1);
        assert!(redone.is_empty());
        let all = log.recent(0);
        assert_eq!(all, vec!["a", "c"]);
    }

    #[test]
    fn test_checkpoint_undo_to() {
        let mut log = EventLog::new();
        log.append("a");
        log.checkpoint("v1");
        log.append("b");
        log.append("c");
        let undone = log.undo_to("v1").unwrap();
        assert_eq!(undone, vec!["c", "b"]);
        let recent = log.recent(0);
        assert_eq!(recent, vec!["a"]);
    }

    #[test]
    fn test_checkpoint_unknown_name() {
        let mut log = EventLog::new();
        log.append("a");
        let result = log.undo_to("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_checkpoint_removed_on_truncation() {
        let mut log = EventLog::new();
        log.append("a");
        log.checkpoint("v1");
        log.append("b");
        log.undo(2); // undo b and a, cursor before checkpoint
        log.append("x"); // truncates everything including checkpoint
        let result = log.undo_to("v1");
        assert!(result.is_err(), "checkpoint should be removed after truncation");
    }

    #[test]
    fn test_cursor_starts_at_zero() {
        let log: EventLog<&str> = EventLog::new();
        assert_eq!(log.cursor(), 0);
    }

    #[test]
    fn test_cursor_advances_on_append() {
        let mut log = EventLog::new();
        log.append("a");
        assert_eq!(log.cursor(), 1);
        log.append("b");
        assert_eq!(log.cursor(), 2);
    }

    #[test]
    fn test_cursor_moves_back_on_undo() {
        let mut log = EventLog::new();
        log.append("a");
        log.append("b");
        log.undo(1);
        assert_eq!(log.cursor(), 1);
    }

    #[test]
    fn test_cursor_moves_forward_on_redo() {
        let mut log = EventLog::new();
        log.append("a");
        log.append("b");
        log.undo(1);
        log.redo(1);
        assert_eq!(log.cursor(), 2);
    }

    #[test]
    fn test_can_undo() {
        let mut log: EventLog<&str> = EventLog::new();
        assert!(!log.can_undo(), "CanUndo() should be false for empty log");
        log.append("a");
        assert!(log.can_undo(), "CanUndo() should be true after append");
    }

    #[test]
    fn test_can_redo() {
        let mut log: EventLog<&str> = EventLog::new();
        assert!(!log.can_redo(), "CanRedo() should be false for empty log");
        log.append("a");
        assert!(!log.can_redo(), "CanRedo() should be false at end");
        log.undo(1);
        assert!(log.can_redo(), "CanRedo() should be true after undo");
    }
}

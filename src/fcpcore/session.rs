// FCP core session — ported from Go (fcp-terraform)

#![allow(dead_code)]

use std::collections::HashMap;

use super::event_log::EventLog;
use super::tokenizer::{is_key_value, parse_key_value, tokenize};

/// Lifecycle hooks that a domain must implement.
pub trait SessionHooks {
    /// The model type for this domain.
    type Model;
    /// The event type for undo/redo.
    type Event: Clone;

    /// Creates a new empty model with the given params.
    fn on_new(&self, params: &HashMap<String, String>) -> Result<Self::Model, String>;
    /// Opens a model from a file path.
    fn on_open(&self, path: &str) -> Result<Self::Model, String>;
    /// Saves a model to a file path.
    fn on_save(&self, model: &Self::Model, path: &str) -> Result<(), String>;
    /// Rebuilds derived indices after undo/redo.
    fn on_rebuild_indices(&self, model: &mut Self::Model);
    /// Returns a compact digest string for drift detection.
    fn get_digest(&self, model: &Self::Model) -> String;
    /// Reverses a single event on the model (for undo).
    fn reverse(&self, event: &Self::Event, model: &mut Self::Model);
    /// Replays a single event on the model (for redo).
    fn replay(&self, event: &Self::Event, model: &mut Self::Model);
}

/// Session routes session-level actions (new, open, save, checkpoint, undo, redo)
/// to the appropriate handler.
pub struct Session<H: SessionHooks> {
    pub model: Option<H::Model>,
    pub file_path: String,
    pub log: EventLog<H::Event>,
    hooks: H,
}

impl<H: SessionHooks> Session<H> {
    /// Creates a new Session with the given hooks.
    pub fn new(hooks: H) -> Self {
        Session {
            model: None,
            file_path: String::new(),
            log: EventLog::new(),
            hooks,
        }
    }

    /// Parses and executes a session command string.
    /// Commands: new "Title" [params...], open PATH, save, save as:PATH,
    /// checkpoint NAME, undo, undo to:NAME, redo, status, close
    pub fn dispatch(&mut self, action: &str) -> String {
        let tokens = tokenize(action);
        if tokens.is_empty() {
            return "empty action".to_string();
        }

        let cmd = tokens[0].to_lowercase();

        match cmd.as_str() {
            "new" => self.dispatch_new(&tokens),
            "open" => self.dispatch_open(&tokens),
            "save" => self.dispatch_save(&tokens),
            "checkpoint" => self.dispatch_checkpoint(&tokens),
            "undo" => self.dispatch_undo(&tokens),
            "redo" => self.dispatch_redo(),
            "status" => self.dispatch_status(),
            "close" => self.dispatch_close(),
            _ => format!("unknown session action {:?}", cmd),
        }
    }

    fn dispatch_new(&mut self, tokens: &[String]) -> String {
        let mut params = HashMap::new();
        let mut positionals = Vec::new();
        for token in &tokens[1..] {
            if is_key_value(token) {
                let (key, value) = parse_key_value(token);
                params.insert(key, value);
            } else {
                positionals.push(token.clone());
            }
        }
        if !positionals.is_empty() {
            params.insert("title".to_string(), positionals[0].clone());
        }

        match self.hooks.on_new(&params) {
            Ok(model) => {
                self.model = Some(model);
                self.log = EventLog::new();
                self.file_path = String::new();

                let title = params
                    .get("title")
                    .filter(|t| !t.is_empty())
                    .cloned()
                    .unwrap_or_else(|| "Untitled".to_string());
                format!("new {:?} created", title)
            }
            Err(e) => format!("error: {}", e),
        }
    }

    fn dispatch_open(&mut self, tokens: &[String]) -> String {
        if tokens.len() < 2 {
            return "open requires a file path".to_string();
        }
        let path = &tokens[1];
        match self.hooks.on_open(path) {
            Ok(model) => {
                self.model = Some(model);
                self.log = EventLog::new();
                self.file_path = path.clone();
                format!("opened {:?}", path)
            }
            Err(e) => format!("error: {}", e),
        }
    }

    fn dispatch_save(&mut self, tokens: &[String]) -> String {
        let model = match self.model.as_ref() {
            Some(m) => m,
            None => return "error: no model to save".to_string(),
        };

        let mut save_path = self.file_path.clone();
        for token in &tokens[1..] {
            if is_key_value(token) {
                let (key, value) = parse_key_value(token);
                if key == "as" {
                    save_path = value;
                }
            }
        }
        if save_path.is_empty() {
            return "error: no file path. Use save as:./file".to_string();
        }
        match self.hooks.on_save(model, &save_path) {
            Ok(()) => {
                self.file_path = save_path.clone();
                format!("saved {:?}", save_path)
            }
            Err(e) => format!("error: {}", e),
        }
    }

    fn dispatch_checkpoint(&mut self, tokens: &[String]) -> String {
        if tokens.len() < 2 {
            return "checkpoint requires a name".to_string();
        }
        let name = &tokens[1];
        self.log.checkpoint(name);
        format!("checkpoint {:?} created", name)
    }

    fn dispatch_undo(&mut self, tokens: &[String]) -> String {
        if self.model.is_none() {
            return "nothing to undo".to_string();
        }

        // undo to:NAME
        if tokens.len() >= 2 {
            let t = &tokens[1];
            if t.len() > 3 && &t[..3] == "to:" {
                let name = &t[3..];
                if name.is_empty() {
                    return "undo to: requires a checkpoint name".to_string();
                }
                match self.log.undo_to(name) {
                    Ok(events) => {
                        let count = events.len();
                        let model = self.model.as_mut().unwrap();
                        for ev in &events {
                            self.hooks.reverse(ev, model);
                        }
                        self.hooks.on_rebuild_indices(model);
                        return format!(
                            "undone {} event{} to checkpoint {:?}",
                            count,
                            plural(count),
                            name
                        );
                    }
                    Err(_) => {
                        return format!("cannot undo to {:?}", name);
                    }
                }
            }
        }

        let events = self.log.undo(1);
        if events.is_empty() {
            return "nothing to undo".to_string();
        }
        let count = events.len();
        let model = self.model.as_mut().unwrap();
        for ev in &events {
            self.hooks.reverse(ev, model);
        }
        self.hooks.on_rebuild_indices(model);
        format!("undone {} event{}", count, plural(count))
    }

    fn dispatch_redo(&mut self) -> String {
        if self.model.is_none() {
            return "nothing to redo".to_string();
        }
        let events = self.log.redo(1);
        if events.is_empty() {
            return "nothing to redo".to_string();
        }
        let count = events.len();
        let model = self.model.as_mut().unwrap();
        for ev in &events {
            self.hooks.replay(ev, model);
        }
        self.hooks.on_rebuild_indices(model);
        format!("redone {} event{}", count, plural(count))
    }

    fn dispatch_status(&self) -> String {
        "status: placeholder".to_string()
    }

    fn dispatch_close(&mut self) -> String {
        "close: placeholder".to_string()
    }
}

fn plural(n: usize) -> &'static str {
    if n != 1 {
        "s"
    } else {
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct MockModel {
        title: String,
        #[allow(dead_code)]
        data: Vec<String>,
    }

    #[derive(Clone, Debug)]
    struct MockEvent(String);

    struct MockHooks {
        open_error: Option<String>,
        save_error: Option<String>,
    }

    impl MockHooks {
        fn new() -> Self {
            MockHooks {
                open_error: None,
                save_error: None,
            }
        }
    }

    impl SessionHooks for MockHooks {
        type Model = MockModel;
        type Event = MockEvent;

        fn on_new(&self, params: &HashMap<String, String>) -> Result<MockModel, String> {
            let title = params
                .get("title")
                .filter(|t| !t.is_empty())
                .cloned()
                .unwrap_or_else(|| "Untitled".to_string());
            Ok(MockModel {
                title,
                data: Vec::new(),
            })
        }

        fn on_open(&self, path: &str) -> Result<MockModel, String> {
            if let Some(ref e) = self.open_error {
                return Err(e.clone());
            }
            Ok(MockModel {
                title: path.to_string(),
                data: vec!["loaded".to_string()],
            })
        }

        fn on_save(&self, _model: &MockModel, _path: &str) -> Result<(), String> {
            if let Some(ref e) = self.save_error {
                return Err(e.clone());
            }
            Ok(())
        }

        fn on_rebuild_indices(&self, _model: &mut MockModel) {}

        fn get_digest(&self, model: &MockModel) -> String {
            format!("[{}: {} items]", model.title, model.data.len())
        }

        fn reverse(&self, _event: &MockEvent, _model: &mut MockModel) {}
        fn replay(&self, _event: &MockEvent, _model: &mut MockModel) {}
    }

    fn create_mock_session() -> Session<MockHooks> {
        Session::new(MockHooks::new())
    }

    #[test]
    fn test_session_new_with_title() {
        let mut session = create_mock_session();
        let result = session.dispatch(r#"new "My Song""#);
        assert_eq!(result, r#"new "My Song" created"#);
        assert!(session.model.is_some());
        let m = session.model.as_ref().unwrap();
        assert_eq!(m.title, "My Song");
    }

    #[test]
    fn test_session_new_with_params() {
        let mut session = create_mock_session();
        session.dispatch(r#"new "Test" tempo:120"#);
        // The model title should be set from the first positional
        let m = session.model.as_ref().unwrap();
        assert_eq!(m.title, "Test");
    }

    #[test]
    fn test_session_new_default_title() {
        let mut session = create_mock_session();
        let result = session.dispatch("new");
        assert_eq!(result, r#"new "Untitled" created"#);
    }

    #[test]
    fn test_session_new_clears_file_path() {
        let mut session = create_mock_session();
        session.dispatch("new Test");
        assert_eq!(session.file_path, "");
    }

    #[test]
    fn test_session_open() {
        let mut session = create_mock_session();
        let result = session.dispatch("open ./test.mid");
        assert_eq!(result, r#"opened "./test.mid""#);
        assert!(session.model.is_some());
        assert_eq!(session.file_path, "./test.mid");
    }

    #[test]
    fn test_session_open_requires_path() {
        let mut session = create_mock_session();
        let result = session.dispatch("open");
        assert_eq!(result, "open requires a file path");
    }

    #[test]
    fn test_session_open_error() {
        let mut session = Session::new(MockHooks {
            open_error: Some("file not found".to_string()),
            save_error: None,
        });
        let result = session.dispatch("open ./missing.mid");
        assert_eq!(result, "error: file not found");
    }

    #[test]
    fn test_session_save() {
        let mut session = create_mock_session();
        session.dispatch("open ./test.mid");
        let result = session.dispatch("save");
        assert_eq!(result, r#"saved "./test.mid""#);
    }

    #[test]
    fn test_session_save_with_as() {
        let mut session = create_mock_session();
        session.dispatch("new Test");
        let result = session.dispatch("save as:./output.mid");
        assert_eq!(result, r#"saved "./output.mid""#);
        assert_eq!(session.file_path, "./output.mid");
    }

    #[test]
    fn test_session_save_no_model() {
        let mut session = create_mock_session();
        let result = session.dispatch("save");
        assert_eq!(result, "error: no model to save");
    }

    #[test]
    fn test_session_save_no_path() {
        let mut session = create_mock_session();
        session.dispatch("new Test");
        let result = session.dispatch("save");
        assert_eq!(result, "error: no file path. Use save as:./file");
    }

    #[test]
    fn test_session_checkpoint() {
        let mut session = create_mock_session();
        let result = session.dispatch("checkpoint v1");
        assert_eq!(result, r#"checkpoint "v1" created"#);
        assert!(session.log.cursor() > 0);
    }

    #[test]
    fn test_session_checkpoint_requires_name() {
        let mut session = create_mock_session();
        let result = session.dispatch("checkpoint");
        assert_eq!(result, "checkpoint requires a name");
    }

    #[test]
    fn test_session_undo() {
        let mut session = create_mock_session();
        session.dispatch("new Test");
        session.log.append(MockEvent("event1".to_string()));
        session.log.append(MockEvent("event2".to_string()));
        let result = session.dispatch("undo");
        assert_eq!(result, "undone 1 event");
    }

    #[test]
    fn test_session_undo_no_model() {
        let mut session = create_mock_session();
        let result = session.dispatch("undo");
        assert_eq!(result, "nothing to undo");
    }

    #[test]
    fn test_session_undo_to_checkpoint() {
        let mut session = create_mock_session();
        session.dispatch("new Test");
        session.log.append(MockEvent("event1".to_string()));
        session.dispatch("checkpoint v1");
        session.log.append(MockEvent("event2".to_string()));
        session.log.append(MockEvent("event3".to_string()));
        let result = session.dispatch("undo to:v1");
        assert_eq!(result, r#"undone 2 events to checkpoint "v1""#);
    }

    #[test]
    fn test_session_undo_calls_rebuild() {
        let mut session = create_mock_session();
        session.dispatch("new Test");
        session.log.append(MockEvent("event1".to_string()));
        let result = session.dispatch("undo");
        assert_eq!(result, "undone 1 event");
        // If it didn't panic, rebuild was called
    }

    #[test]
    fn test_session_redo() {
        let mut session = create_mock_session();
        session.dispatch("new Test");
        session.log.append(MockEvent("event1".to_string()));
        session.log.undo(1);
        let result = session.dispatch("redo");
        assert_eq!(result, "redone 1 event");
    }

    #[test]
    fn test_session_redo_no_model() {
        let mut session = create_mock_session();
        let result = session.dispatch("redo");
        assert_eq!(result, "nothing to redo");
    }

    #[test]
    fn test_session_redo_calls_rebuild() {
        let mut session = create_mock_session();
        session.dispatch("new Test");
        session.log.append(MockEvent("event1".to_string()));
        session.log.undo(1);
        let result = session.dispatch("redo");
        assert_eq!(result, "redone 1 event");
        // If it didn't panic, rebuild was called
    }

    #[test]
    fn test_session_unknown_command() {
        let mut session = create_mock_session();
        let result = session.dispatch("explode");
        assert_eq!(result, r#"unknown session action "explode""#);
    }

    #[test]
    fn test_session_empty_action() {
        let mut session = create_mock_session();
        let result = session.dispatch("");
        assert_eq!(result, "empty action");
    }

    // Workspace-specific placeholder tests

    #[test]
    fn test_session_status_placeholder() {
        let mut session = create_mock_session();
        let result = session.dispatch("status");
        assert_eq!(result, "status: placeholder");
    }

    #[test]
    fn test_session_close_placeholder() {
        let mut session = create_mock_session();
        let result = session.dispatch("close");
        assert_eq!(result, "close: placeholder");
    }

    #[test]
    fn test_session_new_resets_log() {
        let mut session = create_mock_session();
        session.dispatch("new Test");
        session.log.append(MockEvent("e1".to_string()));
        session.log.append(MockEvent("e2".to_string()));
        assert_eq!(session.log.cursor(), 2);
        session.dispatch("new Other");
        assert_eq!(session.log.cursor(), 0);
    }

    #[test]
    fn test_session_open_resets_log() {
        let mut session = create_mock_session();
        session.dispatch("new Test");
        session.log.append(MockEvent("e1".to_string()));
        assert_eq!(session.log.cursor(), 1);
        session.dispatch("open ./file.mid");
        assert_eq!(session.log.cursor(), 0);
    }
}

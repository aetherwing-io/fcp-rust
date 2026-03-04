// Verb registration for fcp-rust

use crate::fcpcore::verb_registry::{VerbRegistry, VerbSpec};

pub fn register_query_verbs(registry: &mut VerbRegistry) {
    registry.register_many(vec![
        VerbSpec {
            name: "find".to_string(),
            syntax: "find QUERY [kind:KIND]".to_string(),
            category: "navigation".to_string(),
        },
        VerbSpec {
            name: "def".to_string(),
            syntax: "def SYMBOL [@selectors...]".to_string(),
            category: "navigation".to_string(),
        },
        VerbSpec {
            name: "refs".to_string(),
            syntax: "refs SYMBOL [@selectors...]".to_string(),
            category: "navigation".to_string(),
        },
        VerbSpec {
            name: "symbols".to_string(),
            syntax: "symbols PATH [kind:KIND]".to_string(),
            category: "navigation".to_string(),
        },
        VerbSpec {
            name: "diagnose".to_string(),
            syntax: "diagnose [PATH] [@all]".to_string(),
            category: "inspection".to_string(),
        },
        VerbSpec {
            name: "inspect".to_string(),
            syntax: "inspect SYMBOL [@selectors...]".to_string(),
            category: "inspection".to_string(),
        },
        VerbSpec {
            name: "callers".to_string(),
            syntax: "callers SYMBOL [@selectors...]".to_string(),
            category: "inspection".to_string(),
        },
        VerbSpec {
            name: "callees".to_string(),
            syntax: "callees SYMBOL [@selectors...]".to_string(),
            category: "inspection".to_string(),
        },
        VerbSpec {
            name: "impl".to_string(),
            syntax: "impl SYMBOL [@selectors...]".to_string(),
            category: "navigation".to_string(),
        },
        VerbSpec {
            name: "map".to_string(),
            syntax: "map".to_string(),
            category: "inspection".to_string(),
        },
        VerbSpec {
            name: "unused".to_string(),
            syntax: "unused [@file:PATH]".to_string(),
            category: "inspection".to_string(),
        },
    ]);
}

pub fn register_mutation_verbs(registry: &mut VerbRegistry) {
    registry.register_many(vec![
        VerbSpec {
            name: "rename".to_string(),
            syntax: "rename SYMBOL NEW_NAME [@selectors...]".to_string(),
            category: "mutation".to_string(),
        },
        VerbSpec {
            name: "extract".to_string(),
            syntax: "extract FUNC_NAME @file:PATH @lines:N-M".to_string(),
            category: "mutation".to_string(),
        },
        VerbSpec {
            name: "inline".to_string(),
            syntax: "inline SYMBOL [@selectors...]".to_string(),
            category: "mutation".to_string(),
        },
        VerbSpec {
            name: "generate".to_string(),
            syntax: "generate TRAIT @struct:NAME  (derive: Debug Clone Copy PartialEq Eq Hash PartialOrd Ord Default)".to_string(),
            category: "mutation".to_string(),
        },
        VerbSpec {
            name: "import".to_string(),
            syntax: "import SYMBOL @file:PATH @line:N".to_string(),
            category: "mutation".to_string(),
        },
    ]);
}

pub fn register_session_verbs(registry: &mut VerbRegistry) {
    registry.register_many(vec![
        VerbSpec {
            name: "open".to_string(),
            syntax: "open PATH".to_string(),
            category: "session".to_string(),
        },
        VerbSpec {
            name: "status".to_string(),
            syntax: "status".to_string(),
            category: "session".to_string(),
        },
        VerbSpec {
            name: "close".to_string(),
            syntax: "close".to_string(),
            category: "session".to_string(),
        },
    ]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_query_verbs() {
        let mut reg = VerbRegistry::new();
        register_query_verbs(&mut reg);
        let verbs = ["find", "def", "refs", "symbols", "diagnose", "inspect", "callers", "callees", "impl", "map", "unused"];
        for name in &verbs {
            assert!(reg.lookup(name).is_some(), "missing query verb: {}", name);
        }
    }

    #[test]
    fn test_register_session_verbs() {
        let mut reg = VerbRegistry::new();
        register_session_verbs(&mut reg);
        for name in &["open", "status", "close"] {
            assert!(reg.lookup(name).is_some(), "missing session verb: {}", name);
        }
    }

    #[test]
    fn test_query_verb_count() {
        let mut reg = VerbRegistry::new();
        register_query_verbs(&mut reg);
        assert_eq!(reg.verbs().len(), 11);
    }

    #[test]
    fn test_session_verb_count() {
        let mut reg = VerbRegistry::new();
        register_session_verbs(&mut reg);
        assert_eq!(reg.verbs().len(), 3);
    }

    #[test]
    fn test_register_mutation_verbs() {
        let mut reg = VerbRegistry::new();
        register_mutation_verbs(&mut reg);
        let verbs = ["rename", "extract", "inline", "generate", "import"];
        for name in &verbs {
            assert!(reg.lookup(name).is_some(), "missing mutation verb: {}", name);
        }
    }

    #[test]
    fn test_mutation_verb_count() {
        let mut reg = VerbRegistry::new();
        register_mutation_verbs(&mut reg);
        assert_eq!(reg.verbs().len(), 5);
    }

    #[test]
    fn test_reference_card_has_categories() {
        let mut reg = VerbRegistry::new();
        register_query_verbs(&mut reg);
        register_mutation_verbs(&mut reg);
        register_session_verbs(&mut reg);
        let card = reg.generate_reference_card(None);
        assert!(card.contains("NAVIGATION:"), "card missing NAVIGATION");
        assert!(card.contains("INSPECTION:"), "card missing INSPECTION");
        assert!(card.contains("MUTATION:"), "card missing MUTATION");
        assert!(card.contains("SESSION:"), "card missing SESSION");
    }

    #[test]
    fn test_all_verbs_registered() {
        let mut reg = VerbRegistry::new();
        register_query_verbs(&mut reg);
        register_mutation_verbs(&mut reg);
        register_session_verbs(&mut reg);
        assert_eq!(reg.verbs().len(), 19);
    }
}

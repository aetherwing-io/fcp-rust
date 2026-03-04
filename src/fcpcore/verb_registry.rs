// FCP core verb registry — ported from Go (fcp-terraform)

use std::collections::HashMap;

/// Defines a single verb in an FCP protocol.
#[derive(Debug, Clone)]
pub struct VerbSpec {
    pub name: String,
    pub syntax: String,
    pub category: String,
}

struct CategoryGroup {
    name: String,
    specs: Vec<VerbSpec>,
}

/// Registry of verb specifications that supports lookup by verb name
/// and reference card generation grouped by category.
pub struct VerbRegistry {
    specs: HashMap<String, VerbSpec>,
    categories: Vec<CategoryGroup>,
    cat_index: HashMap<String, usize>,
}

impl VerbRegistry {
    /// Creates a new empty VerbRegistry.
    pub fn new() -> Self {
        VerbRegistry {
            specs: HashMap::new(),
            categories: Vec::new(),
            cat_index: HashMap::new(),
        }
    }

    /// Registers a single verb specification.
    pub fn register(&mut self, spec: VerbSpec) {
        self.specs.insert(spec.name.clone(), spec.clone());
        if let Some(&idx) = self.cat_index.get(&spec.category) {
            self.categories[idx].specs.push(spec);
        } else {
            let idx = self.categories.len();
            self.cat_index.insert(spec.category.clone(), idx);
            self.categories.push(CategoryGroup {
                name: spec.category.clone(),
                specs: vec![spec],
            });
        }
    }

    /// Registers multiple verb specifications at once.
    pub fn register_many(&mut self, specs: Vec<VerbSpec>) {
        for spec in specs {
            self.register(spec);
        }
    }

    /// Looks up a verb specification by name.
    pub fn lookup(&self, name: &str) -> Option<&VerbSpec> {
        self.specs.get(name)
    }

    /// Returns all registered verb specifications.
    pub fn verbs(&self) -> Vec<&VerbSpec> {
        self.specs.values().collect()
    }

    /// Generates a reference card string grouped by category.
    /// Optional extra_sections adds extra static sections appended after the verb listing.
    pub fn generate_reference_card(
        &self,
        extra_sections: Option<&HashMap<String, String>>,
    ) -> String {
        let mut lines: Vec<String> = Vec::new();

        for cat in &self.categories {
            lines.push(format!("{}:", cat.name.to_uppercase()));
            for spec in &cat.specs {
                lines.push(format!("  {}", spec.syntax));
            }
            lines.push(String::new());
        }

        if let Some(sections) = extra_sections {
            for (title, content) in sections {
                lines.push(format!("{}:", title.to_uppercase()));
                lines.push(content.clone());
                lines.push(String::new());
            }
        }

        // Remove trailing empty lines
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }

        lines.join("\n")
    }
}

impl Default for VerbRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_registry() -> VerbRegistry {
        let mut reg = VerbRegistry::new();
        reg.register_many(vec![
            VerbSpec {
                name: "add".to_string(),
                syntax: "add TYPE LABEL [key:value]".to_string(),
                category: "create".to_string(),
            },
            VerbSpec {
                name: "remove".to_string(),
                syntax: "remove SELECTOR".to_string(),
                category: "modify".to_string(),
            },
            VerbSpec {
                name: "connect".to_string(),
                syntax: "connect SRC -> TGT".to_string(),
                category: "create".to_string(),
            },
            VerbSpec {
                name: "style".to_string(),
                syntax: "style REF [fill:#HEX]".to_string(),
                category: "modify".to_string(),
            },
        ]);
        reg
    }

    #[test]
    fn test_register_and_lookup() {
        let mut reg = VerbRegistry::new();
        reg.register(VerbSpec {
            name: "add".to_string(),
            syntax: "add TYPE LABEL".to_string(),
            category: "create".to_string(),
        });
        let spec = reg.lookup("add").expect("expected to find 'add'");
        assert_eq!(spec.name, "add");
        assert_eq!(spec.syntax, "add TYPE LABEL");
        assert_eq!(spec.category, "create");
    }

    #[test]
    fn test_lookup_unknown() {
        let reg = VerbRegistry::new();
        assert!(reg.lookup("nonexistent").is_none());
    }

    #[test]
    fn test_register_many() {
        let reg = create_test_registry();
        for name in &["add", "remove", "connect", "style"] {
            assert!(
                reg.lookup(name).is_some(),
                "expected to find {:?}",
                name
            );
        }
    }

    #[test]
    fn test_verbs() {
        let reg = create_test_registry();
        let verbs = reg.verbs();
        assert_eq!(verbs.len(), 4);
        let names: Vec<&str> = verbs.iter().map(|v| v.name.as_str()).collect();
        for name in &["add", "remove", "connect", "style"] {
            assert!(names.contains(name), "missing verb {:?}", name);
        }
    }

    #[test]
    fn test_reference_card() {
        let reg = create_test_registry();
        let card = reg.generate_reference_card(None);
        assert!(card.contains("CREATE:"), "card missing CREATE:");
        assert!(card.contains("MODIFY:"), "card missing MODIFY:");
        assert!(
            card.contains("  add TYPE LABEL [key:value]"),
            "card missing add syntax"
        );
        assert!(
            card.contains("  connect SRC -> TGT"),
            "card missing connect syntax"
        );
        assert!(
            card.contains("  remove SELECTOR"),
            "card missing remove syntax"
        );
        assert!(
            card.contains("  style REF [fill:#HEX]"),
            "card missing style syntax"
        );
    }

    #[test]
    fn test_reference_card_extra_sections() {
        let reg = create_test_registry();
        let mut extra = HashMap::new();
        extra.insert(
            "Themes".to_string(),
            "  blue  #dae8fc\n  red   #f8cecc".to_string(),
        );
        let card = reg.generate_reference_card(Some(&extra));
        assert!(card.contains("THEMES:"), "card missing THEMES:");
        assert!(
            card.contains("  blue  #dae8fc"),
            "card missing theme content"
        );
    }

    #[test]
    fn test_reference_card_empty() {
        let reg = VerbRegistry::new();
        let card = reg.generate_reference_card(None);
        assert_eq!(card, "");
    }
}

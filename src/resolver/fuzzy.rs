// Tier 3 fuzzy search — deferred to Phase 1.
// rust-analyzer's built-in workspace/symbol fuzzy matching may make this unnecessary.

#[allow(dead_code)]
pub struct FuzzyIndex;

#[allow(dead_code)]
impl FuzzyIndex {
    pub fn new() -> Self {
        FuzzyIndex
    }
}

impl Default for FuzzyIndex {
    fn default() -> Self {
        Self::new()
    }
}

//! Glob based rules that keep the cleaner away from things the user cares about.

use std::path::Path;

use glob::Pattern;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("invalid pattern `{pattern}`: {reason}")]
pub struct InvalidPattern {
    pub pattern: String,
    pub reason: String,
}

#[derive(Clone, Debug, Default)]
pub struct ProtectionRules {
    path_rules: Vec<(String, Pattern)>,
    image_rules: Vec<(String, Pattern)>,
}

impl ProtectionRules {
    pub fn new(path_globs: &[String], image_globs: &[String]) -> Result<Self, InvalidPattern> {
        Ok(Self { path_rules: compile(path_globs)?, image_rules: compile(image_globs)? })
    }

    /// The first path rule that protects `path` or one of its ancestors.
    pub fn protecting_path_rule(&self, path: &Path) -> Option<&str> {
        self.path_rules
            .iter()
            .find(|(_, pattern)| path.ancestors().any(|ancestor| pattern.matches_path(ancestor)))
            .map(|(source, _)| source.as_str())
    }

    /// The first image rule matching the image reference (`repository:tag`), the bare
    /// repository or the image id.
    pub fn protecting_image_rule(&self, reference: &str, repository: &str, id: &str) -> Option<&str> {
        self.image_rules
            .iter()
            .find(|(_, pattern)| {
                pattern.matches(reference)
                    || pattern.matches(repository)
                    || pattern.matches(id)
                    || id.starts_with(pattern.as_str())
            })
            .map(|(source, _)| source.as_str())
    }

    pub fn path_rule_count(&self) -> usize {
        self.path_rules.len()
    }

    pub fn image_rule_count(&self) -> usize {
        self.image_rules.len()
    }
}

fn compile(globs: &[String]) -> Result<Vec<(String, Pattern)>, InvalidPattern> {
    globs
        .iter()
        .map(|source| {
            Pattern::new(source)
                .map(|pattern| (source.clone(), pattern))
                .map_err(|error| InvalidPattern { pattern: source.clone(), reason: error.msg.to_owned() })
        })
        .collect()
}

//! Modal dialogs and the actions they guard.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::domain::cleaning::{CandidateId, CleaningPlan};
use crate::domain::deletion::{DeletionItem, DeletionPlan};

#[derive(Clone, Debug)]
pub enum PendingAction {
    Delete(DeletionPlan),
    Clean(CleaningPlan),
    ElevateDelete(Vec<DeletionItem>),
    ElevateClean {
        whole: Vec<PathBuf>,
        contents: Vec<PathBuf>,
        candidates: Vec<CandidateId>,
    },
    /// Open the configuration file in `$EDITOR` with the interface suspended.
    EditConfigFile,
    /// Write the preferences and leave.
    SaveAndQuit,
    /// Go back to the default preferences (in memory; `s` persists them).
    ResetSettings,
    Quit,
}

#[derive(Clone, Debug)]
pub enum Confirmation {
    /// Any of `y`/`Enter` confirms.
    YesKey,
    /// The user has to type a word and press Enter.
    TypedWord { expected: String, typed: String },
    /// `y` runs the action, `n` quits without saving, Esc stays.
    SaveOrDiscard,
}

#[derive(Clone, Debug)]
pub struct ConfirmDialog {
    pub title: String,
    pub lines: Vec<String>,
    pub confirmation: Confirmation,
    pub action: PendingAction,
    pub dangerous: bool,
}

impl ConfirmDialog {
    pub fn typed_word_matches(&self) -> bool {
        match &self.confirmation {
            Confirmation::YesKey | Confirmation::SaveOrDiscard => true,
            Confirmation::TypedWord { expected, typed } => typed.trim().eq_ignore_ascii_case(expected),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Debug)]
pub struct MessageDialog {
    pub title: String,
    pub lines: Vec<String>,
    pub severity: Severity,
}

#[derive(Clone, Debug)]
pub enum Overlay {
    Help,
    Log,
    /// The preferences popup (`:settings`). Keeps focus until Esc.
    Settings,
    Confirm(ConfirmDialog),
    Message(MessageDialog),
}

#[derive(Clone, Debug)]
pub struct StatusMessage {
    pub text: String,
    pub severity: Severity,
    pub until: Instant,
}

impl StatusMessage {
    pub fn new(text: impl Into<String>, severity: Severity) -> Self {
        Self { text: text.into(), severity, until: Instant::now() + Duration::from_secs(6) }
    }

    pub fn is_expired(&self) -> bool {
        Instant::now() >= self.until
    }
}

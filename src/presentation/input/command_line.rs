//! The `:` command line for developer commands, vim style.

use std::path::PathBuf;

/// Everything a user can type after `:`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeveloperCommand {
    Quit,
    Help,
    ShowLog,
    ShowStatistics,
    ShowConfigPath,
    SaveConfig,
    GoTo(PathBuf),
    Rescan,
    ExpandDense,
    SetRowLimit(usize),
    SetWorkerThreads(usize),
    SetDenseThreshold(usize),
    SetMaterializeDepth(u16),
    PauseBackground,
    ResumeBackground,
    SetSizeMode(String),
    SetSizeBase(String),
    HeaviestFiles(Option<usize>),
    Reveal,
    ListMounts,
    SetDeletionMode(String),
    Filter(Option<String>),
    Export(PathBuf),
    Cleaner,
    Dashboard,
    Explorer,
    ClearMarks,
    Unknown(String),
}

#[derive(Clone, Debug, Default)]
pub struct CommandLineState {
    pub input: String,
    pub cursor: usize,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
}

impl CommandLineState {
    pub fn open(&mut self) {
        self.input.clear();
        self.cursor = 0;
        self.history_index = None;
    }

    pub fn insert(&mut self, character: char) {
        let byte_index = self.byte_index();
        self.input.insert(byte_index, character);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return self.input.is_empty();
        }
        self.cursor -= 1;
        let byte_index = self.byte_index();
        self.input.remove(byte_index);
        false
    }

    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.input.chars().count());
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.input.chars().count();
    }

    pub fn history_previous(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let index = match self.history_index {
            None => self.history.len() - 1,
            Some(0) => 0,
            Some(index) => index - 1,
        };
        self.history_index = Some(index);
        self.input = self.history[index].clone();
        self.move_end();
    }

    pub fn history_next(&mut self) {
        let Some(index) = self.history_index else {
            return;
        };
        if index + 1 >= self.history.len() {
            self.history_index = None;
            self.input.clear();
            self.cursor = 0;
        } else {
            self.history_index = Some(index + 1);
            self.input = self.history[index + 1].clone();
            self.move_end();
        }
    }

    /// Parses and records the current input.
    pub fn submit(&mut self) -> Option<DeveloperCommand> {
        let text = self.input.trim().to_owned();
        if text.is_empty() {
            return None;
        }
        if self.history.last() != Some(&text) {
            self.history.push(text.clone());
        }
        self.history_index = None;
        Some(parse_command(&text))
    }

    fn byte_index(&self) -> usize {
        self.input.char_indices().nth(self.cursor).map(|(index, _)| index).unwrap_or(self.input.len())
    }
}

pub fn parse_command(text: &str) -> DeveloperCommand {
    let text = text.trim().trim_start_matches(':');
    let mut parts = text.splitn(2, char::is_whitespace);
    let name = parts.next().unwrap_or("").to_ascii_lowercase();
    let argument = parts.next().map(str::trim).filter(|value| !value.is_empty());

    let number = |value: Option<&str>| value.and_then(|value| value.replace('_', "").parse::<usize>().ok());

    match name.as_str() {
        "q" | "quit" | "exit" => DeveloperCommand::Quit,
        "h" | "help" | "?" => DeveloperCommand::Help,
        "log" | "logs" => DeveloperCommand::ShowLog,
        "stats" | "statistics" | "status" => DeveloperCommand::ShowStatistics,
        "config" => match argument {
            Some("save") => DeveloperCommand::SaveConfig,
            _ => DeveloperCommand::ShowConfigPath,
        },
        "goto" | "cd" | "go" | "scan" | "open" => match argument {
            Some(path) => DeveloperCommand::GoTo(PathBuf::from(path)),
            None => DeveloperCommand::Unknown(format!("{name} needs a path")),
        },
        "rescan" | "refresh" => DeveloperCommand::Rescan,
        "expand" | "force" => DeveloperCommand::ExpandDense,
        "limit" | "rows" => match number(argument) {
            Some(value) if value > 0 => DeveloperCommand::SetRowLimit(value),
            _ => DeveloperCommand::Unknown("limit needs a positive number".to_owned()),
        },
        "threads" | "workers" => match number(argument) {
            Some(value) if value > 0 => DeveloperCommand::SetWorkerThreads(value),
            _ => DeveloperCommand::Unknown("threads needs a positive number".to_owned()),
        },
        "dense" => match number(argument) {
            Some(value) if value >= 100 => DeveloperCommand::SetDenseThreshold(value),
            _ => DeveloperCommand::Unknown("dense needs a number of at least 100".to_owned()),
        },
        "depth" | "materialize" => match number(argument) {
            Some(value) if (1..=64).contains(&value) => DeveloperCommand::SetMaterializeDepth(value as u16),
            _ => DeveloperCommand::Unknown("depth needs a number between 1 and 64".to_owned()),
        },
        "pause" => DeveloperCommand::PauseBackground,
        "resume" | "unpause" => DeveloperCommand::ResumeBackground,
        "mode" | "size" => match argument {
            Some(value) => DeveloperCommand::SetSizeMode(value.to_ascii_lowercase()),
            None => DeveloperCommand::Unknown("mode needs `allocated` or `apparent`".to_owned()),
        },
        "base" | "units" => match argument {
            Some(value) => DeveloperCommand::SetSizeBase(value.to_ascii_lowercase()),
            None => DeveloperCommand::Unknown("base needs `decimal` or `binary`".to_owned()),
        },
        "top" | "heaviest" | "largest" => DeveloperCommand::HeaviestFiles(number(argument)),
        "reveal" | "finder" => DeveloperCommand::Reveal,
        "mounts" | "volumes" => DeveloperCommand::ListMounts,
        "trash" | "delete" | "deletion" => match argument {
            Some(value) => DeveloperCommand::SetDeletionMode(value.to_ascii_lowercase()),
            None => DeveloperCommand::Unknown("trash needs `on` or `off`".to_owned()),
        },
        "filter" | "find" | "grep" => DeveloperCommand::Filter(argument.map(str::to_owned)),
        "export" | "dump" => match argument {
            Some(path) => DeveloperCommand::Export(PathBuf::from(path)),
            None => DeveloperCommand::Unknown("export needs a file path".to_owned()),
        },
        "clean" | "cleaner" => DeveloperCommand::Cleaner,
        "dash" | "dashboard" | "home" => DeveloperCommand::Dashboard,
        "explore" | "explorer" | "tree" => DeveloperCommand::Explorer,
        "unmark" | "clear" => DeveloperCommand::ClearMarks,
        other => DeveloperCommand::Unknown(format!("unknown command `{other}`")),
    }
}

/// One line per command, for the help overlay.
pub const COMMAND_REFERENCE: &[(&str, &str)] = &[
    (":goto <path>", "open a directory (adds a root if needed)"),
    (":rescan", "measure the current directory again"),
    (":expand", "force-expand a dense directory"),
    (":limit <n>", "rows shown per directory"),
    (":top [n]", "heaviest files below the current directory"),
    (":filter <text>", "show only rows containing text (:filter clears)"),
    (":threads <n>", "scanner threads"),
    (":dense <n>", "entries above which a directory is dense"),
    (":depth <n>", "levels materialised below the scan origin"),
    (":pause / :resume", "background scanning"),
    (":mode allocated|apparent", "which size to show"),
    (":base decimal|binary", "GB or GiB"),
    (":trash on|off", "delete through the Trash or permanently"),
    (":export <file>", "write the current rows as TSV"),
    (":reveal", "show in Finder / file manager"),
    (":mounts", "list mount points in the log"),
    (":stats", "scanner statistics"),
    (":log", "show the log"),
    (":config [save]", "config file path / write current settings"),
    (":q", "quit"),
];

//! Running external programs and revealing paths in the graphical file manager.

use std::path::Path;
use std::process::Command;

use crate::domain::ports::{CommandFailure, CommandOutput, CommandRunner, FileRevealer};
use crate::infrastructure::privilege::find_in_path;

#[derive(Debug, Default)]
pub struct StdCommandRunner;

impl CommandRunner for StdCommandRunner {
    fn is_installed(&self, program: &str) -> bool {
        find_in_path(program).is_some()
    }

    fn run(&self, program: &str, arguments: &[String]) -> Result<CommandOutput, CommandFailure> {
        let output = Command::new(program)
            .args(arguments)
            .output()
            .map_err(|error| CommandFailure { program: program.to_owned(), detail: error.to_string() })?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if output.status.success() {
            Ok(CommandOutput { stdout, stderr })
        } else {
            Err(CommandFailure {
                program: program.to_owned(),
                detail: stderr
                    .lines()
                    .map(str::trim)
                    .find(|line| !line.is_empty())
                    .unwrap_or("non-zero exit status")
                    .to_owned(),
            })
        }
    }
}

#[derive(Debug, Default)]
pub struct SystemFileRevealer;

impl FileRevealer for SystemFileRevealer {
    fn reveal(&self, path: &Path) -> Result<(), CommandFailure> {
        let (program, arguments): (&str, Vec<&Path>) = if cfg!(target_os = "macos") {
            ("open", vec![Path::new("-R"), path])
        } else {
            let target = if path.is_dir() { path } else { path.parent().unwrap_or(path) };
            ("xdg-open", vec![target])
        };
        let status = Command::new(program)
            .args(arguments)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map_err(|error| CommandFailure { program: program.to_owned(), detail: error.to_string() })?;
        if status.success() {
            Ok(())
        } else {
            Err(CommandFailure {
                program: program.to_owned(),
                detail: format!("exit status {}", status.code().unwrap_or(-1)),
            })
        }
    }
}

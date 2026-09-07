//! Removes paths through `sudo`. The password prompt is sudo's own, on the real terminal.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::domain::ports::{PrivilegeError, PrivilegeEscalator};

#[derive(Debug)]
pub struct SudoEscalator {
    sudo: Option<PathBuf>,
}

impl SudoEscalator {
    pub fn detect() -> Self {
        Self { sudo: find_in_path("sudo") }
    }

    fn run(&self, arguments: &[&str], paths: &[PathBuf], banner: &str) -> Result<(), PrivilegeError> {
        let sudo = self
            .sudo
            .as_ref()
            .ok_or_else(|| PrivilegeError::Unavailable("sudo is not installed".to_owned()))?;
        println!();
        println!("  tree-cleaner needs administrator privileges to {banner}:");
        for path in paths {
            println!("    {}", path.display());
        }
        println!();
        let status = Command::new(sudo)
            .arg("-p")
            .arg("  [sudo] password for %u: ")
            .args(arguments)
            .args(paths)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .map_err(|error| PrivilegeError::Failed(error.to_string()))?;
        match status.code() {
            Some(0) => Ok(()),
            Some(1) => Err(PrivilegeError::Denied),
            Some(code) => Err(PrivilegeError::Failed(format!("exit status {code}"))),
            None => Err(PrivilegeError::Failed("terminated by a signal".to_owned())),
        }
    }
}

impl PrivilegeEscalator for SudoEscalator {
    fn is_available(&self) -> bool {
        self.sudo.is_some()
    }

    fn describe(&self) -> String {
        "sudo".to_owned()
    }

    fn remove_paths(&self, paths: &[PathBuf]) -> Result<(), PrivilegeError> {
        if paths.is_empty() {
            return Ok(());
        }
        self.run(&["rm", "-rf", "--"], paths, "delete")
    }

    fn remove_directory_contents(&self, directories: &[PathBuf]) -> Result<(), PrivilegeError> {
        let sudo = self
            .sudo
            .as_ref()
            .ok_or_else(|| PrivilegeError::Unavailable("sudo is not installed".to_owned()))?;
        for directory in directories {
            println!();
            println!("  tree-cleaner needs administrator privileges to empty:");
            println!("    {}/*", directory.display());
            println!();
            let status = Command::new(sudo)
                .arg("-p")
                .arg("  [sudo] password for %u: ")
                .arg("find")
                .arg(directory)
                .args(["-mindepth", "1", "-maxdepth", "1", "-exec", "rm", "-rf", "--", "{}", "+"])
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status()
                .map_err(|error| PrivilegeError::Failed(error.to_string()))?;
            match status.code() {
                Some(0) => {}
                Some(1) => return Err(PrivilegeError::Denied),
                Some(code) => return Err(PrivilegeError::Failed(format!("exit status {code}"))),
                None => return Err(PrivilegeError::Failed("terminated by a signal".to_owned())),
            }
        }
        Ok(())
    }
}

pub fn find_in_path(program: &str) -> Option<PathBuf> {
    let path_variable = std::env::var_os("PATH")?;
    std::env::split_paths(&path_variable)
        .map(|directory| directory.join(program))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

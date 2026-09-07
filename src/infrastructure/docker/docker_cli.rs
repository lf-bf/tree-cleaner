//! Docker through its command line client. The CLI already knows how to reach the daemon
//! (contexts, sockets, remote hosts), so we do not reimplement any of that.

use std::process::Command;

use crate::domain::cleaning::DockerPruneKind;
use crate::domain::ports::{ContainerDiskUsage, ContainerEngine, ContainerEngineError, ContainerImage};
use crate::domain::storage::ByteSize;
use crate::infrastructure::privilege::find_in_path;

#[derive(Debug)]
pub struct DockerCli {
    binary: Option<std::path::PathBuf>,
}

impl DockerCli {
    pub fn detect() -> Self {
        Self { binary: find_in_path("docker") }
    }

    fn run(&self, arguments: &[&str]) -> Result<String, ContainerEngineError> {
        let binary = self
            .binary
            .as_ref()
            .ok_or_else(|| ContainerEngineError::Unavailable("docker is not installed".to_owned()))?;
        let output = Command::new(binary)
            .args(arguments)
            .output()
            .map_err(|error| ContainerEngineError::Unavailable(error.to_string()))?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            let detail = if stderr.is_empty() { stdout } else { stderr };
            Err(ContainerEngineError::CommandFailed(first_meaningful_line(&detail)))
        }
    }
}

fn first_meaningful_line(text: &str) -> String {
    text.lines().map(str::trim).find(|line| !line.is_empty()).unwrap_or("unknown error").to_owned()
}

/// Parses docker's human sizes (`1.23GB`, `456MB`, `12.3kB`, `0B`). Docker uses decimal units.
pub fn parse_docker_size(text: &str) -> ByteSize {
    let text = text.trim();
    let split_at = text.find(|character: char| character.is_ascii_alphabetic()).unwrap_or(text.len());
    let (number, unit) = text.split_at(split_at);
    let value: f64 = number.trim().parse().unwrap_or(0.0);
    let multiplier: f64 = match unit.trim().to_ascii_lowercase().as_str() {
        "b" | "" => 1.0,
        "kb" => 1e3,
        "mb" => 1e6,
        "gb" => 1e9,
        "tb" => 1e12,
        "pb" => 1e15,
        "kib" => 1024.0,
        "mib" => 1024.0_f64.powi(2),
        "gib" => 1024.0_f64.powi(3),
        "tib" => 1024.0_f64.powi(4),
        _ => 1.0,
    };
    ByteSize::new((value * multiplier).round().max(0.0) as u64)
}

fn short_image_id(raw: &str) -> String {
    let hash = raw.trim().strip_prefix("sha256:").unwrap_or(raw.trim());
    hash.chars().take(12).collect()
}

impl ContainerEngine for DockerCli {
    fn availability(&self) -> Result<String, ContainerEngineError> {
        self.run(&["version", "--format", "{{.Server.Version}}"])
            .map(|version| version.trim().to_owned())
            .map_err(|error| match error {
                ContainerEngineError::CommandFailed(detail) => ContainerEngineError::Unavailable(detail),
                other => other,
            })
    }

    fn list_images(&self) -> Result<Vec<ContainerImage>, ContainerEngineError> {
        let output = self.run(&[
            "image",
            "ls",
            "--all",
            "--no-trunc",
            "--format",
            "{{.ID}}\t{{.Repository}}\t{{.Tag}}\t{{.Size}}\t{{.CreatedSince}}",
        ])?;
        let mut images: Vec<ContainerImage> = output
            .lines()
            .filter_map(|line| {
                let mut fields = line.split('\t');
                let id = short_image_id(fields.next()?);
                let repository = fields.next()?.trim().to_owned();
                let tag = fields.next()?.trim().to_owned();
                let size = parse_docker_size(fields.next()?);
                let created = fields.next().unwrap_or("").trim().to_owned();
                Some(ContainerImage { id, repository, tag, size, created })
            })
            .collect();
        images.sort_by_key(|image| std::cmp::Reverse(image.size));
        Ok(images)
    }

    fn image_ids_in_use(&self) -> Result<Vec<String>, ContainerEngineError> {
        let containers = self.run(&["container", "ls", "--all", "--quiet"])?;
        let ids: Vec<&str> = containers.lines().map(str::trim).filter(|line| !line.is_empty()).collect();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut arguments = vec!["container", "inspect", "--format", "{{.Image}}"];
        arguments.extend(ids);
        let output = self.run(&arguments)?;
        Ok(output.lines().map(short_image_id).filter(|id| !id.is_empty()).collect())
    }

    fn disk_usage(&self) -> Result<ContainerDiskUsage, ContainerEngineError> {
        let output = self.run(&["system", "df", "--format", "{{.Type}}\t{{.Size}}\t{{.Reclaimable}}"])?;
        let mut usage = ContainerDiskUsage::default();
        for line in output.lines() {
            let mut fields = line.split('\t');
            let kind = fields.next().unwrap_or("").trim().to_ascii_lowercase();
            let size = parse_docker_size(fields.next().unwrap_or("0B"));
            let reclaimable_text = fields.next().unwrap_or("0B");
            let reclaimable = parse_docker_size(reclaimable_text.split_whitespace().next().unwrap_or("0B"));
            match kind.as_str() {
                "images" => {
                    usage.images = size;
                    usage.images_reclaimable = reclaimable;
                }
                "containers" => {
                    usage.containers = size;
                    usage.containers_reclaimable = reclaimable;
                }
                "local volumes" => {
                    usage.volumes = size;
                    usage.volumes_reclaimable = reclaimable;
                }
                "build cache" => {
                    usage.build_cache = size;
                    usage.build_cache_reclaimable = reclaimable;
                }
                _ => {}
            }
        }
        Ok(usage)
    }

    fn remove_images(&self, ids: &[String]) -> Result<String, ContainerEngineError> {
        if ids.is_empty() {
            return Ok(String::new());
        }
        let mut arguments = vec!["image", "rm"];
        arguments.extend(ids.iter().map(String::as_str));
        self.run(&arguments)
    }

    fn prune(&self, kind: DockerPruneKind) -> Result<String, ContainerEngineError> {
        match kind {
            DockerPruneKind::StoppedContainers => self.run(&["container", "prune", "--force"]),
            DockerPruneKind::DanglingVolumes => self.run(&["volume", "prune", "--force"]),
            DockerPruneKind::BuildCache => self.run(&["builder", "prune", "--all", "--force"]),
        }
    }
}

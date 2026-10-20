//! The two modes. custodian walks up from where it runs for an initialized `.custodian/` folder: found, the run is
//! journaled (a receipt, the journal, the cache, dated readings, the queue); not found, it is free mode (the answer
//! printed, nothing stored), and the run says so.

use crate::codes::{Code, Result};
use crate::layout::{self, Layout};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Plain: specs, readings, a journal. Agent: the same, and jobs as folders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FolderMode {
    Plain,
    Agent,
}

/// `.custodian/config/custodian.toml`, in the form custodian_kit's store reads it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub mode: FolderMode,
    /// Globs (relative to the root) no reading reads, on top of a spec's own.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// A job's mode when its job file names none (custodian_kit's `store::Settings::job_mode`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_mode: Option<custodian_kit::jobs::JobMode>,
}

/// An initialized project.
#[derive(Debug, Clone)]
pub struct Project {
    pub layout: Layout,
    pub settings: Settings,
}

impl Project {
    pub fn root(&self) -> &Path {
        &self.layout.root
    }

    pub fn agent(&self) -> bool {
        self.settings.mode == FolderMode::Agent
    }

    /// Jobs are agent mode's; a plain folder refuses them.
    pub fn need_agent(&self) -> Result<()> {
        if self.agent() {
            Ok(())
        } else {
            Err(Code::NeedsAgent.refuse(format!("{} was made by `custodian init`, a person's folder; run exactly `custodian agent init` here to make it an agent's", self.layout.rel(&self.layout.folder()))))
        }
    }
}

/// Where a run is.
#[derive(Debug, Clone)]
pub enum Mode {
    /// An initialized folder at or above the start.
    Journaled(Project),
    /// No folder: the workspace root the run reads, nothing stored.
    Free { root: PathBuf },
}

impl Mode {
    /// The mode at `start`: the nearest `.custodian/` with its settings at or above it, else free mode over the
    /// workspace the start is in.
    pub fn find(start: &Path) -> Result<Mode> {
        let start = start.canonicalize().map_err(|e| Code::NoWorkspace.refuse(format!("{}: {e}", start.display())))?;
        for dir in start.ancestors() {
            if !dir.join(layout::FOLDER).is_dir() {
                continue;
            }
            let layout = Layout::at(dir)?;
            let file = layout.path::<layout::Settings>(&[])?;
            if let Ok(text) = std::fs::read_to_string(&file) {
                let settings: Settings = toml::from_str(&text).map_err(|e| Code::NeedsFolder.refuse(format!("{}: {e}", file.display())))?;
                return Ok(Mode::Journaled(Project { layout, settings }));
            }
        }
        Ok(Mode::Free { root: workspace_root(&start) })
    }

    pub fn root(&self) -> &Path {
        match self {
            Mode::Journaled(p) => p.root(),
            Mode::Free { root } => root,
        }
    }

    pub fn project(&self) -> Option<&Project> {
        match self {
            Mode::Journaled(p) => Some(p),
            Mode::Free { .. } => None,
        }
    }

    /// The project, or the refusal that this needs one.
    pub fn need_project(&self) -> Result<&Project> {
        self.project().ok_or_else(|| Code::NeedsFolder.refuse(format!("{} has none", self.root().display())))
    }

    /// An agent's project, or the one refusal that names the one line making it: `custodian agent init` makes the
    /// folder and makes it an agent's, so a folder with no `.custodian` is told that line, not `custodian init` first.
    pub fn need_agent_project(&self) -> Result<&Project> {
        match self.project() {
            Some(p) => p.need_agent().map(|_| p),
            None => Err(Code::NeedsAgent.refuse(format!("{} has no .custodian folder; run exactly `custodian agent init` here: it makes the folder an agent's", self.root().display()))),
        }
    }
}

/// The workspace a directory is in: the nearest ancestor whose Cargo.toml declares `[workspace]`, else the nearest
/// Cargo.toml, else the directory.
pub fn workspace_root(start: &Path) -> PathBuf {
    let mut nearest = None;
    for dir in start.ancestors() {
        let manifest = dir.join("Cargo.toml");
        if let Ok(text) = std::fs::read_to_string(&manifest) {
            if text.lines().any(|l| l.trim() == "[workspace]") {
                return dir.to_path_buf();
            }
            nearest.get_or_insert_with(|| dir.to_path_buf());
        }
    }
    nearest.unwrap_or_else(|| start.to_path_buf())
}

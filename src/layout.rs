//! The `.custodian/` folder. Its parts are custodian_kit's (`store::named`), declared once there; the command line
//! names them as types ([`Part`]) that hand back the kit's descriptors. The few parts only the command line keeps
//! (a reading's record, a check's JSON, a log read from stdin) are declared here, each a pathgen-macros
//! `ArtifactContract` placed by one descriptor in [`TABLE`]. Every path is resolved through pathgen's dev policy
//! whose base is the project root and whose app is `.custodian`: `<root>/.custodian/<role>/<template>`.

use crate::codes::{Code, Result};
use pathgen::{
    AppPathRole, ArtifactEndpoint, ArtifactOpDescriptor, ArtifactOpRegistry, ArtifactOutputDescriptor, PathMutability, PathOwner,
    PathPolicy, PathSensitivity, PathTemplateVars,
};
use pathgen_macros::ArtifactContract;
use std::path::{Path, PathBuf};

/// The folder's name, and pathgen's app name for it.
pub const FOLDER: &str = ".custodian";

/// One part of the folder: where it goes, as a pathgen output descriptor.
pub trait Part {
    fn descriptor() -> ArtifactOutputDescriptor;
}

macro_rules! kit_parts {
    ($($(#[$doc:meta])* $ty:ident => $named:ident,)*) => {
        $(
            $(#[$doc])*
            #[derive(Debug, Clone, Copy)]
            pub struct $ty;
            impl Part for $ty {
                fn descriptor() -> ArtifactOutputDescriptor {
                    custodian_kit::store::named::$named
                }
            }
        )*
    };
}

kit_parts! {
    /// `config/custodian.toml`: the mode (plain or agent) and the globs no reading reads.
    Settings => SETTINGS,
    /// `config/v1.paths.1o.paths`: the declared routes, in 1o.
    PathsTable => PATHS,
    /// `config/schema/{modifier}/v1.schema.json`: the 1o schemas.
    Schema => SCHEMA,
    /// `config/{name}.toml`: the features rule (`features`), the annotate rule (`annotate`).
    ConfigFile => THRESHOLDS,
    /// `config/jobs/{kind}.toml`: a job template.
    JobTemplate => JOB_TEMPLATE,
    /// `data/specs/queued/{name}.toml`: a spec waiting to run.
    QueuedSpec => QUEUED_SPEC,
    /// `data/specs/done/{name}.toml`: a spec that ran.
    DoneSpec => DONE_SPEC,
    /// `data/readings/{name}/{stamp}.md`: a reading's page, dated, every earlier one kept.
    ReadingPage => READING_PAGE,
    /// `data/receipts/{seq}.json`: one run's receipt.
    ReceiptFile => RECEIPT,
    /// `data/inventory.md`: the inventory.
    Inventory => INVENTORY,
    /// `data/plans/{name}.toml`: a plan custodian wrote for another program.
    PlanFile => PLAN,
    /// `data/history/v1.history_index.1o.diff`: the task histories' index (a task per job, a revision per run).
    History => HISTORY_INDEX,
    /// `state/journal.sqlite`: the journal.
    Journal => JOURNAL,
    /// `state/jobs/{state}/{job}/v1.job.1o.job`: a job's folder and its job file.
    JobFolder => JOB_FILE,
    /// `runtime/serve.sock`: the resident's owner-only socket.
    Socket => SOCKET,
}

macro_rules! parts {
    ($($(#[$doc:meta])* $ty:ident { schema: $schema:tt, route: $route:tt, kind: $kind:tt, canonical: $canon:tt, role: $role:ident, template: $tmpl:tt, mutability: $mu:ident })*) => {
        $(
            $(#[$doc])*
            #[derive(ArtifactContract, Debug, Clone, Copy)]
            #[artifact(schema = $schema, route = $route, kind = $kind, version = 1, canonical = $canon, core = pathgen)]
            pub struct $ty;
            impl Part for $ty {
                fn descriptor() -> ArtifactOutputDescriptor {
                    output::<$ty>(AppPathRole::$role, $tmpl, PathMutability::$mu).output
                }
            }
        )*

        /// The parts only the command line keeps, one descriptor each.
        pub static TABLE: &[ArtifactOpDescriptor] = &[
            $(output::<$ty>(AppPathRole::$role, $tmpl, PathMutability::$mu),)*
        ];
    };
}

parts! {
    /// `data/readings/{name}/{stamp}.json`: a reading's record, beside its page.
    ReadingRecord { schema: "custodian.reading.v1", route: "custodian.reading.record", kind: "reading", canonical: "file::json", role: Data, template: "readings/{name}/{stamp}.json", mutability: AppendOnly }
    /// `data/checks/{stamp}.json`: what a maintenance check line printed.
    CheckRunFile { schema: "maintenance.check_run.v1", route: "custodian.check", kind: "check", canonical: "file::json", role: Data, template: "checks/{stamp}.json", mutability: AppendOnly }
    /// `data/logs/{name}.txt`: a log read from stdin for `diagnostics from -`.
    LogFile { schema: "custodian.log.v1", route: "custodian.log", kind: "log", canonical: "file::txt", role: Data, template: "logs/{name}.txt", mutability: AppendOnly }
}

/// One part's descriptor from its contract and its place.
const fn output<A: pathgen::ArtifactContract>(base_role: AppPathRole, relative_template: &'static str, mutability: PathMutability) -> ArtifactOpDescriptor {
    ArtifactOpDescriptor::new(ArtifactOutputDescriptor {
        route_id: A::ROUTE_ID,
        schema: A::SCHEMA,
        kind: A::KIND,
        version: A::VERSION,
        endpoint: A::CANONICAL,
        base_role,
        relative_template,
        sensitivity: PathSensitivity::Internal,
        mutability,
        owner: PathOwner::CurrentUser,
    })
}

/// The table as pathgen's registry: one binding per route and endpoint.
pub fn registry() -> ArtifactOpRegistry {
    ArtifactOpRegistry::new(TABLE)
}

/// A project's `.custodian/` folder: its root and the policy that places the parts.
#[derive(Debug, Clone)]
pub struct Layout {
    pub root: PathBuf,
    policy: PathPolicy,
}

impl Layout {
    /// The layout of the project at `root` (the folder need not exist yet).
    pub fn at(root: &Path) -> Result<Layout> {
        let policy = PathPolicy::dev(FOLDER).map_err(|e| Code::Write.refuse(e))?.with_dev_base(root);
        Ok(Layout { root: root.to_path_buf(), policy })
    }

    /// `<root>/.custodian`.
    pub fn folder(&self) -> PathBuf {
        self.root.join(FOLDER)
    }

    /// The path of part `A`, its template filled from `vars`.
    pub fn path<A: Part>(&self, vars: &[(&str, &str)]) -> Result<PathBuf> {
        let d = A::descriptor();
        let mut v = PathTemplateVars::new();
        for (k, val) in vars {
            v = v.with(k, val);
        }
        self.policy.artifact_output(&d, &v).map(|r| r.path).map_err(|e| Code::Write.refuse(e))
    }

    /// The directory part `A` sits in.
    pub fn dir<A: Part>(&self, vars: &[(&str, &str)]) -> Result<PathBuf> {
        Ok(self.path::<A>(vars)?.parent().map(Path::to_path_buf).unwrap_or_default())
    }

    /// `state/jobs/<state>/`.
    pub fn jobs_dir(&self, state: &str) -> Result<PathBuf> {
        Ok(self.dir::<JobFolder>(&[("state", state), ("job", "x")])?.parent().map(Path::to_path_buf).unwrap_or_default())
    }

    /// `state/jobs/<state>/<job>/`.
    pub fn job_dir(&self, state: &str, job: &str) -> Result<PathBuf> {
        self.dir::<JobFolder>(&[("state", state), ("job", job)])
    }

    /// `data/readings/`.
    pub fn readings_dir(&self) -> Result<PathBuf> {
        Ok(self.dir::<ReadingPage>(&[("name", "x"), ("stamp", "x")])?.parent().map(Path::to_path_buf).unwrap_or_default())
    }

    /// `p` relative to the root, for a line a person reads.
    pub fn rel(&self, p: &Path) -> String {
        p.strip_prefix(&self.root).unwrap_or(p).to_string_lossy().into_owned()
    }

    /// Every part as `route  path`, for `custodian list`: the kit's, then the command line's own.
    pub fn parts(&self) -> Vec<(String, String)> {
        use custodian_kit::store::named::*;
        let kit = [SETTINGS, PATHS, SCHEMA, THRESHOLDS, JOB_TEMPLATE, QUEUED_SPEC, DONE_SPEC, READING_PAGE, RECEIPT, INVENTORY, PLAN, HISTORY_INDEX, JOURNAL, JOB_FILE, SOCKET];
        kit.iter().chain(TABLE.iter().map(|d| &d.output)).map(|d| (d.route_id.to_string(), format!("{FOLDER}/{}/{}", d.base_role.label(), d.relative_template))).collect()
    }
}

/// Writes `bytes` to `path` whole: a sibling temporary file, then a rename, so a reader never sees half a file.
pub fn write_whole(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| Code::Write.refuse(format!("{}: {e}", dir.display())))?;
    }
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tmp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, bytes).map_err(|e| Code::Write.refuse(format!("{}: {e}", tmp.display())))?;
    std::fs::rename(&tmp, path).map_err(|e| Code::Write.refuse(format!("{}: {e}", path.display())))
}

/// The endpoint names the table's file parts use, for the README's table.
pub fn endpoint_label(e: ArtifactEndpoint) -> &'static str {
    e.label()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_part_resolves_under_the_folder_and_the_kit_names_its_own() {
        registry().validate_unique().unwrap();
        let l = Layout::at(Path::new("/p")).unwrap();
        assert_eq!(l.path::<Journal>(&[]).unwrap(), Path::new("/p/.custodian/state/journal.sqlite"));
        assert_eq!(l.path::<QueuedSpec>(&[("name", "env")]).unwrap(), Path::new("/p/.custodian/data/specs/queued/env.toml"));
        assert_eq!(l.path::<JobFolder>(&[("state", "done"), ("job", "j")]).unwrap(), Path::new("/p/.custodian/state/jobs/done/j/v1.job.1o.job"));
        assert_eq!(l.path::<PathsTable>(&[]).unwrap(), Path::new("/p/.custodian/config/v1.paths.1o.paths"));
        assert_eq!(l.path::<ReadingRecord>(&[("name", "env"), ("stamp", "s")]).unwrap(), Path::new("/p/.custodian/data/readings/env/s.json"));
        assert_eq!(l.path::<CheckRunFile>(&[("stamp", "s")]).unwrap(), Path::new("/p/.custodian/data/checks/s.json"));
        assert_eq!(l.jobs_dir("running").unwrap(), Path::new("/p/.custodian/state/jobs/running"));
        assert!(l.path::<QueuedSpec>(&[("name", "../x")]).is_err(), "a template value cannot leave the folder");
        // the kit's own parts are the kit's descriptors, not copies
        assert_eq!(<JobFolder as Part>::descriptor(), custodian_kit::store::named::JOB_FILE);
        assert_eq!(<Socket as Part>::descriptor(), custodian_kit::store::named::SOCKET);
    }
}

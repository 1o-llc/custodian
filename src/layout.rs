//! The `.custodian/` folder, declared once: each part a type whose `#[artifact(..)]` says its schema, route and
//! kind (pathgen-macros), each placed by one descriptor in [`TABLE`] (its role directory and its template), and
//! every path custodian writes resolved through pathgen's dev policy whose base is the project root and whose app is
//! `.custodian`, so a part's path is `<root>/.custodian/<role>/<template>`. Nothing in custodian writes a path that
//! is not in this table.
//!
//! The templates are the ones custodian_kit's store names, so the kit's reading of a project (its `paths.toml`, its
//! `custodian:` routes) and the command line's agree; a test holds the two equal.

use crate::codes::{Code, Result};
use pathgen::{
    AppPathRole, ArtifactEndpoint, ArtifactOpDescriptor, ArtifactOpRegistry, ArtifactOutputDescriptor, PathMutability, PathOwner,
    PathPolicy, PathSensitivity, PathTemplateVars,
};
use pathgen_macros::ArtifactContract;
use std::path::{Path, PathBuf};

/// The folder's name, and pathgen's app name for it.
pub const FOLDER: &str = ".custodian";

macro_rules! parts {
    ($($(#[$doc:meta])* $ty:ident { schema: $schema:tt, route: $route:tt, kind: $kind:tt, canonical: $canon:tt, role: $role:ident, template: $tmpl:literal, mutability: $mu:ident })*) => {
        $(
            $(#[$doc])*
            #[derive(ArtifactContract, Debug, Clone, Copy)]
            #[artifact(schema = $schema, route = $route, kind = $kind, version = 1, canonical = $canon, core = pathgen)]
            pub struct $ty;
        )*

        /// Every part's place, one descriptor each.
        pub static TABLE: &[ArtifactOpDescriptor] = &[
            $(output::<$ty>(AppPathRole::$role, $tmpl, PathMutability::$mu),)*
        ];
    };
}

parts! {
    /// `config/custodian.toml`: the mode (plain or agent) and the globs no reading reads.
    Settings { schema: "custodian.settings.v1", route: "custodian.settings", kind: "settings", canonical: "file::toml", role: Config, template: "custodian.toml", mutability: Writable }
    /// `config/v1.paths.1o.paths`: the declared routes, in 1o (custodian_kit's paths table; the older
    /// `config/paths.toml` is still read).
    PathsTable { schema: "custodian.paths.v1", route: "custodian.paths", kind: "paths", canonical: "file::txt", role: Config, template: "v1.paths.1o.paths", mutability: Writable }
    /// `config/schema/{modifier}/v1.schema.json`: the 1o schemas, where the 1o language server finds them.
    Schema { schema: "custodian.schema.v1", route: "custodian.schema", kind: "schema", canonical: "file::json", role: Config, template: "schema/{modifier}/v1.schema.json", mutability: Writable }
    /// `config/{name}.toml`: the features rule (`features`), the annotate rule (`annotate`).
    ConfigFile { schema: "custodian.config.v1", route: "custodian.config", kind: "config", canonical: "file::toml", role: Config, template: "{name}.toml", mutability: Writable }
    /// `config/jobs/{kind}.toml`: a job template, the user's to edit once written.
    JobTemplate { schema: "custodian.job_template.v1", route: "custodian.job_template", kind: "job_template", canonical: "file::toml", role: Config, template: "jobs/{kind}.toml", mutability: Writable }
    /// `data/specs/queued/{name}.toml`: a spec waiting to run.
    QueuedSpec { schema: "custodian.spec.v1", route: "custodian.spec.queued", kind: "spec", canonical: "file::toml", role: Data, template: "specs/queued/{name}.toml", mutability: Writable }
    /// `data/specs/done/{name}.toml`: a spec that ran.
    DoneSpec { schema: "custodian.spec.v1", route: "custodian.spec.done", kind: "spec", canonical: "file::toml", role: Data, template: "specs/done/{name}.toml", mutability: Writable }
    /// `data/readings/{name}/{stamp}.md`: a reading's page, dated, every earlier one kept.
    ReadingPage { schema: "custodian.reading.v1", route: "custodian.reading.page", kind: "reading", canonical: "file::markdown", role: Data, template: "readings/{name}/{stamp}.md", mutability: AppendOnly }
    /// `data/readings/{name}/{stamp}.json`: the reading's record.
    ReadingRecord { schema: "custodian.reading.v1", route: "custodian.reading.record", kind: "reading", canonical: "file::json", role: Data, template: "readings/{name}/{stamp}.json", mutability: AppendOnly }
    /// `data/receipts/{seq}.json`: one run's receipt.
    ReceiptFile { schema: "custodian.receipt.v1", route: "custodian.receipt", kind: "receipt", canonical: "file::json", role: Data, template: "receipts/{seq}.json", mutability: AppendOnly }
    /// `data/inventory.md`: the inventory, rendered from the journal.
    Inventory { schema: "custodian.inventory.v1", route: "custodian.inventory", kind: "inventory", canonical: "file::markdown", role: Data, template: "inventory.md", mutability: Writable }
    /// `data/plans/{name}`: a plan custodian wrote for another program (code_passenger's spec, the annotate plan).
    PlanFile { schema: "custodian.plan.v1", route: "custodian.plan", kind: "plan", canonical: "file::toml", role: Data, template: "plans/{name}", mutability: Writable }
    /// `data/checks/{stamp}.json`: what a maintenance check line printed.
    CheckRunFile { schema: "maintenance.check_run.v1", route: "custodian.check", kind: "check", canonical: "file::json", role: Data, template: "checks/{stamp}.json", mutability: AppendOnly }
    /// `data/bundles/{name}.bundle`: a route or a job as one annotated file.
    BundleFile { schema: "custodian.bundle.v1", route: "custodian.bundle", kind: "bundle", canonical: "file::txt", role: Data, template: "bundles/{name}.bundle", mutability: AppendOnly }
    /// `state/journal.sqlite`: receipts, the readings index, jobs, test results, install receipts, checkpoints.
    Journal { schema: "custodian.journal.v1", route: "custodian.journal", kind: "journal", canonical: "file::txt", role: State, template: "journal.sqlite", mutability: Writable }
    /// `state/jobs/{state}/{job}/v1.job.1o.job`: a job's folder and its job file in 1o (optional; the folder is the
    /// job; an older `job.toml` is still read).
    JobFolder { schema: "custodian.job.v1", route: "custodian.job", kind: "job", canonical: "file::txt", role: State, template: "jobs/{state}/{job}/v1.job.1o.job", mutability: Writable }
    /// `state/jobs/{state}/{job}/closing.{ext}`: a job's closing receipt (`md` the facts page, `json` the record).
    Closing { schema: "custodian.closing.v1", route: "custodian.closing", kind: "closing", canonical: "file::json", role: State, template: "jobs/{state}/{job}/closing.{ext}", mutability: Writable }
    /// `runtime/serve.sock`: the resident's owner-only socket.
    Socket { schema: "custodian.serve.v1", route: "custodian.serve", kind: "socket", canonical: "file::txt", role: Runtime, template: "serve.sock", mutability: Ephemeral }
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
    pub fn path<A: pathgen::ArtifactContract>(&self, vars: &[(&str, &str)]) -> Result<PathBuf> {
        let d = registry().find(A::ROUTE_ID, A::CANONICAL).map_err(|e| Code::Write.refuse(e))?;
        let mut v = PathTemplateVars::new();
        for (k, val) in vars {
            v = v.with(k, val);
        }
        self.policy.artifact_output(&d.output, &v).map(|r| r.path).map_err(|e| Code::Write.refuse(e))
    }

    /// The directory part `A` sits in.
    pub fn dir<A: pathgen::ArtifactContract>(&self, vars: &[(&str, &str)]) -> Result<PathBuf> {
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

    /// Every part as `route  path`, for `custodian list`.
    pub fn parts(&self) -> Vec<(String, String)> {
        TABLE.iter().map(|d| (d.output.route_id.to_string(), format!("{FOLDER}/{}/{}", d.output.base_role.label(), d.output.relative_template))).collect()
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
    fn every_part_is_bound_once_and_resolves_under_the_folder() {
        registry().validate_unique().unwrap();
        let l = Layout::at(Path::new("/p")).unwrap();
        assert_eq!(l.path::<Journal>(&[]).unwrap(), Path::new("/p/.custodian/state/journal.sqlite"));
        assert_eq!(l.path::<QueuedSpec>(&[("name", "env")]).unwrap(), Path::new("/p/.custodian/data/specs/queued/env.toml"));
        assert_eq!(l.path::<ReadingPage>(&[("name", "env"), ("stamp", "20261019T000000Z")]).unwrap(), Path::new("/p/.custodian/data/readings/env/20261019T000000Z.md"));
        assert_eq!(l.jobs_dir("running").unwrap(), Path::new("/p/.custodian/state/jobs/running"));
        assert_eq!(l.path::<Socket>(&[]).unwrap(), Path::new("/p/.custodian/runtime/serve.sock"));
        assert!(l.path::<QueuedSpec>(&[("name", "../x")]).is_err(), "a template value cannot leave the folder");
    }

    #[test]
    fn the_kit_names_the_same_places() {
        use custodian_kit::store::named;
        let root = Path::new("/p");
        let kit = PathPolicy::dev(FOLDER).unwrap().with_dev_base(root);
        let k = |d: &ArtifactOutputDescriptor, vars: &[(&str, &str)]| {
            let mut v = PathTemplateVars::new();
            for (a, b) in vars {
                v = v.with(a, b);
            }
            kit.artifact_output(d, &v).unwrap().path
        };
        let l = Layout::at(root).unwrap();
        assert_eq!(l.path::<Settings>(&[]).unwrap(), k(&named::SETTINGS, &[]));
        assert_eq!(l.path::<PathsTable>(&[]).unwrap(), k(&named::PATHS, &[]));
        assert_eq!(l.path::<Schema>(&[("modifier", "job")]).unwrap(), k(&named::SCHEMA, &[("modifier", "job")]));
        assert_eq!(l.path::<ConfigFile>(&[("name", "features")]).unwrap(), k(&named::THRESHOLDS, &[("name", "features")]));
        assert_eq!(l.path::<JobTemplate>(&[("kind", "triage")]).unwrap(), k(&named::JOB_TEMPLATE, &[("kind", "triage")]));
        assert_eq!(l.path::<QueuedSpec>(&[("name", "a")]).unwrap(), k(&named::QUEUED_SPEC, &[("name", "a")]));
        assert_eq!(l.path::<DoneSpec>(&[("name", "a")]).unwrap(), k(&named::DONE_SPEC, &[("name", "a")]));
        assert_eq!(l.path::<ReadingPage>(&[("name", "a"), ("stamp", "s")]).unwrap(), k(&named::READING_PAGE, &[("name", "a"), ("stamp", "s")]));
        assert_eq!(l.path::<ReceiptFile>(&[("seq", "3")]).unwrap(), k(&named::RECEIPT, &[("seq", "3")]));
        assert_eq!(l.path::<Inventory>(&[]).unwrap(), k(&named::INVENTORY, &[]));
        assert_eq!(l.path::<Journal>(&[]).unwrap(), k(&named::JOURNAL, &[]));
        assert_eq!(l.path::<JobFolder>(&[("state", "done"), ("job", "j")]).unwrap(), k(&named::JOB_FILE, &[("state", "done"), ("job", "j")]));
        assert_eq!(l.path::<Socket>(&[]).unwrap(), k(&named::SOCKET, &[]));
    }
}

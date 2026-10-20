//! The `.custodian/` folder. Its parts are custodian_kit's (`store::named`), declared once there, the command line's
//! own included; the command line names them as types ([`Part`]) that hand back the kit's descriptors, each an
//! `ArtifactContract` read from its descriptor. Every path is resolved through the kit's policy (pathgen's dev policy
//! whose base is the project root and whose app is `.custodian`): `<root>/.custodian/<role>/<template>`.

use crate::codes::{Code, Result};
use custodian_kit::store::named;
use pathgen::{ArtifactEndpoint, ArtifactOpDescriptor, ArtifactOpRegistry, ArtifactOutputDescriptor, PathPolicy};
use std::path::{Path, PathBuf};

/// The folder's name, and pathgen's app name for it.
pub use custodian_kit::store::FOLDER;

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
                    named::$named
                }
            }
            impl pathgen::ArtifactContract for $ty {
                const SCHEMA: &'static str = named::$named.schema;
                const ROUTE_ID: &'static str = named::$named.route_id;
                const KIND: &'static str = named::$named.kind;
                const VERSION: u32 = named::$named.version;
                const CANONICAL: ArtifactEndpoint = named::$named.endpoint;
                const IMPORTS: &'static [ArtifactEndpoint] = &[named::$named.endpoint];
                const EXPORTS: &'static [ArtifactEndpoint] = &[named::$named.endpoint];
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
    /// `config/{name}.toml`: the features rule (`features`), the annotate rule (`annotate`), the old paths table (`paths`).
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
    /// `data/readings/{name}/{stamp}.json`: a reading's record, beside its page.
    ReadingRecord => READING_RECORD,
    /// `data/checks/{stamp}.json`: what a maintenance check line printed.
    CheckRunFile => CHECK_RUN,
    /// `data/logs/{name}.txt`: a log read from stdin for `diagnostics from -`.
    LogFile => LOG,
    /// `state/writer.sqlite`: the one writer's lock.
    WriterLock => WRITER,
    /// `data/specs/changes/v1.{name}.1o.change`: a change spec (`<name>.toml` beside it is its twin).
    ChangeSpec => CHANGE_SPEC,
    /// `data/preflight/{name}/{stamp}.json`: a preflight's record.
    PreflightRecord => PREFLIGHT,
    /// `data/transforms/{name}/{stamp}.json`: a transform's record.
    TransformRecord => TRANSFORM,
    /// `data/linkage/v1.{name}.1o.linkage`: a linkage record.
    Linkage => LINKAGE,
    /// `data/plans/intent/{name}.json`: an intent's plan.
    IntentPlan => INTENT_PLAN,
    /// `data/trees/{point}.json`: a tree snapshot.
    Tree => TREE,
    /// `data/costs/{seq}.json`: a reading's cost, or a reuse's saving, under its receipt's number.
    CostFile => COST,
}

/// The parts the command line alone writes (a reading's record, a check's JSON, a log read from stdin), one
/// descriptor each: the kit's.
pub static TABLE: &[ArtifactOpDescriptor] = &[ArtifactOpDescriptor::new(named::READING_RECORD), ArtifactOpDescriptor::new(named::CHECK_RUN), ArtifactOpDescriptor::new(named::LOG)];

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
        let policy = named::policy(root).map_err(|e| Code::Write.refuse(e))?;
        Ok(Layout { root: root.to_path_buf(), policy })
    }

    /// `<root>/.custodian`.
    pub fn folder(&self) -> PathBuf {
        self.root.join(FOLDER)
    }

    /// The path of part `A`, its template filled from `vars`.
    pub fn path<A: Part>(&self, vars: &[(&str, &str)]) -> Result<PathBuf> {
        named::resolve(&self.policy, &A::descriptor(), vars).map_err(|e| Code::Write.refuse(e))
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

    /// Every part as `route  path`, for `custodian list`: the kit's, every one (`store::named::ALL`).
    pub fn parts(&self) -> Vec<(String, String)> {
        named::ALL.iter().map(|d| (d.route_id.to_string(), format!("{FOLDER}/{}/{}", d.base_role.label(), d.relative_template))).collect()
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

    /// Every part the command line names is the kit's: each type's descriptor and contract, the table's three, and
    /// `custodian list`'s parts, which are the kit's golden list in its order.
    #[test]
    fn every_path_the_command_line_names_is_the_kits() {
        use pathgen::ArtifactContract;
        fn same<A: Part + ArtifactContract>(kit: ArtifactOutputDescriptor) {
            assert_eq!(A::descriptor(), kit, "{}", kit.route_id);
            assert_eq!((A::SCHEMA, A::ROUTE_ID, A::KIND, A::VERSION, A::CANONICAL), (kit.schema, kit.route_id, kit.kind, kit.version, kit.endpoint));
            assert_eq!((A::IMPORTS, A::EXPORTS), (&[kit.endpoint][..], &[kit.endpoint][..]), "as the derive wrote it: the canonical endpoint");
        }
        use custodian_kit::store::named as k;
        same::<Settings>(k::SETTINGS);
        same::<PathsTable>(k::PATHS);
        same::<Schema>(k::SCHEMA);
        same::<ConfigFile>(k::THRESHOLDS);
        same::<JobTemplate>(k::JOB_TEMPLATE);
        same::<QueuedSpec>(k::QUEUED_SPEC);
        same::<DoneSpec>(k::DONE_SPEC);
        same::<ReadingPage>(k::READING_PAGE);
        same::<ReceiptFile>(k::RECEIPT);
        same::<Inventory>(k::INVENTORY);
        same::<PlanFile>(k::PLAN);
        same::<History>(k::HISTORY_INDEX);
        same::<Journal>(k::JOURNAL);
        same::<JobFolder>(k::JOB_FILE);
        same::<Socket>(k::SOCKET);
        same::<ReadingRecord>(k::READING_RECORD);
        same::<CheckRunFile>(k::CHECK_RUN);
        same::<LogFile>(k::LOG);
        same::<WriterLock>(k::WRITER);
        same::<ChangeSpec>(k::CHANGE_SPEC);
        same::<PreflightRecord>(k::PREFLIGHT);
        same::<TransformRecord>(k::TRANSFORM);
        same::<Linkage>(k::LINKAGE);
        same::<IntentPlan>(k::INTENT_PLAN);
        same::<Tree>(k::TREE);
        // the three the command line alone writes kept the schema, route and place they had as its own declarations
        assert_eq!(TABLE.iter().map(|d| (d.output.schema, d.output.route_id, d.output.relative_template)).collect::<Vec<_>>(), [
            ("custodian.reading.v1", "custodian.reading.record", "readings/{name}/{stamp}.json"),
            ("maintenance.check_run.v1", "custodian.check", "checks/{stamp}.json"),
            ("custodian.log.v1", "custodian.log", "logs/{name}.txt"),
        ]);
        let l = Layout::at(Path::new("/p")).unwrap();
        assert_eq!(l.parts().len(), k::ALL.len());
        let policy = k::policy(Path::new("/p")).unwrap();
        assert_eq!(l.path::<TransformRecord>(&[("name", "n"), ("stamp", "s")]).unwrap(), k::resolve(&policy, &k::TRANSFORM, &[("name", "n"), ("stamp", "s")]).unwrap());
        assert_eq!(l.dir::<ChangeSpec>(&[("name", "x")]).unwrap(), Path::new("/p/.custodian/data/specs/changes"));
    }
}

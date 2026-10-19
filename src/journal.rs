//! The journal: one SQLite file, `.custodian/state/journal.sqlite`: custodian_kit's tables (a receipt per run in
//! `runs`, the `jobs`, the `tests` outcomes) and the ones custodian adds: the readings index the same-search check
//! reads, the events (job states, steps, test progress), the test lines, the install receipts and the job queue's
//! checkpoints. The receipts, their sequence (uh-tom-mick's checked counter, brought up to the file's last receipt
//! inside the writing transaction, so two processes never take one number), the receipt files and the rebuild of
//! the `runs` from them are receipt_history's journal; custodian keeps its own tables on that journal's connection.
//!
//! custodian's tables are declared once, in [`SCHEMA`]; the inventory (`custodian list`) is rendered from them.

use crate::codes::{Code, Refusal, Result};
use custodian_kit::api::Receipt;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The tables custodian adds to custodian_kit's journal (`custodian_kit::store::JOURNAL_SCHEMA`: `runs`, the
/// receipts; `jobs`; `tests`, one row per test outcome), which is made first, so the kit and the command line read
/// one file. A `jobs` table made by the kit gains the three columns custodian keeps ([`JOB_COLUMNS`]).
pub const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS readings (
  seq INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  kind TEXT NOT NULL,
  target TEXT NOT NULL,
  spec_digest TEXT NOT NULL,
  index_key TEXT NOT NULL,
  word TEXT NOT NULL,
  stamp TEXT NOT NULL,
  page TEXT NOT NULL,
  record TEXT NOT NULL,
  reading_digest TEXT NOT NULL,
  job TEXT
);
CREATE INDEX IF NOT EXISTS readings_by_key ON readings (spec_digest, index_key);
CREATE TABLE IF NOT EXISTS events (
  n INTEGER PRIMARY KEY AUTOINCREMENT,
  utc TEXT NOT NULL,
  kind TEXT NOT NULL,
  job TEXT,
  subject TEXT NOT NULL,
  state TEXT NOT NULL,
  detail TEXT NOT NULL,
  seq INTEGER
);
CREATE TABLE IF NOT EXISTS test_runs (
  seq INTEGER PRIMARY KEY,
  job TEXT,
  package TEXT NOT NULL,
  target TEXT NOT NULL,
  line TEXT NOT NULL,
  exit_code INTEGER,
  millis INTEGER NOT NULL,
  passed INTEGER NOT NULL,
  failed INTEGER NOT NULL,
  ignored INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS installs (
  seq INTEGER PRIMARY KEY,
  action TEXT NOT NULL,
  bin TEXT NOT NULL,
  destination TEXT NOT NULL,
  sha256 TEXT NOT NULL,
  receipt TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS checkpoints (
  handle INTEGER PRIMARY KEY AUTOINCREMENT,
  job TEXT NOT NULL,
  step INTEGER NOT NULL,
  payload BLOB NOT NULL,
  utc TEXT NOT NULL
);
";

/// The columns custodian keeps on a job besides the kit's.
pub const JOB_COLUMNS: &[(&str, &str)] = &[("origin", "TEXT"), ("opened_seq", "INTEGER"), ("closed_seq", "INTEGER")];

/// One reading as the index keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadingRow {
    pub seq: i64,
    pub name: String,
    pub kind: String,
    pub target: String,
    pub spec_digest: String,
    pub index_key: String,
    /// The option word the reading ran with (`none`, or a bundle's filters).
    pub word: String,
    pub stamp: String,
    pub page: String,
    pub record: String,
    pub reading_digest: String,
    pub job: Option<String>,
}

/// One job as the journal keeps it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobRow {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub state: String,
    pub folder: String,
    pub origin: Option<String>,
    pub opened_seq: Option<i64>,
    pub closed_seq: Option<i64>,
    pub opened_utc: String,
    pub closed_utc: Option<String>,
}

/// One event: a job's state change, a step, a test's progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub n: i64,
    pub utc: String,
    /// `job`, `step`, `test`.
    pub kind: String,
    pub job: Option<String>,
    pub subject: String,
    pub state: String,
    pub detail: String,
    pub seq: Option<i64>,
}

/// One cargo test line's run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestRun {
    pub seq: i64,
    pub job: Option<String>,
    pub package: String,
    pub target: String,
    pub line: String,
    pub exit_code: Option<i32>,
    pub millis: i64,
    pub passed: i64,
    pub failed: i64,
    pub ignored: i64,
}

/// One install receipt as the journal lists it: (seq, action, bin, destination, sha256).
pub type InstallRow = (i64, String, String, String, String);

pub struct Journal {
    pub path: PathBuf,
    /// The receipts: receipt_history's journal, whose connection custodian's tables share.
    receipts: receipt_history::Journal,
}

fn jerr(e: impl std::fmt::Display) -> crate::codes::Refusal {
    Code::Journal.refuse(e)
}

/// receipt_history's refusal as custodian's: the journal's and a write's codes, its detail.
fn rh(e: receipt_history::Error) -> Refusal {
    match e.refusal {
        receipt_history::Refusal::Journal => Code::Journal.refuse(e.detail),
        receipt_history::Refusal::Write => Code::Write.refuse(e.detail),
    }
}

/// Now, UTC, `2026-10-19T07:38:03Z`.
pub fn now_utc() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Now as a reading's stamp, `20261019T073803Z`.
pub fn stamp() -> String {
    chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
}

fn reading_of(r: &Row<'_>) -> rusqlite::Result<ReadingRow> {
    Ok(ReadingRow {
        seq: r.get("seq")?,
        name: r.get("name")?,
        kind: r.get("kind")?,
        target: r.get("target")?,
        spec_digest: r.get("spec_digest")?,
        index_key: r.get("index_key")?,
        word: r.get("word")?,
        stamp: r.get("stamp")?,
        page: r.get("page")?,
        record: r.get("record")?,
        reading_digest: r.get("reading_digest")?,
        job: r.get("job")?,
    })
}

fn job_of(r: &Row<'_>) -> rusqlite::Result<JobRow> {
    Ok(JobRow {
        id: r.get("id")?,
        name: r.get("name")?,
        kind: r.get("kind")?,
        state: r.get("state")?,
        folder: r.get("folder")?,
        origin: r.get("origin")?,
        opened_seq: r.get("opened_seq")?,
        closed_seq: r.get("closed_seq")?,
        opened_utc: r.get::<_, Option<String>>("opened_utc")?.unwrap_or_default(),
        closed_utc: r.get("closed_utc")?,
    })
}

fn event_of(r: &Row<'_>) -> rusqlite::Result<Event> {
    Ok(Event { n: r.get("n")?, utc: r.get("utc")?, kind: r.get("kind")?, job: r.get("job")?, subject: r.get("subject")?, state: r.get("state")?, detail: r.get("detail")?, seq: r.get("seq")? })
}

fn test_run_of(r: &Row<'_>) -> rusqlite::Result<TestRun> {
    Ok(TestRun {
        seq: r.get("seq")?,
        job: r.get("job")?,
        package: r.get("package")?,
        target: r.get("target")?,
        line: r.get("line")?,
        exit_code: r.get("exit_code")?,
        millis: r.get("millis")?,
        passed: r.get("passed")?,
        failed: r.get("failed")?,
        ignored: r.get("ignored")?,
    })
}

impl Journal {
    /// The journal at `path`, its tables made if new: receipt_history's (`runs`, `jobs`, `tests`), then
    /// custodian's, and the columns custodian keeps on a job.
    pub fn open(path: &Path) -> Result<Journal> {
        let receipts = receipt_history::Journal::open(path).map_err(rh)?;
        let conn = receipts.connection();
        conn.execute_batch(SCHEMA).map_err(jerr)?;
        let have: Vec<String> = {
            let mut st = conn.prepare("SELECT name FROM pragma_table_info('jobs')").map_err(jerr)?;
            let names = st.query_map([], |r| r.get::<_, String>(0)).map_err(jerr)?;
            names.collect::<rusqlite::Result<Vec<_>>>().map_err(jerr)?
        };
        for (col, ty) in JOB_COLUMNS {
            if !have.iter().any(|h| h == col) {
                conn.execute_batch(&format!("ALTER TABLE jobs ADD COLUMN {col} {ty}")).map_err(jerr)?;
            }
        }
        Ok(Journal { path: path.to_path_buf(), receipts })
    }

    fn conn(&self) -> &Connection {
        self.receipts.connection()
    }

    /// Keeps a receipt: takes the next sequence number inside one writing transaction, writes the receipt file the
    /// layout names for that number, and inserts the row (receipt_history's keep). Returns the number and the file.
    pub fn keep(&mut self, r: &mut Receipt, file_for: impl Fn(i64) -> Result<PathBuf>) -> Result<(i64, PathBuf)> {
        self.receipts.keep(r, |n| file_for(n).map_err(|e| receipt_history::Refusal::Write.refuse(e))).map_err(rh)
    }

    pub fn receipt(&self, seq: i64) -> Result<Option<Receipt>> {
        self.receipts.receipt(seq).map_err(rh)
    }

    /// Every receipt, oldest first.
    pub fn receipts(&self) -> Result<Vec<Receipt>> {
        self.receipts.runs().map_err(rh)
    }

    /// The receipts of a job, oldest first.
    pub fn receipts_of_job(&self, job: &str) -> Result<Vec<Receipt>> {
        self.receipts.runs_of_job(job).map_err(rh)
    }

    fn rows<T, P: rusqlite::Params>(&self, sql: &str, p: P, f: fn(&Row<'_>) -> rusqlite::Result<T>) -> Result<Vec<T>> {
        let mut st = self.conn().prepare(sql).map_err(jerr)?;
        let rows = st.query_map(p, f).map_err(jerr)?;
        rows.collect::<rusqlite::Result<Vec<T>>>().map_err(jerr)
    }

    // ---- readings ----

    pub fn index_reading(&self, row: &ReadingRow) -> Result<()> {
        self.conn()
            .execute(
                "INSERT INTO readings (seq, name, kind, target, spec_digest, index_key, word, stamp, page, record, reading_digest, job) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![row.seq, row.name, row.kind, row.target, row.spec_digest, row.index_key, row.word, row.stamp, row.page, row.record, row.reading_digest, row.job],
            )
            .map_err(jerr)?;
        Ok(())
    }

    /// The newest reading of this spec over this code, with the receipt that made it: the kit's one key (the
    /// spec's hash and klepto's fingerprint of the code, `custodian_kit::api::SearchKey`) looked up in the runs, as
    /// custodian_kit's own journal looks it up, and the reading the index keeps under that run.
    pub fn same_search(&self, spec_hash: &str, code_digest: &str) -> Result<Option<(ReadingRow, Receipt)>> {
        match self.receipts.same_search(spec_hash, code_digest).map_err(rh)? {
            Some(r) => Ok(self.reading(r.seq)?.map(|row| (row, r))),
            None => Ok(None),
        }
    }

    /// The readings of a spec, oldest first.
    pub fn readings_of(&self, name: &str) -> Result<Vec<ReadingRow>> {
        self.rows("SELECT * FROM readings WHERE name = ?1 ORDER BY seq", [name], reading_of)
    }

    pub fn reading(&self, seq: i64) -> Result<Option<ReadingRow>> {
        self.conn().query_row("SELECT * FROM readings WHERE seq = ?1", [seq], reading_of).optional().map_err(jerr)
    }

    /// Every reading, oldest first.
    pub fn readings(&self) -> Result<Vec<ReadingRow>> {
        self.rows("SELECT * FROM readings ORDER BY seq", [], reading_of)
    }

    // ---- jobs and events ----

    pub fn put_job(&self, j: &JobRow) -> Result<()> {
        self.conn()
            .execute(
                "INSERT INTO jobs (id, name, kind, state, folder, origin, opened_seq, closed_seq, opened_utc, closed_utc) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, kind = excluded.kind, state = excluded.state, folder = excluded.folder, origin = excluded.origin,
                 opened_seq = excluded.opened_seq, closed_seq = excluded.closed_seq, closed_utc = excluded.closed_utc",
                params![j.id, j.name, j.kind, j.state, j.folder, j.origin, j.opened_seq, j.closed_seq, j.opened_utc, j.closed_utc],
            )
            .map_err(jerr)?;
        Ok(())
    }

    pub fn job(&self, id: &str) -> Result<Option<JobRow>> {
        self.conn().query_row("SELECT * FROM jobs WHERE id = ?1 OR name = ?1 ORDER BY opened_utc DESC LIMIT 1", [id], job_of).optional().map_err(jerr)
    }

    pub fn jobs(&self) -> Result<Vec<JobRow>> {
        self.rows("SELECT * FROM jobs ORDER BY opened_utc, id", [], job_of)
    }

    /// The jobs in `state`.
    pub fn jobs_in(&self, state: &str) -> Result<Vec<JobRow>> {
        self.rows("SELECT * FROM jobs WHERE state = ?1 ORDER BY opened_utc, id", [state], job_of)
    }

    pub fn event(&self, kind: &str, job: Option<&str>, subject: &str, state: &str, detail: &str, seq: Option<i64>) -> Result<i64> {
        self.conn()
            .execute("INSERT INTO events (utc, kind, job, subject, state, detail, seq) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)", params![now_utc(), kind, job, subject, state, detail, seq])
            .map_err(jerr)?;
        Ok(self.conn().last_insert_rowid())
    }

    /// The events after `n`, oldest first.
    pub fn events_since(&self, n: i64) -> Result<Vec<Event>> {
        self.rows("SELECT * FROM events WHERE n > ?1 ORDER BY n", [n], event_of)
    }

    // ---- tests ----

    pub fn test_run(&self, t: &TestRun, results: &[(String, String)]) -> Result<()> {
        self.conn()
            .execute(
                "INSERT INTO test_runs (seq, job, package, target, line, exit_code, millis, passed, failed, ignored) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![t.seq, t.job, t.package, t.target, t.line, t.exit_code, t.millis, t.passed, t.failed, t.ignored],
            )
            .map_err(jerr)?;
        for (test, outcome) in results {
            self.conn().execute("INSERT INTO tests (seq, package, target, test, outcome, millis) VALUES (?1, ?2, ?3, ?4, ?5, NULL)", params![t.seq, t.package, t.target, test, outcome]).map_err(jerr)?;
        }
        Ok(())
    }

    pub fn test_runs(&self) -> Result<Vec<TestRun>> {
        self.rows("SELECT * FROM test_runs ORDER BY seq", [], test_run_of)
    }

    pub fn test_runs_of_job(&self, job: &str) -> Result<Vec<TestRun>> {
        self.rows("SELECT * FROM test_runs WHERE job = ?1 ORDER BY seq", [job], test_run_of)
    }

    /// Per test: (package, test, runs, failures), the most failing first. A failure is cargo's `FAILED` (or `failed`,
    /// the word an earlier custodian wrote).
    pub fn test_outcomes(&self) -> Result<Vec<(String, String, i64, i64)>> {
        let mut st = self
            .conn()
            .prepare("SELECT package, test, COUNT(*), SUM(CASE WHEN outcome IN ('FAILED', 'failed') THEN 1 ELSE 0 END) FROM tests GROUP BY package, test ORDER BY 4 DESC, 3 DESC, 1, 2")
            .map_err(jerr)?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).map_err(jerr)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(jerr)
    }

    // ---- installs ----

    pub fn install(&self, seq: i64, action: &str, bin: &str, destination: &str, sha256: &str, receipt_json: &str) -> Result<()> {
        self.conn()
            .execute("INSERT INTO installs (seq, action, bin, destination, sha256, receipt) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![seq, action, bin, destination, sha256, receipt_json])
            .map_err(jerr)?;
        Ok(())
    }

    /// (seq, action, bin, destination, sha256), oldest first.
    pub fn installs(&self) -> Result<Vec<InstallRow>> {
        let mut st = self.conn().prepare("SELECT seq, action, bin, destination, sha256 FROM installs ORDER BY seq").map_err(jerr)?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))).map_err(jerr)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(jerr)
    }

    // ---- checkpoints ----

    pub fn save_checkpoint(&self, job: &str, step: i64, payload: &[u8]) -> Result<i64> {
        self.conn().execute("INSERT INTO checkpoints (job, step, payload, utc) VALUES (?1, ?2, ?3, ?4)", params![job, step, payload, now_utc()]).map_err(jerr)?;
        Ok(self.conn().last_insert_rowid())
    }

    pub fn load_checkpoint(&self, handle: i64) -> Result<Option<Vec<u8>>> {
        self.conn().query_row("SELECT payload FROM checkpoints WHERE handle = ?1", [handle], |r| r.get(0)).optional().map_err(jerr)
    }

    pub fn remove_checkpoint(&self, handle: i64) -> Result<()> {
        self.conn().execute("DELETE FROM checkpoints WHERE handle = ?1", [handle]).map_err(jerr)?;
        Ok(())
    }

    /// Every checkpoint of a job, gone once the job has run all its stages.
    pub fn remove_checkpoints_of(&self, job: &str) -> Result<()> {
        self.conn().execute("DELETE FROM checkpoints WHERE job = ?1", [job]).map_err(jerr)?;
        Ok(())
    }

    /// The reading a job's step made under `name`: its own, or the one its reuse pointed at.
    pub fn step_reading(&self, job: &str, name: &str) -> Result<Option<ReadingRow>> {
        let last: Option<(i64, Option<i64>)> = self
            .conn()
            .query_row("SELECT seq, reused_seq FROM runs WHERE job = ?1 AND name = ?2 AND command = 'reading' ORDER BY seq DESC LIMIT 1", params![job, name], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()
            .map_err(jerr)?;
        match last {
            Some((_, Some(reused))) => self.reading(reused),
            Some((seq, None)) => self.reading(seq),
            None => Ok(None),
        }
    }

    /// A job's newest checkpoint: (handle, the step it was saved after, the bytes).
    pub fn latest_checkpoint(&self, job: &str) -> Result<Option<(i64, i64, Vec<u8>)>> {
        self.conn()
            .query_row("SELECT handle, step, payload FROM checkpoints WHERE job = ?1 ORDER BY handle DESC LIMIT 1", [job], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()
            .map_err(jerr)
    }

    /// How many receipts the journal holds.
    pub fn count(&self) -> Result<i64> {
        self.receipts.count().map_err(rh)
    }

    /// The journal brought up to the records a checkout carries: every receipt file (`data/receipts/<seq>.json`)
    /// the journal has no row for is put back under its own number, with the reading it made indexed (its page and
    /// record where the receipt names them, else the copy in its job's folder, wherever the folder is now), and every
    /// job folder (`state/jobs/<state>/<job>/`) the journal does not know is put back with its state and times. The
    /// journal is machine state, never committed; the receipts and the job folders are the record, so a fresh
    /// checkout's journal is rebuilt from them and its sequence continues from the highest receipt. A journal that
    /// holds every receipt and job is left as it is.
    pub fn rebuild_from(&mut self, root: &Path, receipts_dir: &Path, jobs_root: &Path) -> Result<Rebuilt> {
        let mut out = Rebuilt::default();
        let folders = job_folders(jobs_root);
        // the runs from the receipt files (receipt_history's), each reading indexed in the same transaction
        let back = self
            .receipts
            .rebuild_from(receipts_dir, |conn, r| {
                if let Some(row) = reading_row_of(root, r, &folders) {
                    conn.execute(
                        "INSERT OR IGNORE INTO readings (seq, name, kind, target, spec_digest, index_key, word, stamp, page, record, reading_digest, job) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                        params![row.seq, row.name, row.kind, row.target, row.spec_digest, row.index_key, row.word, row.stamp, row.page, row.record, row.reading_digest, row.job],
                    )
                    .map_err(|e| receipt_history::Refusal::Journal.refuse(e))?;
                    out.readings += 1;
                }
                Ok(())
            })
            .map_err(rh)?;
        out.receipts = back.receipts;
        out.last = back.last;
        // the job folders the journal does not know
        let jobs = self
            .receipts
            .writing(|conn| {
                let known: std::collections::BTreeSet<String> = {
                    let mut st = conn.prepare("SELECT id FROM jobs").map_err(|e| receipt_history::Refusal::Journal.refuse(e))?;
                    let ids = st.query_map([], |r| r.get::<_, String>(0)).map_err(|e| receipt_history::Refusal::Journal.refuse(e))?;
                    ids.collect::<rusqlite::Result<_>>().map_err(|e| receipt_history::Refusal::Journal.refuse(e))?
                };
                let mut jobs = 0;
                for (state, dir) in &folders {
                    let jf = custodian_kit::jobs::job_file(dir);
                    let Some(id) = jf.id.clone() else { continue };
                    if known.contains(&id) {
                        continue;
                    }
                    let closing: Option<custodian_kit::jobs::Closing> = std::fs::read_to_string(dir.join("closing.json")).ok().and_then(|t| serde_json::from_str(&t).ok());
                    let folder = dir.strip_prefix(root).unwrap_or(dir).to_string_lossy().into_owned();
                    conn.execute(
                        "INSERT OR IGNORE INTO jobs (id, name, kind, state, folder, opened_utc, closed_utc) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                        params![id, jf.name.clone().unwrap_or_default(), jf.kind.clone().unwrap_or_else(|| "specs".into()), state, folder, closing.as_ref().and_then(|c| c.opened_utc.clone()), closing.as_ref().map(|c| c.closed_utc.clone())],
                    )
                    .map_err(|e| receipt_history::Refusal::Journal.refuse(e))?;
                    jobs += 1;
                }
                Ok(jobs)
            })
            .map_err(rh)?;
        out.jobs = jobs;
        Ok(out)
    }
}

/// What a rebuild put back.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rebuilt {
    pub receipts: usize,
    pub readings: usize,
    pub jobs: usize,
    /// The highest receipt number after it: where the sequence continues.
    pub last: i64,
}

impl Rebuilt {
    pub fn any(&self) -> bool {
        self.receipts + self.jobs > 0
    }
}

/// Every job folder under `state/jobs/`, with its state (`queued`, `running`, `done`).
pub fn job_folders(jobs_root: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for state in ["queued", "running", "done"] {
        let dir = jobs_root.join(state);
        let mut v: Vec<PathBuf> = std::fs::read_dir(&dir).map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect()).unwrap_or_default();
        v.sort();
        out.extend(v.into_iter().map(|p| (state.to_string(), p)));
    }
    out
}

/// The reading a receipt made, as the index keeps it: a run that read a spec (`reading`, or `run` before the kit's
/// word), accepted, not a reuse, whose record is where the receipt says or in its job's folder.
fn reading_row_of(root: &Path, r: &Receipt, folders: &[(String, PathBuf)]) -> Option<ReadingRow> {
    if !(r.command == "reading" || r.command == "run") || r.reused_seq.is_some() || !r.accepted || r.spec_hash.is_none() {
        return None;
    }
    let json = r.outputs.iter().find(|o| o.ends_with(".json"))?;
    let page = r.outputs.iter().find(|o| o.ends_with(".md")).cloned().unwrap_or_else(|| json.replace(".json", ".md"));
    let rel = |p: &Path| p.strip_prefix(root).unwrap_or(p).to_string_lossy().into_owned();
    let found = if root.join(json).is_file() {
        Some((json.clone(), page.clone()))
    } else {
        // the job's folder, wherever its state is now: readings/<name>/<file>
        let name = Path::new(json).file_name()?.to_string_lossy().into_owned();
        let job_first = folders.iter().filter(|(_, d)| r.job.as_deref().is_some_and(|j| custodian_kit::jobs::job_file(d).id.as_deref() == Some(j))).chain(folders.iter());
        job_first.map(|(_, d)| d.join("readings").join(&r.name).join(&name)).find(|p| p.is_file()).map(|p| (rel(&p), rel(&p.with_extension("md"))))
    }?;
    let report = custodian_kit::reading::load(&root.join(&found.0)).ok()?;
    let digest = custodian_kit::api::Reading { report: report.clone() }.digest();
    Some(ReadingRow {
        seq: r.seq,
        name: r.name.clone(),
        kind: report.kind.as_str().into(),
        target: report.target.clone(),
        spec_digest: r.spec_hash.clone().unwrap_or_default(),
        index_key: r.index_key.clone().or_else(|| r.code_digest.clone()).unwrap_or_default(),
        word: "none".into(),
        stamp: report.stamp.clone(),
        page: found.1,
        record: found.0,
        reading_digest: digest,
        job: r.job.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt(name: &str) -> Receipt {
        Receipt {
            schema: custodian_kit::api::RECEIPT_SCHEMA.into(),
            seq: 0,
            utc: now_utc(),
            command: "run".into(),
            name: name.into(),
            spec: None,
            spec_hash: Some("s".into()),
            code_digest: None,
            index_key: Some("k".into()),
            plan_id: "custodian".into(),
            plan_digest: "p".into(),
            operation_digest: "o".into(),
            receipt_digest: "r".into(),
            line: "custodian run x".into(),
            exit_code: Some(0),
            accepted: true,
            disposition: "Continue".into(),
            reason: "done".into(),
            outputs: vec!["a.md".into()],
            job: None,
            reused_seq: None,
            millis: 3,
            runner: custodian_kit::receipts::RUNNER_IN_PROCESS.into(),
        }
    }

    // `two_journals_on_one_file_never_take_one_number` moved with the sequence it proves: receipt_history's
    // tests/journal.rs. The one below proves custodian's tables sit beside receipt_history's in one file.
    #[test]
    fn a_journal_custodian_kit_made_is_extended_not_replaced() {
        let dir = std::env::temp_dir().join(format!("custodian-kit-journal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("journal.sqlite");
        let kit = Connection::open(&file).unwrap();
        kit.execute_batch(custodian_kit::store::JOURNAL_SCHEMA).unwrap();
        drop(kit);
        let mut j = Journal::open(&file).unwrap();
        let job = JobRow { id: "j-1".into(), name: "n".into(), kind: "specs".into(), state: "running".into(), folder: "f".into(), origin: Some("sha256:x".into()), opened_seq: None, closed_seq: None, opened_utc: now_utc(), closed_utc: None };
        j.put_job(&job).unwrap();
        assert_eq!(j.job("j-1").unwrap().unwrap().origin.as_deref(), Some("sha256:x"));
        let (n, _) = j.keep(&mut receipt("one"), |n| Ok(dir.join(format!("{n}.json")))).unwrap();
        assert_eq!(n, 1);
        let kit_rows: i64 = Connection::open(&file).unwrap().query_row("SELECT COUNT(*) FROM runs", [], |r| r.get(0)).unwrap();
        assert_eq!(kit_rows, 1, "the receipt is in the kit's runs table");
    }
}

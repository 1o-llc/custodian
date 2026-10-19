//! The journal: one SQLite file, `.custodian/state/journal.sqlite`: custodian_kit's tables (a receipt per run in
//! `runs`, the `jobs`, the `tests` outcomes) and the ones custodian adds: the readings index the same-search check
//! reads, the events (job states, steps, test progress), the test lines, the install receipts and the job queue's
//! checkpoints. Its sequence is uh-tom-mick's checked counter, brought up to the file's last receipt inside
//! the writing transaction (`BEGIN IMMEDIATE`), so two processes never take one number.
//!
//! The tables are declared once, in [`SCHEMA`]; the inventory (`custodian list`) is rendered from them.

use crate::codes::{Code, Result};
use crate::layout::write_whole;
use custodian_kit::api::Receipt;
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use uh_tom_mick::AtomicCounter;

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
    conn: Connection,
    seq: AtomicCounter,
}

fn jerr(e: impl std::fmt::Display) -> crate::codes::Refusal {
    Code::Journal.refuse(e)
}

/// Now, UTC, `2026-10-19T07:38:03Z`.
pub fn now_utc() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Now as a reading's stamp, `20261019T073803Z`.
pub fn stamp() -> String {
    chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string()
}

fn receipt_of(r: &Row<'_>) -> rusqlite::Result<Receipt> {
    let outputs: String = r.get("outputs")?;
    Ok(Receipt {
        schema: custodian_kit::api::RECEIPT_SCHEMA.into(),
        seq: r.get("seq")?,
        utc: r.get("utc")?,
        command: r.get("command")?,
        name: r.get("name")?,
        spec: r.get("spec")?,
        spec_hash: r.get("spec_hash")?,
        code_digest: r.get("code_digest")?,
        index_key: r.get("index_key")?,
        plan_id: r.get("plan_id")?,
        plan_digest: r.get("plan_digest")?,
        operation_digest: r.get("operation_digest")?,
        receipt_digest: r.get("receipt_digest")?,
        line: r.get("line")?,
        exit_code: r.get("exit_code")?,
        accepted: r.get::<_, i64>("accepted")? != 0,
        disposition: r.get("disposition")?,
        reason: r.get("reason")?,
        outputs: serde_json::from_str(&outputs).unwrap_or_default(),
        job: r.get("job")?,
        reused_seq: r.get("reused_seq")?,
        millis: r.get("millis")?,
        runner: r.get("runner")?,
    })
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
    /// The journal at `path`, its tables made if new.
    pub fn open(path: &Path) -> Result<Journal> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d).map_err(|e| Code::Write.refuse(format!("{}: {e}", d.display())))?;
        }
        let conn = Connection::open(path).map_err(|e| jerr(format!("{}: {e}", path.display())))?;
        conn.busy_timeout(std::time::Duration::from_secs(10)).map_err(jerr)?;
        conn.execute_batch(custodian_kit::store::JOURNAL_SCHEMA).map_err(jerr)?;
        conn.execute_batch(SCHEMA).map_err(jerr)?;
        let have: Vec<String> = {
            let mut st = conn.prepare("SELECT name FROM pragma_table_info('jobs')").map_err(jerr)?;
            let names = st.query_map([], |r| r.get::<_, String>(0)).map_err(jerr)?;
            names.collect::<rusqlite::Result<Vec<_>>>().map_err(jerr)?
        };
        // a journal from before the receipt's runner field gets the column, every older run a line run
        let has_runner: bool = conn.prepare("SELECT 1 FROM pragma_table_info('runs') WHERE name = 'runner'").and_then(|mut st| st.exists([])).map_err(jerr)?;
        if !has_runner {
            conn.execute_batch("ALTER TABLE runs ADD COLUMN runner TEXT NOT NULL DEFAULT 'line'").map_err(jerr)?;
        }
        for (col, ty) in JOB_COLUMNS {
            if !have.iter().any(|h| h == col) {
                conn.execute_batch(&format!("ALTER TABLE jobs ADD COLUMN {col} {ty}")).map_err(jerr)?;
            }
        }
        let last: i64 = conn.query_row("SELECT COALESCE(MAX(seq), 0) FROM runs", [], |r| r.get(0)).map_err(jerr)?;
        Ok(Journal { path: path.to_path_buf(), conn, seq: AtomicCounter::new(last.max(0) as u64) })
    }

    /// Keeps a receipt: takes the next sequence number inside one writing transaction, writes the receipt file the
    /// layout names for that number, and inserts the row. Returns the number and the file.
    pub fn keep(&mut self, r: &mut Receipt, file_for: impl Fn(i64) -> Result<PathBuf>) -> Result<(i64, PathBuf)> {
        let tx = self.conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(jerr)?;
        let last: i64 = tx.query_row("SELECT COALESCE(MAX(seq), 0) FROM runs", [], |row| row.get(0)).map_err(jerr)?;
        // a receipt file is never written over: the number is above every receipt file already there too
        let dir = file_for(1)?.parent().map(Path::to_path_buf).unwrap_or_default();
        let on_disk = std::fs::read_dir(&dir)
            .map(|rd| rd.filter_map(|e| e.ok()).filter_map(|e| e.path().file_stem().and_then(|s| s.to_str()).and_then(|s| s.parse::<i64>().ok())).max().unwrap_or(0))
            .unwrap_or(0);
        let last = last.max(on_disk);
        let have = self.seq.load();
        if (last as u64) > have {
            self.seq.add(last as u64 - have).map_err(jerr)?;
        }
        let n = self.seq.increment().map_err(jerr)? as i64;
        r.seq = n;
        let file = file_for(n)?;
        write_whole(&file, serde_json::to_string_pretty(r).map_err(jerr)?.as_bytes())?;
        tx.execute(
            "INSERT INTO runs (seq, utc, command, name, spec, spec_hash, index_key, code_digest, job, reused_seq, line, exit_code, accepted, disposition, reason, plan_id, plan_digest, operation_digest, receipt_digest, outputs, millis, runner)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)",
            params![
                n,
                r.utc,
                r.command,
                r.name,
                r.spec,
                r.spec_hash,
                r.index_key,
                r.code_digest,
                r.job,
                r.reused_seq,
                r.line,
                r.exit_code,
                r.accepted as i64,
                r.disposition,
                r.reason,
                r.plan_id,
                r.plan_digest,
                r.operation_digest,
                r.receipt_digest,
                serde_json::to_string(&r.outputs).map_err(jerr)?,
                r.millis,
                r.runner,
            ],
        )
        .map_err(jerr)?;
        tx.commit().map_err(jerr)?;
        Ok((n, file))
    }

    pub fn receipt(&self, seq: i64) -> Result<Option<Receipt>> {
        self.conn.query_row("SELECT * FROM runs WHERE seq = ?1", [seq], receipt_of).optional().map_err(jerr)
    }

    /// Every receipt, oldest first.
    pub fn receipts(&self) -> Result<Vec<Receipt>> {
        self.rows("SELECT * FROM runs ORDER BY seq", [], receipt_of)
    }

    /// The receipts of a job, oldest first.
    pub fn receipts_of_job(&self, job: &str) -> Result<Vec<Receipt>> {
        self.rows("SELECT * FROM runs WHERE job = ?1 ORDER BY seq", [job], receipt_of)
    }

    fn rows<T, P: rusqlite::Params>(&self, sql: &str, p: P, f: fn(&Row<'_>) -> rusqlite::Result<T>) -> Result<Vec<T>> {
        let mut st = self.conn.prepare(sql).map_err(jerr)?;
        let rows = st.query_map(p, f).map_err(jerr)?;
        rows.collect::<rusqlite::Result<Vec<T>>>().map_err(jerr)
    }

    // ---- readings ----

    pub fn index_reading(&self, row: &ReadingRow) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO readings (seq, name, kind, target, spec_digest, index_key, word, stamp, page, record, reading_digest, job) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![row.seq, row.name, row.kind, row.target, row.spec_digest, row.index_key, row.word, row.stamp, row.page, row.record, row.reading_digest, row.job],
            )
            .map_err(jerr)?;
        Ok(())
    }

    /// The newest reading of this spec over this code, with the receipt that made it.
    pub fn same_search(&self, spec_digest: &str, index_key: &str) -> Result<Option<(ReadingRow, Receipt)>> {
        let row = self
            .conn
            .query_row("SELECT * FROM readings WHERE spec_digest = ?1 AND index_key = ?2 ORDER BY seq DESC LIMIT 1", params![spec_digest, index_key], reading_of)
            .optional()
            .map_err(jerr)?;
        match row {
            Some(r) => {
                let receipt = self.receipt(r.seq)?.ok_or_else(|| jerr(format!("reading #{} has no receipt", r.seq)))?;
                Ok(Some((r, receipt)))
            }
            None => Ok(None),
        }
    }

    /// The readings of a spec, oldest first.
    pub fn readings_of(&self, name: &str) -> Result<Vec<ReadingRow>> {
        self.rows("SELECT * FROM readings WHERE name = ?1 ORDER BY seq", [name], reading_of)
    }

    pub fn reading(&self, seq: i64) -> Result<Option<ReadingRow>> {
        self.conn.query_row("SELECT * FROM readings WHERE seq = ?1", [seq], reading_of).optional().map_err(jerr)
    }

    /// Every reading, oldest first.
    pub fn readings(&self) -> Result<Vec<ReadingRow>> {
        self.rows("SELECT * FROM readings ORDER BY seq", [], reading_of)
    }

    // ---- jobs and events ----

    pub fn put_job(&self, j: &JobRow) -> Result<()> {
        self.conn
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
        self.conn.query_row("SELECT * FROM jobs WHERE id = ?1 OR name = ?1 ORDER BY opened_utc DESC LIMIT 1", [id], job_of).optional().map_err(jerr)
    }

    pub fn jobs(&self) -> Result<Vec<JobRow>> {
        self.rows("SELECT * FROM jobs ORDER BY opened_utc, id", [], job_of)
    }

    /// The jobs in `state`.
    pub fn jobs_in(&self, state: &str) -> Result<Vec<JobRow>> {
        self.rows("SELECT * FROM jobs WHERE state = ?1 ORDER BY opened_utc, id", [state], job_of)
    }

    pub fn event(&self, kind: &str, job: Option<&str>, subject: &str, state: &str, detail: &str, seq: Option<i64>) -> Result<i64> {
        self.conn
            .execute("INSERT INTO events (utc, kind, job, subject, state, detail, seq) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)", params![now_utc(), kind, job, subject, state, detail, seq])
            .map_err(jerr)?;
        Ok(self.conn.last_insert_rowid())
    }

    /// The events after `n`, oldest first.
    pub fn events_since(&self, n: i64) -> Result<Vec<Event>> {
        self.rows("SELECT * FROM events WHERE n > ?1 ORDER BY n", [n], event_of)
    }

    // ---- tests ----

    pub fn test_run(&self, t: &TestRun, results: &[(String, String)]) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO test_runs (seq, job, package, target, line, exit_code, millis, passed, failed, ignored) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                params![t.seq, t.job, t.package, t.target, t.line, t.exit_code, t.millis, t.passed, t.failed, t.ignored],
            )
            .map_err(jerr)?;
        for (test, outcome) in results {
            self.conn.execute("INSERT INTO tests (seq, package, target, test, outcome, millis) VALUES (?1, ?2, ?3, ?4, ?5, NULL)", params![t.seq, t.package, t.target, test, outcome]).map_err(jerr)?;
        }
        Ok(())
    }

    pub fn test_runs(&self) -> Result<Vec<TestRun>> {
        self.rows("SELECT * FROM test_runs ORDER BY seq", [], test_run_of)
    }

    pub fn test_runs_of_job(&self, job: &str) -> Result<Vec<TestRun>> {
        self.rows("SELECT * FROM test_runs WHERE job = ?1 ORDER BY seq", [job], test_run_of)
    }

    /// Per test: (package, test, runs, failures), the most failing first.
    pub fn test_outcomes(&self) -> Result<Vec<(String, String, i64, i64)>> {
        let mut st = self
            .conn
            .prepare("SELECT package, test, COUNT(*), SUM(CASE WHEN outcome = 'failed' THEN 1 ELSE 0 END) FROM tests GROUP BY package, test ORDER BY 4 DESC, 3 DESC, 1, 2")
            .map_err(jerr)?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))).map_err(jerr)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(jerr)
    }

    // ---- installs ----

    pub fn install(&self, seq: i64, action: &str, bin: &str, destination: &str, sha256: &str, receipt_json: &str) -> Result<()> {
        self.conn
            .execute("INSERT INTO installs (seq, action, bin, destination, sha256, receipt) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![seq, action, bin, destination, sha256, receipt_json])
            .map_err(jerr)?;
        Ok(())
    }

    /// (seq, action, bin, destination, sha256), oldest first.
    pub fn installs(&self) -> Result<Vec<InstallRow>> {
        let mut st = self.conn.prepare("SELECT seq, action, bin, destination, sha256 FROM installs ORDER BY seq").map_err(jerr)?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))).map_err(jerr)?;
        rows.collect::<rusqlite::Result<Vec<_>>>().map_err(jerr)
    }

    // ---- checkpoints ----

    pub fn save_checkpoint(&self, job: &str, step: i64, payload: &[u8]) -> Result<i64> {
        self.conn.execute("INSERT INTO checkpoints (job, step, payload, utc) VALUES (?1, ?2, ?3, ?4)", params![job, step, payload, now_utc()]).map_err(jerr)?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn load_checkpoint(&self, handle: i64) -> Result<Option<Vec<u8>>> {
        self.conn.query_row("SELECT payload FROM checkpoints WHERE handle = ?1", [handle], |r| r.get(0)).optional().map_err(jerr)
    }

    pub fn remove_checkpoint(&self, handle: i64) -> Result<()> {
        self.conn.execute("DELETE FROM checkpoints WHERE handle = ?1", [handle]).map_err(jerr)?;
        Ok(())
    }

    /// Every checkpoint of a job, gone once the job has run all its stages.
    pub fn remove_checkpoints_of(&self, job: &str) -> Result<()> {
        self.conn.execute("DELETE FROM checkpoints WHERE job = ?1", [job]).map_err(jerr)?;
        Ok(())
    }

    /// The reading a job's step made under `name`: its own, or the one its reuse pointed at.
    pub fn step_reading(&self, job: &str, name: &str) -> Result<Option<ReadingRow>> {
        let last: Option<(i64, Option<i64>)> = self
            .conn
            .query_row("SELECT seq, reused_seq FROM runs WHERE job = ?1 AND name = ?2 AND command = 'run' ORDER BY seq DESC LIMIT 1", params![job, name], |r| Ok((r.get(0)?, r.get(1)?)))
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
        self.conn
            .query_row("SELECT handle, step, payload FROM checkpoints WHERE job = ?1 ORDER BY handle DESC LIMIT 1", [job], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .optional()
            .map_err(jerr)
    }

    /// How many receipts the journal holds.
    pub fn count(&self) -> Result<i64> {
        self.conn.query_row("SELECT COUNT(*) FROM runs", [], |r| r.get(0)).map_err(jerr)
    }
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

    #[test]
    fn two_journals_on_one_file_never_take_one_number() {
        let dir = std::env::temp_dir().join(format!("custodian-journal-{}", std::process::id()));
        let file = dir.join("journal.sqlite");
        let mut a = Journal::open(&file).unwrap();
        let mut b = Journal::open(&file).unwrap();
        let at = |n: i64| Ok(dir.join(format!("{n}.json")));
        let (n1, _) = a.keep(&mut receipt("one"), at).unwrap();
        let (n2, _) = b.keep(&mut receipt("two"), at).unwrap();
        let (n3, f3) = a.keep(&mut receipt("three"), at).unwrap();
        assert_eq!((n1, n2, n3), (1, 2, 3));
        let back: Receipt = serde_json::from_str(&std::fs::read_to_string(f3).unwrap()).unwrap();
        assert_eq!((back.seq, back.name.as_str()), (3, "three"));
        assert_eq!(a.receipt(2).unwrap().unwrap().name, "two");
        assert_eq!(b.receipts().unwrap().len(), 3);
    }

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

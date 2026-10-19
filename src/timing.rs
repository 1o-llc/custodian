//! Every duration custodian waits by, declared once, as xccute declares its own (`xccute::timing`): a wait names
//! its line here and nothing else in custodian holds a number of seconds. The job lock's waits are xccute's hold
//! waits, taken from its table; the journal's is custodian_kit's.

use std::time::Duration;

/// The longest a line waits for the project's job lock (another process running a job's start, its close or the
/// queue): xccute's hold timeout.
pub const JOB_LOCK_TIMEOUT: Duration = xccute::timing::HOLD_TIMEOUT;

/// How long it waits between two tries at a busy job lock: xccute's hold backoff.
pub const JOB_LOCK_BACKOFF: Duration = xccute::timing::HOLD_RETRY_BACKOFF;

/// How long a write to the journal waits for another writer (custodian_kit's, one journal, one wait).
pub const JOURNAL_BUSY: Duration = custodian_kit::store::JOURNAL_BUSY;

/// How often `watch --follow` looks for new events.
pub const WATCH_POLL: Duration = Duration::from_millis(500);

/// Every duration above, by name, in the order declared.
pub const TIMING: &[(&str, Duration)] = &[("job_lock_timeout", JOB_LOCK_TIMEOUT), ("job_lock_backoff", JOB_LOCK_BACKOFF), ("journal_busy", JOURNAL_BUSY), ("watch_poll", WATCH_POLL)];

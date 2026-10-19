//! Every refusal the command line gives, in one liaise table: `[CUST1001] sentence: what was refused`.
//!
//! custodian_kit's own refusals are `CUST0001` to `CUST0099` and pass through as the kit wrote them; the command
//! line's start at 1001, so a code means one thing wherever it is printed. The code is what a script matches on,
//! the sentence what a person reads, the detail after it what was refused.

pub use liaise_proc_macro::LiaiseCode;

liaise_proc_macro::liaise_codes! {
    prefix: "CUST",
    pub Code {
        // 10xx: the line itself
        LineRefused = 1001 => "the words are not a declared custodian line",
        NeedsFolder = 1002 => "this needs an initialized folder (.custodian) here or above: custodian init",
        NeedsAgent = 1003 => "jobs are agent mode: custodian agent init",
        NoWorkspace = 1004 => "no Cargo.toml here or above",

        // 11xx: option words
        WordRefused = 1101 => "the options do not make a valid word",

        // 12xx: specs and readings
        SpecNotFound = 1201 => "no spec of that name or path",
        NothingToDiff = 1202 => "a diff needs two readings of the spec",
        ReadingUnreadable = 1203 => "a reading the journal names cannot be read back",
        CountNotFound = 1204 => "no count by that name",
        CountAmbiguous = 1205 => "the name fits more than one count",

        // 13xx: the journal and the folder
        Journal = 1301 => "the journal cannot be read or written",
        Write = 1302 => "a file custodian keeps cannot be written",
        WriterHeld = 1303 => "another run in this process holds the journal's writer",

        // 14xx: jobs
        NoSuchJob = 1401 => "no such job",
        JobRunning = 1402 => "a job is already running; close it first: custodian job close",
        JobFile = 1403 => "a job.toml or template does not read",
        UnknownKind = 1404 => "no job template of that kind",
        StepFailed = 1405 => "a job step's line did not run to an accepted status",

        // 15xx: the socket
        Socket = 1501 => "the serve socket cannot be bound or reached",
        Frame = 1502 => "a frame on the serve socket is not a request or a reply",
        ServeRunning = 1503 => "custodian serve already answers on this project's socket",

        // 16xx: lines custodian runs
        ProgramMissing = 1601 => "the program a declared line runs is not on this machine",
        LineFailed = 1602 => "a declared line could not be started",

        // 17xx: self install
        Install = 1701 => "the install did not complete",

        // 18xx: test, check, annotate, features
        NoTests = 1801 => "no test lines to run",
        CheckRefused = 1802 => "maintenance refused the check line",
        NoWriterLine = 1803 => "code_passenger has no line that writes this, and custodian never edits source itself",
    }
}

/// A refusal: the code and the sentence with its detail. The kit's refusals arrive as the same type.
pub type Refusal = custodian_kit::api::Refusal;

/// The result every handler returns.
pub type Result<T> = std::result::Result<T, Refusal>;

impl Code {
    /// The refusal this code raises, with what was refused.
    pub fn refuse(self, detail: impl std::fmt::Display) -> Refusal {
        Refusal { code: self.code(), message: format!("{}: {detail}", self.message()) }
    }
}

/// The kit's refusal for a kit error.
pub fn kit(e: impl Into<Refusal>) -> Refusal {
    e.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_codes_render_as_liaise_does() {
        assert_eq!(Code::LineRefused.render(), "[CUST1001] the words are not a declared custodian line");
        let r = Code::NoSuchJob.refuse("j-1 in jobs/queued");
        assert_eq!(r.to_string(), "[CUST1401] no such job: j-1 in jobs/queued");
        assert!(Code::ALL_CODES.iter().all(|c| *c >= 1000), "the kit has 0001 to 0099");
    }
}

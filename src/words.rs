//! The command line's option sets, each a big-bad-bool word declared once and checked by flaga_solver: the observe
//! levels and the job kinds. The bundle filters are custodian_kit's word (`api::FiltersWord`, its rules `reveal`
//! requires `explicit` and conflicts with `redact_secret`), used as it is. The word is what a receipt stores and
//! what a same-search key digests.

use crate::codes::Code;
use big_bad_bool::bitpack;
use flaga_solver::{check, schema_of};

/// How much a run shows while it runs: the failures, the progress, or every line.
#[bitpack(storage = u8, version = 1)]
pub struct ObserveLevel {
    #[bits(2)]
    #[describe("what a run shows while it runs")]
    #[labels(failed = 0, progress = 1, all = 2)]
    #[invalid(3)]
    level: u8,
}

/// What a job is: a folder of specs, one of the templates custodian_kit ships, or an exercise.
#[bitpack(storage = u8, version = 1)]
pub struct JobKind {
    #[bits(3)]
    #[describe("what a job is")]
    #[labels(specs = 0, triage = 1, features = 2, exercise = 3)]
    #[invalid(4..=7)]
    kind: u8,
}

/// A word's rules, checked by flaga_solver; the refusal names the rule.
fn verdict(word: &str, v: flaga_solver::Verdict) -> crate::codes::Result<()> {
    if v.is_ok() {
        Ok(())
    } else {
        Err(Code::WordRefused.refuse(format!("{word}: {v}")))
    }
}

/// The names of a field's labels that hold in a value (flags: every bit set; else: equal).
fn labels_of(field: &str, value: u128, flag_field: bool, labels: &[big_bad_bool::ValueLabel]) -> Vec<&'static str> {
    labels
        .iter()
        .filter(|l| l.field == field)
        .filter(|l| if flag_field { l.value != 0 && value & l.value == l.value } else { value == l.value })
        .map(|l| l.label)
        .collect()
}

impl ObserveLevel {
    /// The level a `--observe` word names.
    pub fn of_word(word: &str) -> crate::codes::Result<Self> {
        let v = Self::LABELS.iter().find(|l| l.field == "level" && l.label == word).map(|l| l.value as u8);
        match v {
            Some(v) => {
                let w = Self::from_raw(v);
                verdict("observe level", check(&schema_of!(ObserveLevel), w.raw() as u128))?;
                Ok(w)
            }
            None => Err(Code::WordRefused.refuse(format!("--observe {word}: one of {}", Self::words().join(", ")))),
        }
    }

    /// Every level's word, lowest first.
    pub fn words() -> Vec<&'static str> {
        Self::LABELS.iter().filter(|l| l.field == "level").map(|l| l.label).collect()
    }

    pub fn word(self) -> &'static str {
        labels_of("level", self.level() as u128, false, Self::LABELS).first().copied().unwrap_or("failed")
    }

    pub fn shows_progress(self) -> bool {
        self.level() >= 1
    }

    pub fn shows_all(self) -> bool {
        self.level() >= 2
    }
}

impl JobKind {
    pub fn of_word(word: &str) -> crate::codes::Result<Self> {
        Self::LABELS
            .iter()
            .find(|l| l.field == "kind" && l.label == word)
            .map(|l| Self::from_raw(l.value as u8))
            .ok_or_else(|| Code::UnknownKind.refuse(format!("`{word}`: one of {}", Self::words().join(", "))))
    }

    pub fn words() -> Vec<&'static str> {
        Self::LABELS.iter().filter(|l| l.field == "kind").map(|l| l.label).collect()
    }

    pub fn word(self) -> &'static str {
        labels_of("kind", self.kind() as u128, false, Self::LABELS).first().copied().unwrap_or("specs")
    }

    /// The scheduling class the queue gives a job of this kind: a person waits on triage, a folder of specs is
    /// standard, the features survey is batch work.
    pub fn qos(self) -> waker_protocol::QoSClass {
        use waker_protocol::QoSClass::*;
        match self.word() {
            "triage" => Interactive,
            "specs" => Standard,
            _ => Batch,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observe_levels_and_job_kinds_are_words() {
        assert_eq!(ObserveLevel::words(), ["failed", "progress", "all"]);
        assert_eq!(ObserveLevel::of_word("progress").unwrap().word(), "progress");
        assert!(ObserveLevel::of_word("loud").is_err());
        assert_eq!(JobKind::of_word("triage").unwrap().qos(), waker_protocol::QoSClass::Interactive);
        assert_eq!(JobKind::words(), ["specs", "triage", "features", "exercise"]);
        assert_eq!(JobKind::of_word("exercise").unwrap().qos(), waker_protocol::QoSClass::Batch);
        assert_eq!(JobKind::of_word("features").unwrap().qos(), waker_protocol::QoSClass::Batch);
    }
}

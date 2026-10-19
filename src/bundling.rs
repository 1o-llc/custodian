//! `custodian bundle ROUTE|JOB [--no-tests] [--no-benches] [--no-examples] [--no-feature F] [--only src]
//! [--redact-level secret] [--reveal]`: a
//! route (`crate:<name>`, `decl:<name>`, a declared `path:`) or a job's folder as one annotated bundles file.
//!
//! The filters are custodian_kit's word (`FiltersWord`, a big-bad-bool word whose rules flaga_solver checks before
//! anything is read: `reveal` requires `explicit`, which only `--reveal` on the command line sets, and conflicts
//! with `redact_secret`). The word is what the receipt keeps and what the same-search key digests, so the same
//! route with the same filters over the same code is a reuse. A route goes through custodian_kit's bundle (the
//! files klepto read, secret items held redacted through classified unless revealed); a job through its job
//! bundler (the folder, its receipts, the windows around its sites).

use crate::codes::Result;
use crate::decide;
use crate::facade::{Ctx, Ran};
use crate::journal::ReadingRow;
use crate::readings;
use crate::surface::{s, so, Bundle};
use custodian_kit::api::{FiltersWord, IndexKey, Resolving};

/// The word a line's filters make, checked: secret items redacted unless `--reveal`.
pub fn word_of(l: &Bundle) -> Result<FiltersWord> {
    let mut names: Vec<&str> = Vec::new();
    if l.no_tests {
        names.push("no_tests");
    }
    if l.no_benches {
        names.push("no_benches");
    }
    if l.no_examples {
        names.push("no_examples");
    }
    if l.only.is_some() {
        names.push("only_src");
    }
    if l.redact_level.is_some() || !l.reveal {
        names.push("redact_secret");
    }
    let mut w = FiltersWord::from_names(names)?;
    if l.reveal {
        // --reveal clears the default redaction; an explicit --redact-level stays, and the word's rule refuses the two
        w = if l.redact_level.is_some() { w.with_filters_reveal().with_filters_explicit() } else { w.reveal_explicitly() };
    }
    w.check()
}

/// The word as receipts store it, with the feature a `--no-feature` drops.
pub fn word_text(w: FiltersWord, l: &Bundle) -> String {
    let mut t = format!("filters={}", w.names().join("|"));
    if let Some(f) = so(&l.no_feature) {
        t.push_str(&format!(" no_feature={f}"));
    }
    t
}

pub fn bundle(ctx: &mut Ctx, l: &Bundle) -> Result<Ran> {
    let word = word_of(l)?;
    let text = word_text(word, l);
    let target = s(&l.target);
    let job = ctx.journal.as_ref().map(|j| j.job(&target)).transpose()?.flatten();
    if let Some(row) = job {
        return bundle_job(ctx, &row, word, &text);
    }
    let mut ran = Ran::named(format!("bundle {target}"));
    let spec = readings::inline_spec("bundle", &target)?;
    let sd = readings::spec_digest(&spec, &text);
    let key = IndexKey::of_root(ctx.root(), &spec.scope)?.0;
    ran.spec_digest = Some(sd.clone());
    ran.index_key = Some(key.clone());
    ran.say(format!("filters: {text} (word {:#04x}, checked by flaga_solver)", word.raw()));
    if let Some(j) = ctx.journal.as_ref() {
        let (d, prev) = decide::same_search(j, &sd, &key, l.again)?;
        ran.say(d.said());
        if let (Some((row, _)), false) = (prev, l.again) {
            ran.reused_seq = Some(row.seq);
            ran.outputs = vec![row.page.clone()];
            ran.say(format!("reused: {} (bundle #{})", row.page, row.seq));
            return Ok(ran);
        }
    }
    let no_feature = so(&l.no_feature);
    let b = {
        let ws = ctx.workspace(&spec.scope, Resolving::Free)?;
        match &no_feature {
            Some(f) => custodian_kit::bundle::bundle_without_features(ws, &target, word, &[f.as_str()])?,
            None => custodian_kit::api::bundle(ws, &target, word)?,
        }
    };
    let rel = ctx.rel(&b.file);
    ran.say(format!("bundled {target}: {} files, {} items held redacted, {} dropped, {}", b.files, b.held, b.dropped.len(), b.digest));
    ran.say(format!("bundle: {rel}"));
    ran.outputs = vec![rel.clone()];
    if ctx.mode.project().is_some() {
        ran.reading = Some(ReadingRow {
            seq: 0,
            name: format!("bundle-{}", readings::slug(&target)),
            kind: "bundle".into(),
            target: target.clone(),
            spec_digest: sd,
            index_key: key,
            word: text,
            stamp: crate::journal::stamp(),
            page: rel.clone(),
            record: rel,
            reading_digest: b.digest,
            job: None,
        });
    }
    Ok(ran)
}

/// A closed job's bundle with what a bare `custodian bundle <job>` takes: secret items held redacted.
pub fn bundle_job_by_default(ctx: &mut Ctx, row: &crate::journal::JobRow) -> Result<Ran> {
    let l = Bundle { target: row.id.clone().into(), ..Bundle::default() };
    let word = word_of(&l)?;
    let text = word_text(word, &l);
    bundle_job(ctx, row, word, &text)
}

fn bundle_job(ctx: &mut Ctx, row: &crate::journal::JobRow, word: FiltersWord, text: &str) -> Result<Ran> {
    let mut ran = Ran::named(format!("bundle {}", row.id));
    let journal = ctx.journal()?;
    let receipts: Vec<(i64, String)> = journal.receipts_of_job(&row.id)?.into_iter().map(|r| (r.seq, serde_json::to_string_pretty(&r).unwrap_or_default())).collect();
    let folder = ctx.root().join(&row.folder);
    let title = format!("{}-{}", readings::slug(&row.name), crate::journal::stamp());
    let reveal = word.reveals();
    let (file, digest, held) = custodian_kit::job_bundle::bundle_job(ctx.root(), &folder, &title, &receipts, reveal)?;
    ran.say(format!("filters: {text}"));
    ran.say(format!("bundled job {}: {} receipts, {held} windows held redacted{}, {digest}", row.id, receipts.len(), if reveal { " (revealed)" } else { "" }));
    ran.say(format!("bundle: {} (custodian job import it in another project)", ctx.rel(&file)));
    ran.outputs = vec![ctx.rel(&file)];
    ran.job = Some(row.id.clone());
    Ok(ran)
}

//! custodian: the command line of custodian_kit.
//!
//! Every subcommand is a line declared once with simple_impl_derive over xccute ([`surface`]); every run of a line
//! goes through xccute's runtime and leaves a receipt ([`facade`]), kept in a SQLite journal ([`journal`]) inside an
//! initialized `.custodian/` folder whose parts are declared with pathgen ([`layout`]); with no folder at or above
//! the start, a run is free mode and nothing is stored ([`mode`]). Readings come from custodian_kit
//! ([`readings`]); jobs are folders ([`jobs`]) run by the runner (scope for a job's steps, waker_protocol for the
//! queue with checkpoints, [`runtime`]) and watched by the observer set ([`observe`]); the resident answers on an
//! owner-only socket ([`serve`]). [`testing`], [`maint`], [`diag`], [`bundling`] and [`install`] are the rest of
//! the lines. Refusals are one liaise table ([`codes`]); option sets are big-bad-bool words ([`words`]); every
//! digest is stable_digest over one canonical form ([`digest`]); the gates are xccute decisions ([`decide`]).

#![deny(unsafe_code)]

pub mod bundling;
pub mod cli;
pub mod codes;
pub mod counting;
pub mod decide;
pub mod diag;
pub mod digest;
pub mod facade;
pub mod history;
pub mod install;
pub mod jobs;
pub mod journal;
pub mod layout;
pub mod maint;
pub mod mode;
pub mod observe;
pub mod readings;
pub mod runtime;
pub mod serve;
pub mod surface;
pub mod testing;
pub mod words;

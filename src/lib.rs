//! Decision-facing contracts for xccute.
//!
//! `xccute_decisions` owns the source-level language for guided decisions:
//! questions to ask, observation tool responsibilities, optional/required path
//! steps, and compact evidence expectations. The runtime still owns execution,
//! material verification, connector receipts, and trace receipts. This crate is
//! the place where NodePlan/denv-style connectors can describe *why* a process
//! asks for a check before deciding what to do next.

pub mod context_pack;
pub mod context_request;
pub mod guide;
pub mod observation_tool;
pub mod path;
pub mod prelude;
pub mod runbook;
pub mod runbook_record;
pub mod runbook_journal;

pub use context_pack::*;
pub use context_request::*;
pub use guide::*;
pub use observation_tool::*;
pub use path::*;
pub use prelude::*;
pub use runbook::*;
pub use runbook_record::*;
pub use runbook_journal::*;

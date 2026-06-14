//! `eos-sim` — a deterministic, single-threaded scheduling simulator for the
//! Eos learning-augmented build scheduler (ADR-0004, `docs/specs/eos-scheduler.md`).
//!
//! The simulator is a standalone analysis tool: it consumes a plain data
//! [`trace`] (plan DAG + worker pool) and evaluates the entry-point coarsening
//! heuristic variants H1–H4 and PEFT dispatch under bounded dispatch windows.
//! It has **no dependency on any `eos` runtime crate** — it is the compensating
//! control that makes the scheduling heuristics, which are invisible to the
//! formal TLA+/Lean tracks, empirically measurable (campaign finding F14).
//!
//! Determinism is a first-class property: a fixed `--seed` reproduces
//! byte-identical metrics output (see the integration tests).

pub mod trace;

pub use trace::{Trace, TraceEdge, TraceError, TraceNode, WorkerSpec};

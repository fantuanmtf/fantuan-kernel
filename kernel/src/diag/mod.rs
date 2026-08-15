//! Diagnostics framework v1 (DESIGN.md §6).
//!
//! Every check is one Rust module returning a Severity; the runner prints a
//! report per stage. Iron rule: every check is READ-ONLY — a rescue kernel
//! never writes what it inspects. The precise low-level operations inside the
//! checks (CPUID, RDMSR, MMIO) are inline assembly; the logic is plain Rust
//! (DESIGN.md §2).

use core::fmt::Write;

use crate::serial::{self, Serial};

pub mod cpu;
pub mod gpu;
pub mod ram;
pub mod storage;

#[derive(Clone, Copy, PartialEq, PartialOrd)]
pub enum Severity {
    Ok,
    Warning,
    Critical,
}

pub struct Check {
    pub name: &'static str,
    pub run: fn(&mut Serial) -> Severity,
}

fn sev_name(s: Severity) -> &'static str {
    match s {
        Severity::Ok => "ok",
        Severity::Warning => "warning",
        Severity::Critical => "critical",
    }
}

/// Run one stage of checks; returns false when anything is critical.
pub fn run_stage(stage: &str, checks: &[Check]) -> bool {
    let mut s = Serial::new(serial::COM1);
    let _ = writeln!(s, "diag: stage {} — {} checks", stage, checks.len());
    let mut worst = Severity::Ok;
    for c in checks {
        let sev = (c.run)(&mut s);
        let _ = writeln!(s, "  [{}] {}", sev_name(sev), c.name);
        if sev > worst {
            worst = sev;
        }
    }
    let _ = writeln!(s, "diag: stage {} summary: {}", stage, sev_name(worst));
    worst != Severity::Critical
}

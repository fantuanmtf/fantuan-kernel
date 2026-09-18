//! Diagnostics framework v1 (DESIGN.md §6).
//!
//! Every check is one Rust module returning a Severity; the runner prints a
//! report per stage. Iron rule: every check is READ-ONLY — a rescue kernel
//! never writes what it inspects. The precise low-level operations inside the
//! checks (CPUID, RDMSR, MMIO) live in each arch's own checks; the shared
//! ones log through the kernel-core `Log` sink.

use core::fmt::Write;

use crate::log::Log;

pub mod diskhealth;
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
    pub run: fn(&mut Log) -> Severity,
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
    let mut s = Log::new();
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

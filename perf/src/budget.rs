//! Budgets and verdicts. Pure logic, unit tested.

use serde::Serialize;

/// The hard budgets from the project brief.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Budgets {
    pub idle_ram_mb: f64,
    pub idle_cpu_pct: f64,
    pub active_cpu_pct: f64,
    pub cold_start_ms: f64,
    pub installer_mb: f64,
}

impl Default for Budgets {
    fn default() -> Self {
        Self { idle_ram_mb: 60.0, idle_cpu_pct: 0.5, active_cpu_pct: 5.0, cold_start_ms: 1500.0, installer_mb: 20.0 }
    }
}

/// Cores of the reference low-end machine. CPU percentages are reported as a
/// share of that machine's total capacity (Task Manager convention), so the
/// number does not depend on how many cores the CI runner has.
pub const REFERENCE_CORES: f64 = 2.0;

/// CPU percentage of the reference machine given CPU time used over a wall interval.
pub fn cpu_pct(cpu_ms: u64, wall_ms: u64) -> f64 {
    if wall_ms == 0 {
        return 0.0;
    }
    cpu_ms as f64 / wall_ms as f64 / REFERENCE_CORES * 100.0
}

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub metric: &'static str,
    pub value: f64,
    pub budget: f64,
    pub unit: &'static str,
    pub pass: bool,
}

impl Check {
    pub fn new(metric: &'static str, value: f64, budget: f64, unit: &'static str) -> Self {
        Self { metric, value, budget, unit, pass: value <= budget }
    }
}

pub fn markdown(title: &str, checks: &[Check], notes: &[String]) -> String {
    let mut s = format!("### {title}\n\n| Metric | Measured | Budget | Result |\n|---|---|---|---|\n");
    for c in checks {
        let mark = if c.pass { "✅ pass" } else { "❌ over" };
        s += &format!("| {} | {:.2} {} | {:.2} {} | {} |\n", c.metric, c.value, c.unit, c.budget, c.unit, mark);
    }
    for n in notes {
        s += &format!("\n- {n}");
    }
    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_pct_normalises_to_reference_machine() {
        // One full core for the whole interval = 50% of a dual-core machine.
        assert_eq!(cpu_pct(1000, 1000), 50.0);
        assert_eq!(cpu_pct(60, 60_000), 0.05);
        assert_eq!(cpu_pct(5, 0), 0.0);
    }

    #[test]
    fn check_passes_at_budget() {
        assert!(Check::new("x", 60.0, 60.0, "MB").pass);
        assert!(!Check::new("x", 60.1, 60.0, "MB").pass);
    }

    #[test]
    fn markdown_lists_every_check() {
        let md = markdown("T", &[Check::new("Idle RAM", 70.0, 60.0, "MB")], &["note".into()]);
        assert!(md.contains("| Idle RAM | 70.00 MB | 60.00 MB | ❌ over |"));
        assert!(md.contains("- note"));
    }
}

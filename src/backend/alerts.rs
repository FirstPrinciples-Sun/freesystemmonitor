//! Threshold alerts with desktop notifications.
//!
//! Evaluated once per poll tick. Notifications fire only on the rising
//! edge (tracked by key in main.rs), so a sustained hot CPU notifies once.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alert {
    /// stable key for edge detection: "cpu", "mem", "disk", "temp", "swap"
    pub key: &'static str,
    /// true = critical (red), false = warning (amber)
    pub critical: bool,
    pub text: String,
}

pub struct AlertInput {
    pub cpu: f32,
    pub mem_pct: f32,
    pub disk_pct: f32,
    pub swap_pct: f32,
    pub temp_c: Option<i64>,
}

pub fn evaluate(inp: &AlertInput) -> Vec<Alert> {
    let mut out = vec![];
    let mut push = |key: &'static str, crit_at: f32, warn_at: f32, v: f32, fmt: &dyn Fn(f32) -> String| {
        if v >= crit_at {
            out.push(Alert { key, critical: true, text: fmt(v) });
        } else if v >= warn_at {
            out.push(Alert { key, critical: false, text: fmt(v) });
        }
    };
    push("cpu", 90.0, 75.0, inp.cpu, &|v| format!("CPU at {v:.0}%"));
    push("mem", 90.0, 75.0, inp.mem_pct, &|v| {
        format!("Memory at {v:.0}%")
    });
    push("disk", 90.0, 80.0, inp.disk_pct, &|v| {
        format!("Disk at {v:.0}%")
    });
    push("swap", 90.0, 70.0, inp.swap_pct, &|v| {
        format!("Swap at {v:.0}%")
    });
    if let Some(t) = inp.temp_c {
        let v = t as f32;
        push("temp", 90.0, 80.0, v, &|x| format!("CPU temp at {x:.0}°C"));
    }
    out
}

/// Best-effort desktop notification, fire-and-forget.
pub fn notify(critical: bool, title: &str, body: &str) {
    let level = if critical { "critical" } else { "normal" };
    let _ = std::process::Command::new("notify-send")
        .args(["-a", "FreeMon", "-u", level, title, body])
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quiet() -> AlertInput {
        AlertInput { cpu: 10.0, mem_pct: 40.0, disk_pct: 35.0, swap_pct: 0.0, temp_c: Some(50) }
    }

    #[test]
    fn no_alerts_when_quiet() {
        assert!(evaluate(&quiet()).is_empty());
    }

    #[test]
    fn thresholds_and_levels() {
        let a = evaluate(&AlertInput { cpu: 95.0, mem_pct: 80.0, disk_pct: 91.0, swap_pct: 0.0, temp_c: Some(85) });
        let crit: Vec<&str> = a.iter().filter(|x| x.critical).map(|x| x.key).collect();
        let warn: Vec<&str> = a.iter().filter(|x| !x.critical).map(|x| x.key).collect();
        assert!(crit.contains(&"cpu"));
        assert!(crit.contains(&"disk"));
        assert!(warn.contains(&"mem"));
        assert!(warn.contains(&"temp"));
        assert_eq!(a.len(), 4);
    }
}

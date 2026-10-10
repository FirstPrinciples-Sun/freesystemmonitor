/// Scan a journal tail for problem lines. Returns (errors, warnings, total_lines).
pub fn analyze(tail: &str) -> (i32, i32, i32) {
    let mut err = 0;
    let mut warn = 0;
    let mut total = 0;
    for line in tail.lines() {
        total += 1;
        let l = line.to_lowercase();
        if l.contains("error")
            || l.contains("failed")
            || l.contains("failure")
            || l.contains("fatal")
            || l.contains("critical")
        {
            err += 1;
        } else if l.contains("warn")
            || l.contains("denied")
            || l.contains("timeout")
            || l.contains("retry")
        {
            warn += 1;
        }
    }
    (err, warn, total)
}

pub fn tail_logs() -> String {
    for args in [
        vec!["--user", "-n", "200", "--no-pager", "-e"],
        vec!["-n", "200", "--no-pager", "-e"],
    ] {
        if let Ok(o) = std::process::Command::new("journalctl")
            .args(&args)
            .output()
        {
            if o.status.success() {
                let t = String::from_utf8_lossy(&o.stdout).into_owned();
                if !t.trim().is_empty() {
                    return pengendalian(&t);
                }
            }
        }
    }
    "No journal available. Try running inside a systemd session.".into()
}

fn pengendalian(t: &str) -> String {
    // keep last ~12k chars so the UI stays fast
    if t.len() > 12_000 {
        t[t.len() - 12_000..].to_string()
    } else {
        t.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::analyze;

    #[test]
    fn counts_severity() {
        let tail = "ok line\nFailed to start foo\nwarning: denied by policy\nanother ok\nERROR disk full\n";
        let (e, w, t) = analyze(tail);
        assert_eq!((e, w, t), (2, 1, 5));
    }
}

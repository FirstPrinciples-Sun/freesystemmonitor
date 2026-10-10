#[derive(Clone)]
pub struct SvcInfo {
    pub name: String,
    pub desc: String,
    pub state: String,
    pub active: bool,
}

fn unescape_unit(s: &str) -> String {
    // systemctl escapes '-' as \x2d etc. in unit names
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'x') {
            chars.next();
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() == 2 {
                if let Ok(b) = u8::from_str_radix(&hex, 16) {
                    out.push(b as char);
                    continue;
                }
            }
            out.push('\\');
            out.push('x');
            out.push_str(&hex);
            continue;
        }
        out.push(c);
    }
    out
}

pub fn list_user_services() -> Vec<SvcInfo> {
    let out = std::process::Command::new("systemctl")
        .args([
            "--user",
            "list-units",
            "--type=service",
            "--all",
            "--no-legend",
            "--no-pager",
        ])
        .output();
    let mut v = Vec::new();
    if let Ok(o) = out {
        v = parse_units(&String::from_utf8_lossy(&o.stdout));
    }
    if v.is_empty() {
        // fallback: system services (read-only view still useful)
        if let Ok(o) = std::process::Command::new("systemctl")
            .args([
                "list-units",
                "--type=service",
                "--all",
                "--no-legend",
                "--no-pager",
            ])
            .output()
        {
            v = parse_units(&String::from_utf8_lossy(&o.stdout));
        }
    }
    v
}

fn parse_units(txt: &str) -> Vec<SvcInfo> {
    let mut v = Vec::new();
    for line in txt.lines().take(120) {
        // lines may start with "● " (failed) or whitespace
        let line = line.trim_start_matches(['●', '*', ' ', '\t']);
        // UNIT LOAD ACTIVE SUB DESCRIPTION
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 && parts[0].ends_with(".service") {
            v.push(SvcInfo {
                name: unescape_unit(parts[0]),
                desc: if parts.len() > 4 {
                    parts[4..].join(" ")
                } else {
                    String::new()
                },
                state: parts[3].to_string(),
                active: parts[2] == "active",
            });
        }
    }
    v
}

pub fn service_action(name: &str, action: &str) -> anyhow::Result<String> {
    // try --user first, then system (may need polkit)
    let r = std::process::Command::new("systemctl")
        .args(["--user", action, name])
        .output()?;
    if r.status.success() {
        return Ok(format!("{name}: {action} ok"));
    }
    let r2 = std::process::Command::new("systemctl")
        .args([action, name])
        .output()?;
    if r2.status.success() {
        Ok(format!("{name}: {action} ok (system)"))
    } else {
        anyhow::bail!("{}", String::from_utf8_lossy(&r2.stderr).trim())
    }
}

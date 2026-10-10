#[derive(Clone)]
pub struct AutostartEntry {
    pub name: String,
    pub file: String,
    pub enabled: bool,
}

fn parse_desktop(path: &std::path::Path) -> Option<(String, bool)> {
    let txt = std::fs::read_to_string(path).ok()?;
    let mut name = path.file_stem()?.to_string_lossy().into_owned();
    let mut hidden = false;
    for line in txt.lines() {
        if let Some(v) = line.strip_prefix("Name=") {
            if !v.trim().is_empty() {
                name = v.trim().to_string();
            }
        }
        if line.trim() == "Hidden=true" {
            hidden = true;
        }
    }
    Some((name, !hidden))
}

pub fn list_autostart() -> Vec<AutostartEntry> {
    let mut v = Vec::new();
    let mut dirs = vec![];
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(format!("{home}/.config/autostart"));
    }
    dirs.push("/etc/xdg/autostart".into());
    for d in dirs {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().map(|x| x == "desktop").unwrap_or(false) {
                    if let Some((name, enabled)) = parse_desktop(&p) {
                        v.push(AutostartEntry {
                            name,
                            file: p.to_string_lossy().into_owned(),
                            enabled,
                        });
                    }
                }
            }
        }
    }
    v.sort_by(|a, b| a.name.cmp(&b.name));
    v
}

pub fn set_autostart_enabled(file: &str, enabled: bool) -> anyhow::Result<String> {
    // Only toggle user-local copy. If file is system-wide, copy it to ~/.config/autostart first.
    let home = std::env::var("HOME")?;
    let user_dir = format!("{home}/.config/autostart");
    std::fs::create_dir_all(&user_dir)?;
    let src = std::path::Path::new(file);
    let fname = src
        .file_name()
        .ok_or_else(|| anyhow::anyhow!("bad filename"))?;
    let dest = std::path::Path::new(&user_dir).join(fname);
    let target = if file.starts_with(&user_dir) {
        src.to_path_buf()
    } else {
        if !dest.exists() {
            std::fs::copy(src, &dest)?;
        }
        dest
    };
    let mut txt = std::fs::read_to_string(&target).unwrap_or_default();
    // strip existing Hidden= lines
    let mut lines: Vec<String> = txt
        .lines()
        .filter(|l| !l.starts_with("Hidden="))
        .map(|s| s.to_string())
        .collect();
    if !enabled {
        if !lines.iter().any(|l| l.trim() == "[Desktop Entry]") {
            lines.insert(0, "[Desktop Entry]".into());
        }
        lines.push("Hidden=true".into());
    }
    txt = lines.join("\n") + "\n";
    std::fs::write(&target, txt)?;
    Ok(format!(
        "{} {}",
        target.display(),
        if enabled { "enabled" } else { "disabled" }
    ))
}

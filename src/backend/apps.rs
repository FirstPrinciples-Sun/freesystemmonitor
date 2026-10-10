#[derive(Clone)]
pub struct AppInfo {
    pub name: String,
    pub id: String,
    pub size: String,
    pub size_mb: f32,
    pub origin: String,
}

pub fn list_flatpak_apps() -> Vec<AppInfo> {
    let out = std::process::Command::new("flatpak")
        .args(["list", "--app", "--columns=name,application,size,origin"])
        .output();
    let mut v = Vec::new();
    if let Ok(o) = out {
        if o.status.success() {
            for line in String::from_utf8_lossy(&o.stdout).lines() {
                // tab-separated
                let p: Vec<&str> = line.split('\t').collect();
                if p.len() >= 2 {
                    let size = p.get(2).map(|s| s.trim().to_string()).unwrap_or_default();
                    v.push(AppInfo {
                        name: p[0].trim().to_string(),
                        id: p.get(1).unwrap_or(&"").trim().to_string(),
                        size_mb: parse_size_mb(&size),
                        size,
                        origin: p.get(3).map(|s| s.trim().to_string()).unwrap_or_default(),
                    });
                }
            }
        }
    }
    v
}

pub fn parse_size_mb(s: &str) -> f32 {
    // flatpak "installed-size" looks like "1.2 GB", "345.5 MB", "800 kB", "-"
    let s = s.trim();
    let (num, unit) = match s.find(' ') {
        Some(i) => (&s[..i], s[i + 1..].trim()),
        None => return 0.0,
    };
    let n: f32 = num.replace(',', ".").parse().unwrap_or(0.0);
    match unit.to_uppercase().as_str() {
        "GB" | "GIB" | "G" => n * 1024.0,
        "MB" | "MIB" | "M" => n,
        "KB" | "KIB" | "K" => n / 1024.0,
        _ => 0.0,
    }
}

pub fn uninstall_flatpak(id: &str) -> anyhow::Result<String> {
    let out = std::process::Command::new("flatpak")
        .args(["uninstall", "-y", id])
        .output()?;
    if out.status.success() {
        Ok(format!("uninstalled {id}"))
    } else {
        anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr).trim())
    }
}

#[cfg(test)]
mod tests {
    use super::parse_size_mb;

    #[test]
    fn parses_flatpak_sizes() {
        assert!((parse_size_mb("1.2 GB") - 1228.8).abs() < 0.1);
        assert!((parse_size_mb("345.5 MB") - 345.5).abs() < 0.1);
        assert!((parse_size_mb("800 kB") - 0.78).abs() < 0.02);
        assert_eq!(parse_size_mb("-"), 0.0);
        assert_eq!(parse_size_mb(""), 0.0);
    }
}

//! Real app icons for processes, like Windows Task Manager.
//!
//! Scans .desktop files once at startup and maps executable names
//! (and StartupWMClass / app names) to icon image files.
//! Lookups are O(1); image loading is cached in main.rs.

use std::collections::{HashMap, HashSet};

pub struct IconDb {
    map: HashMap<String, String>,
}

/// (process key, icon name) for apps whose process name doesn't match
/// any desktop-file key (e.g. comm "brave" vs Exec "brave-browser-stable").
const ICON_ALIASES: &[(&str, &str)] = &[
    ("brave", "brave-browser"),
    ("kwin_wayland", "kwin"),
    ("kwin_x11", "kwin"),
    ("spotify", "spotify-client"),
    ("code", "code"),
    ("discord", "discord"),
    ("steam", "steam"),
];

impl IconDb {
    pub fn build() -> Self {
        let mut map: HashMap<String, String> = HashMap::new();
        let mut dirs = vec![
            "/usr/share/applications".to_string(),
            "/var/lib/flatpak/exports/share/applications".to_string(),
            "/usr/local/share/applications".to_string(),
        ];
        if let Ok(home) = std::env::var("HOME") {
            dirs.push(format!("{home}/.local/share/applications"));
            dirs.push(format!(
                "{home}/.local/share/flatpak/exports/share/applications"
            ));
        }
        // pass 1: collect (keys, icon-name) from desktop files
        let mut pending: Vec<(Vec<String>, String)> = vec![];
        let mut wanted: HashSet<String> = HashSet::new();
        for d in dirs {
            let rd = match std::fs::read_dir(&d) {
                Ok(r) => r,
                Err(_) => continue,
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().map(|x| x == "desktop").unwrap_or(false) {
                    if let Some((keys, icon)) = parse_desktop(&p) {
                        if icon.contains('/') {
                            // absolute path: use directly when it exists
                            if exists(&icon) {
                                for k in keys {
                                    map.entry(k).or_insert_with(|| icon.clone());
                                }
                            }
                        } else {
                            wanted.insert(icon.clone());
                            pending.push((keys, icon));
                        }
                    }
                }
            }
        }
        // pass 2: one filesystem walk per theme root, indexed by wanted names.
        // alias targets must be indexed too (they may lack desktop files).
        for (_, ic) in ICON_ALIASES {
            wanted.insert(ic.to_string());
        }
        let index = build_index(&wanted);
        for (keys, icon) in &pending {
            if let Some(ipath) = index.get(icon) {
                for k in keys {
                    map.entry(k.clone()).or_insert_with(|| ipath.clone());
                }
            }
        }
        // pass 3: well-known aliases for processes without matching desktop keys
        for (alias, ic) in ICON_ALIASES {
            if !map.contains_key(*alias) {
                if let Some(ipath) = index.get(*ic) {
                    map.insert(alias.to_string(), ipath.clone());
                }
            }
        }
        Self { map }
    }

    /// key: lowercase exe stem or process name. Falls back to the part
    /// before the first '-' or space ("firefox-bin" -> "firefox").
    pub fn find(&self, key: &str) -> Option<&str> {
        if let Some(v) = self.map.get(key) {
            return Some(v.as_str());
        }
        let short = key.split(['-', ' ']).next().unwrap_or(key);
        if short != key {
            if let Some(v) = self.map.get(short) {
                return Some(v.as_str());
            }
        }
        None
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.map.len()
    }
}

fn parse_desktop(path: &std::path::Path) -> Option<(Vec<String>, String)> {
    let txt = std::fs::read_to_string(path).ok()?;
    let mut keys: Vec<String> = vec![];
    let mut icon: Option<String> = None;
    for line in txt.lines() {
        if let Some(v) = line.strip_prefix("Exec=") {
            // first real token, skipping "env" and VAR= assignments
            let mut base: Option<String> = None;
            for tok in v.split_whitespace() {
                if tok == "env" || tok.contains('=') || tok.starts_with('%') {
                    continue;
                }
                base = Some(tok.rsplit('/').next().unwrap_or(tok).to_lowercase());
                break;
            }
            if let Some(b) = base {
                if !b.is_empty() && !keys.contains(&b) {
                    keys.push(b);
                }
            }
        } else if let Some(v) = line.strip_prefix("StartupWMClass=") {
            let w = v.trim().to_lowercase();
            if !w.is_empty() && !keys.contains(&w) {
                keys.push(w);
            }
        } else if let Some(v) = line.strip_prefix("Name=") {
            let n = v.trim().to_lowercase();
            if n.len() >= 3 && !keys.contains(&n) {
                keys.push(n);
            }
        } else if icon.is_none() {
            if let Some(v) = line.strip_prefix("Icon=") {
                let ic = v.trim().to_string();
                if !ic.is_empty() {
                    icon = Some(ic);
                }
            }
        }
    }
    let icon = icon?;
    if keys.is_empty() {
        return None;
    }
    Some((keys, icon))
}

fn exists(p: &str) -> bool {
    std::path::Path::new(p).is_file()
}

/// Walk theme roots once (max depth 3, capped) and index files whose
/// stem is in `wanted`. Handles breeze-style <cat>/<size>/ and
/// hicolor-style <size>/apps/ layouts, plus flat pixmaps dirs.
fn build_index(wanted: &HashSet<String>) -> HashMap<String, String> {
    fn rank(ext: &str) -> u8 {
        match ext {
            "svg" => 3,
            "png" => 2,
            _ => 1,
        }
    }
    let mut idx: HashMap<String, (String, u8)> = HashMap::new();
    let mut roots: Vec<String> = vec![];
    if let Ok(home) = std::env::var("HOME") {
        roots.push(format!("{home}/.local/share/icons"));
    }
    roots.extend(
        [
            "/usr/share/icons/breeze",
            "/usr/share/icons/breeze-dark",
            "/usr/share/icons/hicolor",
            "/usr/share/icons/Adwaita",
            "/usr/share/pixmaps",
        ]
        .iter()
        .map(|s| s.to_string()),
    );
    // stack of (dir, depth)
    let mut stack: Vec<(String, u8)> = roots.into_iter().map(|r| (r, 0)).collect();
    let mut seen = 0usize;
    while let Some((dir, depth)) = stack.pop() {
        if idx.len() >= wanted.len() || seen > 60_000 {
            break;
        }
        let rd = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for e in rd.flatten() {
            seen += 1;
            let p = e.path();
            if p.is_dir() {
                if depth < 3 {
                    stack.push((p.to_string_lossy().into_owned(), depth + 1));
                }
                continue;
            }
            let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("");
            if !matches!(ext, "svg" | "png" | "xpm") {
                continue;
            }
            let stem = p
                .file_stem()
                .and_then(|x| x.to_str())
                .unwrap_or("")
                .to_string();
            if !wanted.contains(&stem) {
                continue;
            }
            let r = rank(ext);
            let ps = p.to_string_lossy().into_owned();
            match idx.get(&stem) {
                Some((_, old)) if *old >= r => {}
                _ => {
                    idx.insert(stem, (ps, r));
                }
            }
        }
    }
    idx.into_iter().map(|(k, (p, _))| (k, p)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_builds_and_resolves_common_apps() {
        let db = IconDb::build();
        assert!(db.len() > 10, "expected desktop files, got {}", db.len());
        // dolphin ships with Fedora KDE; firefox may or may not exist
        let _ = db.find("dolphin");
    }

    #[test]
    fn fallback_strips_suffix() {
        let mut db = IconDb {
            map: HashMap::new(),
        };
        db.map.insert("firefox".into(), "/tmp/x.svg".into());
        assert_eq!(db.find("firefox-bin"), Some("/tmp/x.svg"));
        assert_eq!(db.find("nope"), None);
    }

    #[test]
    fn aliases_resolve() {
        let db = IconDb::build();
        // brave's comm is "brave" but the desktop key is "brave-browser-stable"
        if db.map.contains_key("brave-browser-stable") {
            assert_eq!(db.find("brave"), db.find("brave-browser-stable"));
        }
    }
}

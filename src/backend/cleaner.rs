use super::system::fmt_bytes;

fn dir_size(path: &str) -> u64 {
    let out = std::process::Command::new("du")
        .args(["-sb", path])
        .output();
    if let Ok(o) = out {
        let txt = String::from_utf8_lossy(&o.stdout);
        if let Some(n) = txt.split_whitespace().next() {
            return n.parse::<u64>().unwrap_or(0);
        }
    }
    0
}

/// (label, bytes) for the Cleaner bar chart.
pub fn sizes() -> Vec<(String, u64)> {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    let journal = journal_bytes();
    vec![
        ("User cache".into(), dir_size(&format!("{home}/.cache"))),
        (
            "Thumbnails".into(),
            dir_size(&format!("{home}/.cache/thumbnails")),
        ),
        (
            "Trash".into(),
            dir_size(&format!("{home}/.local/share/Trash")),
        ),
        ("Journal".into(), journal),
    ]
}

fn journal_bytes() -> u64 {
    // "Archived and active journals take up 1.6G in the file system."
    let out = std::process::Command::new("journalctl")
        .args(["--disk-usage"])
        .output();
    let txt = out
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let pos = match txt.find("take up ") {
        Some(i) => i + 8,
        None => return 0,
    };
    let rest = txt[pos..].trim();
    let end = rest
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == ','))
        .unwrap_or(rest.len());
    let num: f64 = rest[..end].replace(',', ".").parse().unwrap_or(0.0);
    let unit = rest[end..]
        .chars()
        .next()
        .unwrap_or('?')
        .to_ascii_uppercase();
    let mult = match unit {
        'T' => 1_099_511_627_776.0,
        'G' => 1_073_741_824.0,
        'M' => 1_048_576.0,
        'K' => 1024.0,
        _ => 1.0,
    };
    (num * mult) as u64
}

pub fn cleaner_summary() -> String {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".into());
    let cache = dir_size(&format!("{home}/.cache"));
    let thumbs = dir_size(&format!("{home}/.cache/thumbnails"));
    let trash = dir_size(&format!("{home}/.local/share/Trash"));
    let journal = std::process::Command::new("journalctl")
        .args(["--disk-usage"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    format!(
        "Cache: {}   Thumbnails: {}   Trash: {}\nJournal: {}\n\nTip: cleaning needs no root for your own cache/trash. Journal vacuum may ask for sudo.",
        fmt_bytes(cache),
        fmt_bytes(thumbs),
        fmt_bytes(trash),
        if journal.is_empty() { "unknown".into() } else { journal }
    )
}

pub fn clean_kind(kind: &str) -> anyhow::Result<String> {
    let home = std::env::var("HOME")?;
    match kind {
        "trash" => {
            let p = format!("{home}/.local/share/Trash/files");
            let _ = std::fs::remove_dir_all(format!("{home}/.local/share/Trash/files"));
            let _ = std::fs::remove_dir_all(format!("{home}/.local/share/Trash/info"));
            std::fs::create_dir_all(&p)?;
            Ok("trash emptied".into())
        }
        "thumbs" => {
            let p = format!("{home}/.cache/thumbnails");
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p)?;
            Ok("thumbnails cleared".into())
        }
        "journal" => {
            let out = std::process::Command::new("journalctl")
                .args(["--vacuum-size=200M"])
                .output()?;
            if out.status.success() {
                Ok("journal vacuumed to 200M".into())
            } else {
                anyhow::bail!("need root? {}", String::from_utf8_lossy(&out.stderr).trim())
            }
        }
        _ => {
            // safe cache subset only
            for sub in ["mozilla", "thumbnails", "pip", "uv", "cargo/registry/cache"] {
                let _ = std::fs::remove_dir_all(format!("{home}/.cache/{sub}"));
            }
            Ok("safe cache subset cleared".into())
        }
    }
}

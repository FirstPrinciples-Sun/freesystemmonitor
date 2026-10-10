pub fn cpu_temp() -> String {
    // try hwmon then thermal zones
    if let Ok(rd) = std::fs::read_dir("/sys/class/hwmon") {
        let mut best: Option<i64> = None;
        for e in rd.flatten() {
            let base = e.path();
            for i in 1..=8 {
                let p = base.join(format!("temp{i}_input"));
                if let Ok(t) = std::fs::read_to_string(&p) {
                    if let Ok(milli) = t.trim().parse::<i64>() {
                        let c = milli / 1000;
                        if (0..150).contains(&c) {
                            best = Some(best.map(|b| b.max(c)).unwrap_or(c));
                        }
                    }
                }
            }
        }
        if let Some(c) = best {
            return format!("{c}°C");
        }
    }
    if let Ok(rd) = std::fs::read_dir("/sys/class/thermal") {
        for e in rd.flatten() {
            let p = e.path().join("temp");
            if let Ok(t) = std::fs::read_to_string(&p) {
                if let Ok(milli) = t.trim().parse::<i64>() {
                    let c = if milli > 1000 { milli / 1000 } else { milli };
                    if (0..150).contains(&c) {
                        return format!("{c}°C");
                    }
                }
            }
        }
    }
    "n/a".into()
}

pub fn battery() -> String {
    for bat in ["BAT0", "BAT1"] {
        let cap = std::fs::read_to_string(format!("/sys/class/power_supply/{bat}/capacity"));
        let st = std::fs::read_to_string(format!("/sys/class/power_supply/{bat}/status"));
        if let Ok(c) = cap {
            let s = st.unwrap_or_default().trim().to_string();
            return format!("{}% {}", c.trim(), s);
        }
    }
    "AC / no battery".into()
}

fn short_gpu(s: &str) -> String {
    // "00:02.0 VGA compatible controller: Intel Corporation Raptor Lake-P [UHD Graphics] (rev 04)"
    // -> "Intel Raptor Lake-P [UHD Graphics]"
    let mut t = s.to_string();
    for prefix in [
        "VGA compatible controller: ",
        "3D controller: ",
        "Display controller: ",
    ] {
        if let Some(pos) = t.find(prefix) {
            t = t[pos + prefix.len()..].to_string();
            break;
        }
    }
    t = t.replace(" Corporation", "");
    if let Some(pos) = t.find(" (rev") {
        t.truncate(pos);
    }
    // drop leading PCI slot id like "00:02.0 "
    let t2 = t.trim().to_string();
    if let Some(sp) = t2.find(' ') {
        let head = &t2[..sp];
        if head
            .chars()
            .all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.')
            && head.contains(':')
        {
            return t2[sp + 1..].trim().to_string();
        }
    }
    t2
}

pub fn gpu_name() -> String {
    // lspci is the most reliable without NVML
    if let Ok(o) = std::process::Command::new("lspci").output() {
        let txt = String::from_utf8_lossy(&o.stdout);
        let mut gpus = vec![];
        for line in txt.lines() {
            let l = line.to_lowercase();
            if l.contains("vga") || l.contains("3d controller") || l.contains("display controller")
            {
                if let Some(colon) = line.find(':') {
                    gpus.push(line[colon + 1..].trim().to_string());
                }
            }
        }
        if !gpus.is_empty() {
            return short_gpu(&gpus.join(" / "));
        }
    }
    // fallback: /sys DRM cards
    if let Ok(rd) = std::fs::read_dir("/sys/class/drm") {
        let n = rd
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with("card"))
            .count();
        if n > 0 {
            return format!("{n} DRM card(s)");
        }
    }
    "Unknown GPU".into()
}

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

/// (percent, charging?) — None on desktops without a battery.
pub fn battery_pct() -> Option<(f32, bool)> {
    for bat in ["BAT0", "BAT1"] {
        let cap = std::fs::read_to_string(format!("/sys/class/power_supply/{bat}/capacity"));
        if let Ok(c) = cap {
            if let Ok(v) = c.trim().parse::<f32>() {
                let st = std::fs::read_to_string(format!("/sys/class/power_supply/{bat}/status"))
                    .unwrap_or_default();
                let charging = st.trim().eq_ignore_ascii_case("charging");
                return Some((v.clamp(0.0, 100.0), charging));
            }
        }
    }
    None
}

fn short_gpu(s: &str) -> String {
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

/// NVIDIA dedicated GPU info (single nvidia-smi call): name, usage%, temp°C, VRAM.
pub struct GpuInfo {
    pub name: String,
    pub usage: f32,
    pub temp: f32,
    pub vram: String,
}

pub fn nvidia_info() -> Option<GpuInfo> {
    let o = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,utilization.gpu,temperature.gpu,memory.used,memory.total",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    if !o.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&o.stdout)
        .lines()
        .next()?
        .to_string();
    let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
    if parts.len() < 5 {
        return None;
    }
    let used: f64 = parts[3].parse().unwrap_or(0.0);
    let total: f64 = parts[4].parse().unwrap_or(0.0);
    let vram = if total > 0.0 {
        format!("{:.1} / {:.0} GB VRAM", used / 1024.0, total / 1024.0)
    } else {
        String::new()
    };
    Some(GpuInfo {
        name: parts[0].to_string(),
        usage: parts[1].parse::<f32>().unwrap_or(-1.0),
        temp: parts[2].parse::<f32>().unwrap_or(-273.0),
        vram,
    })
}

/// Intel integrated GPU activity via frequency proxy.
/// Reads gt_act_freq_mhz / gt_max_freq_mhz / gt_min_freq_mhz from the
/// DRM card whose PCI vendor is Intel (0x8086). Returns (usage%, freq label).
pub struct IntelGpu {
    pub usage: f32,
    pub freq: String,
}

pub fn intel_gpu() -> Option<IntelGpu> {
    let rd = std::fs::read_dir("/sys/class/drm").ok()?;
    let mut cards: Vec<_> = rd
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().into_owned();
            n.starts_with("card") && n["card".len()..].chars().all(|c| c.is_ascii_digit())
        })
        .collect();
    cards.sort_by_key(|e| e.file_name());
    for c in cards {
        let vendor = std::fs::read_to_string(c.path().join("device/vendor")).unwrap_or_default();
        if vendor.trim() != "0x8086" {
            continue;
        }
        let read = |f: &str| {
            std::fs::read_to_string(c.path().join(f))
                .ok()
                .and_then(|t| t.trim().parse::<f32>().ok())
        };
        let (act, max, min) = (
            read("gt_act_freq_mhz")?,
            read("gt_max_freq_mhz")?,
            read("gt_min_freq_mhz")?,
        );
        if max <= min {
            return None;
        }
        let usage = ((act - min) / (max - min) * 100.0).clamp(0.0, 100.0);
        return Some(IntelGpu {
            usage,
            freq: format!("{:.1} GHz", act / 1000.0),
        });
    }
    None
}

/// GPU utilization 0..100, or None when it can't be measured.
/// Tries NVIDIA (nvidia-smi), then AMD (amdgpu busy_percent sysfs).
pub fn gpu_usage() -> Option<f32> {
    if let Ok(o) = std::process::Command::new("nvidia-smi")
        .args([
            "--query-gpu=utilization.gpu",
            "--format=csv,noheader,nounits",
        ])
        .output()
    {
        if o.status.success() {
            let txt = String::from_utf8_lossy(&o.stdout);
            if let Some(line) = txt.lines().next() {
                if let Ok(v) = line.trim().parse::<f32>() {
                    if (0.0..=100.0).contains(&v) {
                        return Some(v);
                    }
                }
            }
        }
    }
    // AMD: /sys/class/drm/cardN/device/gpu_busy_percent
    if let Ok(rd) = std::fs::read_dir("/sys/class/drm") {
        let mut cards: Vec<_> = rd
            .flatten()
            .filter(|e| {
                let n = e.file_name().to_string_lossy().into_owned();
                n.starts_with("card") && n["card".len()..].chars().all(|c| c.is_ascii_digit())
            })
            .collect();
        cards.sort_by_key(|e| e.file_name());
        for c in cards {
            let p = c.path().join("device/gpu_busy_percent");
            if let Ok(t) = std::fs::read_to_string(&p) {
                if let Ok(v) = t.trim().parse::<f32>() {
                    if (0.0..=100.0).contains(&v) {
                        return Some(v);
                    }
                }
            }
        }
    }
    None
}

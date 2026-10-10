//! Hardware sensors (no lm_sensors needed) and CPU frequency governor.
//! Reads /sys directly: hwmon temperatures/fans, cpufreq governors.

#[derive(Clone)]
pub struct SensorRow {
    /// e.g. "k10temp · Tctl" or "nct6775 · fan2"
    pub label: String,
    /// e.g. "46°C" or "1200 RPM"
    pub info: String,
    /// 0..100 for temperatures (vs 105°C ceiling), 0 for fans
    pub pct: f32,
}

pub fn read_sensors() -> Vec<SensorRow> {
    let mut out = vec![];
    let rd = match std::fs::read_dir("/sys/class/hwmon") {
        Ok(r) => r,
        Err(_) => return out,
    };
    let mut hwmons: Vec<_> = rd.flatten().collect();
    hwmons.sort_by_key(|e| e.file_name());
    for h in hwmons {
        let base = h.path();
        let chip = std::fs::read_to_string(base.join("name"))
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|_| "?".into());
        if chip == "acpitz" {
            // firmware pseudo-sensor, usually duplicates; keep anyway if real
        }
        for i in 1..=16u32 {
            let inp = base.join(format!("temp{i}_input"));
            let Ok(t) = std::fs::read_to_string(&inp) else {
                if i > 4 {
                    break;
                }
                continue;
            };
            let milli: i64 = match t.trim().parse() {
                Ok(v) => v,
                Err(_) => continue,
            };
            let c = milli as f32 / 1000.0;
            if !(0.0..150.0).contains(&c) {
                continue;
            }
            let label = std::fs::read_to_string(base.join(format!("temp{i}_label")))
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| format!("temp{i}"));
            out.push(SensorRow {
                label: format!("{chip} · {label}"),
                info: format!("{c:.0}°C"),
                pct: (c / 105.0 * 100.0).clamp(0.0, 100.0),
            });
        }
        for i in 1..=8u32 {
            let inp = base.join(format!("fan{i}_input"));
            let Ok(t) = std::fs::read_to_string(&inp) else {
                if i > 2 {
                    break;
                }
                continue;
            };
            let rpm: i64 = match t.trim().parse() {
                Ok(v) => v,
                Err(_) => continue,
            };
            if rpm <= 0 {
                continue;
            }
            let label = std::fs::read_to_string(base.join(format!("fan{i}_label")))
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| format!("fan{i}"));
            out.push(SensorRow {
                label: format!("{chip} · {label}"),
                info: format!("{rpm} RPM"),
                pct: 0.0,
            });
        }
        if out.len() >= 24 {
            break;
        }
    }
    out
}

pub struct Governor {
    pub current: String,
    pub available: Vec<String>,
}

pub fn read_governor() -> Governor {
    let mut cur = String::from("unknown");
    let mut avail: Vec<String> = vec![];
    if let Ok(policies) = std::fs::read_dir("/sys/devices/system/cpu/cpufreq") {
        for p in policies.flatten() {
            if cur == "unknown" {
                if let Ok(t) = std::fs::read_to_string(p.path().join("scaling_governor")) {
                    cur = t.trim().to_string();
                }
            }
            if avail.is_empty() {
                if let Ok(t) = std::fs::read_to_string(p.path().join("scaling_available_governors"))
                {
                    avail = t.split_whitespace().map(|s| s.to_string()).collect();
                }
            }
            if cur != "unknown" && !avail.is_empty() {
                break;
            }
        }
    }
    Governor {
        current: cur,
        available: avail,
    }
}

pub fn set_governor(gov: &str) -> anyhow::Result<String> {
    let mut ok = 0;
    let mut last_err = String::new();
    if let Ok(policies) = std::fs::read_dir("/sys/devices/system/cpu/cpufreq") {
        for p in policies.flatten() {
            let target = p.path().join("scaling_governor");
            match std::fs::write(&target, gov) {
                Ok(_) => ok += 1,
                Err(e) => last_err = e.to_string(),
            }
        }
    }
    if ok > 0 {
        Ok(format!("governor → {gov} ({ok} policies)"))
    } else {
        anyhow::bail!("needs root? {last_err}")
    }
}

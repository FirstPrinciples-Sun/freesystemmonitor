use std::collections::HashMap;
use sysinfo::{Disks, Networks, System};

pub struct Snapshot {
    pub cpu_usage: f32,
    pub cpu_brand: String,
    pub cpu_count: usize,
    pub mem_used_gb: f32,
    pub mem_total_gb: f32,
    pub swap_used_gb: f32,
    pub swap_total_gb: f32,
    pub load_avg: String,
    pub uptime: String,
    pub hostname: String,
    pub kernel: String,
    pub distro: String,
    pub desktop: String,
    pub net_stat: String,
    pub rx_kbps: f64,
    pub tx_kbps: f64,
    pub disk_stat: String,
    pub disk_pct: f32,
    pub mounts: Vec<(String, u64, u64)>,
    pub ifaces: Vec<IfInfo>,
    pub diskio: Vec<DiskIo>,
    pub cpu_freq: String,
    pub mem_free_gb: f32,
}

#[derive(Clone)]
pub struct IfInfo {
    pub name: String,
    pub rx_kbps: f64,
    pub tx_kbps: f64,
    pub ip: String,
}

#[derive(Clone)]
pub struct DiskIo {
    pub dev: String,
    pub r_kbps: f64,
    pub w_kbps: f64,
}

pub struct SysState {
    pub sys: System,
    pub networks: Networks,
    pub disks: Disks,
    pub prev_rx: u64,
    pub prev_tx: u64,
    pub prev_if: HashMap<String, (u64, u64)>,
    pub prev_dio: HashMap<String, (u64, u64)>,
}

impl SysState {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        let networks = Networks::new_with_refreshed_list();
        let disks = Disks::new_with_refreshed_list();
        Self {
            sys,
            networks,
            disks,
            prev_rx: 0,
            prev_tx: 0,
            prev_if: HashMap::new(),
            prev_dio: HashMap::new(),
        }
    }

    pub fn snapshot(&mut self, secs: f32) -> Snapshot {
        let secs = secs.max(0.5) as f64;
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.sys
            .refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        self.networks.refresh(true);
        self.disks.refresh(true);

        let cpu_usage = self.sys.global_cpu_usage();
        let cpu_count = self.sys.cpus().len().max(1);
        let cpu_brand = self
            .sys
            .cpus()
            .first()
            .map(|c| c.brand().trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "CPU".into());

        let mem_total_gb = self.sys.total_memory() as f32 / 1_073_741_824.0;
        let mem_used_gb = self.sys.used_memory() as f32 / 1_073_741_824.0;
        let swap_total_gb = self.sys.total_swap() as f32 / 1_073_741_824.0;
        let swap_used_gb = self.sys.used_swap() as f32 / 1_073_741_824.0;

        let load = System::load_average();
        let load_avg = format!("{:.2} {:.2} {:.2}", load.one, load.five, load.fifteen);
        let uptime = format_uptime(System::uptime());

        let hostname = System::host_name().unwrap_or_else(|| "localhost".into());
        let kernel = System::kernel_version().unwrap_or_else(|| "?".into());
        let distro = System::name()
            .map(|n| match System::os_version() {
                Some(v) => format!("{n} {v}"),
                None => n,
            })
            .unwrap_or_else(|| "Linux".into());
        let desktop = std::env::var("XDG_CURRENT_DESKTOP")
            .or_else(|_| std::env::var("DESKTOP_SESSION"))
            .unwrap_or_else(|_| "Plasma".into());

        // network totals + rate
        let mut rx = 0u64;
        let mut tx = 0u64;
        for n in self.networks.values() {
            rx += n.total_received();
            tx += n.total_transmitted();
        }
        let rx_rate = rx.saturating_sub(self.prev_rx) as f64 / 1024.0 / secs;
        let tx_rate = tx.saturating_sub(self.prev_tx) as f64 / 1024.0 / secs;
        self.prev_rx = rx;
        self.prev_tx = tx;
        let net_stat = format!(
            "↓ {:.0} KB/s  ↑ {:.0} KB/s  (tot ↓ {} ↑ {})",
            rx_rate,
            tx_rate,
            fmt_bytes(rx),
            fmt_bytes(tx)
        );

        // per-interface rates + first IP
        let mut ifaces: Vec<IfInfo> = vec![];
        for (name, n) in self.networks.iter() {
            if name == "lo" {
                continue;
            }
            let (tr, tt) = (n.total_received(), n.total_transmitted());
            let (pr, pt) = self.prev_if.get(name).copied().unwrap_or((tr, tt));
            self.prev_if.insert(name.clone(), (tr, tt));
            let ip = n
                .ip_networks()
                .iter()
                .find_map(|ipn| {
                    let s = format!("{}", ipn.addr);
                    if s.starts_with("127.") || s == "::1" {
                        None
                    } else {
                        Some(s)
                    }
                })
                .unwrap_or_default();
            ifaces.push(IfInfo {
                name: name.clone(),
                rx_kbps: tr.saturating_sub(pr) as f64 / 1024.0 / secs,
                tx_kbps: tt.saturating_sub(pt) as f64 / 1024.0 / secs,
                ip,
            });
        }
        ifaces.sort_by(|a, b| {
            (b.rx_kbps + b.tx_kbps)
                .partial_cmp(&(a.rx_kbps + a.tx_kbps))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // per-device disk I/O from /proc/diskstats (sectors are 512 B)
        let mut diskio: Vec<DiskIo> = vec![];
        if let Ok(txt) = std::fs::read_to_string("/proc/diskstats") {
            for line in txt.lines() {
                let f: Vec<&str> = line.split_whitespace().collect();
                if f.len() < 14 {
                    continue;
                }
                let dev = f[2];
                if dev.starts_with("loop")
                    || dev.starts_with("ram")
                    || dev.starts_with("zram")
                    || dev.starts_with("fd")
                    || dev.starts_with("sr")
                {
                    continue;
                }
                let (rs, ws) = (
                    f[5].parse::<u64>().unwrap_or(0),
                    f[9].parse::<u64>().unwrap_or(0),
                );
                let (pr, pw) = self.prev_dio.get(dev).copied().unwrap_or((rs, ws));
                self.prev_dio.insert(dev.to_string(), (rs, ws));
                diskio.push(DiskIo {
                    dev: dev.to_string(),
                    r_kbps: rs.saturating_sub(pr) as f64 * 0.5 / secs,
                    w_kbps: ws.saturating_sub(pw) as f64 * 0.5 / secs,
                });
            }
        }
        diskio.sort_by(|a, b| {
            (b.r_kbps + b.w_kbps)
                .partial_cmp(&(a.r_kbps + a.w_kbps))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        diskio.truncate(6);

        // disk usage: biggest mount + full list for the Storage card
        let mut disk_stat = String::from("—");
        let mut best = (0u64, 0u64, String::new());
        let mut mounts: Vec<(String, u64, u64)> = vec![];
        for d in self.disks.list() {
            let total = d.total_space();
            if total > best.0 {
                best = (
                    total,
                    d.available_space(),
                    d.mount_point().to_string_lossy().into_owned(),
                );
            }
            if total > 0 {
                mounts.push((
                    d.mount_point().to_string_lossy().into_owned(),
                    total.saturating_sub(d.available_space()),
                    total,
                ));
            }
        }
        mounts.sort_by_key(|a| std::cmp::Reverse(a.2));
        mounts.truncate(5);
        let mut disk_pct = 0.0f32;
        if best.0 > 0 {
            let used = best.0 - best.1;
            let pct = used as f64 / best.0 as f64 * 100.0;
            disk_pct = pct.clamp(0.0, 100.0) as f32;
            disk_stat = if best.2 == "/" {
                format!("{} / {} ({:.0}%)", fmt_bytes(used), fmt_bytes(best.0), pct)
            } else {
                format!(
                    "{}: {} / {} ({:.0}%)",
                    best.2,
                    fmt_bytes(used),
                    fmt_bytes(best.0),
                    pct
                )
            };
        }

        // cpu frequency (average, GHz)
        let mut freq_mhz = 0u64;
        let mut freq_n = 0u64;
        for c in self.sys.cpus() {
            if c.frequency() > 0 {
                freq_mhz += c.frequency();
                freq_n += 1;
            }
        }
        let cpu_freq = if freq_n > 0 {
            format!("{:.1} GHz avg", freq_mhz as f64 / freq_n as f64 / 1000.0)
        } else {
            "—".into()
        };
        let mem_free_gb = self.sys.free_memory() as f32 / 1_073_741_824.0;

        Snapshot {
            cpu_usage,
            cpu_brand,
            cpu_count,
            mem_used_gb,
            mem_total_gb,
            swap_used_gb,
            swap_total_gb,
            load_avg,
            uptime,
            hostname,
            kernel,
            distro,
            desktop,
            net_stat,
            rx_kbps: rx_rate,
            tx_kbps: tx_rate,
            disk_stat,
            disk_pct,
            mounts,
            ifaces,
            diskio,
            cpu_freq,
            mem_free_gb,
        }
    }

    pub fn core_usages(&self) -> Vec<(String, f32)> {
        self.sys
            .cpus()
            .iter()
            .enumerate()
            .map(|(i, c)| (format!("CPU{i}"), c.cpu_usage()))
            .collect()
    }
}

pub fn format_uptime(secs: u64) -> String {
    let d = secs / 86400;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    if d > 0 {
        format!("{d}d {h}h {m}m")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

pub fn fmt_bytes(b: u64) -> String {
    const GB: f64 = 1_073_741_824.0;
    const MB: f64 = 1_048_576.0;
    const KB: f64 = 1024.0;
    let f = b as f64;
    if f >= GB {
        format!("{:.1} GB", f / GB)
    } else if f >= MB {
        format!("{:.0} MB", f / MB)
    } else if f >= KB {
        format!("{:.0} KB", f / KB)
    } else {
        format!("{b} B")
    }
}

#[allow(dead_code)]
pub fn uid_name_map() -> HashMap<u32, String> {
    // parse /etc/passwd quickly (no extra deps)
    let mut m = HashMap::new();
    if let Ok(txt) = std::fs::read_to_string("/etc/passwd") {
        for line in txt.lines() {
            let p: Vec<&str> = line.split(':').collect();
            if p.len() >= 3 {
                if let Ok(uid) = p[2].parse::<u32>() {
                    m.insert(uid, p[0].to_string());
                }
            }
        }
    }
    m
}

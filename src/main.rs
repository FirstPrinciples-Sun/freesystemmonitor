mod backend;

use backend::{alerts, apps, autostart, cleaner, icons, logs, process, sensors, services, system};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use std::cell::RefCell;

slint::include_modules!();

fn to_shared(s: impl Into<String>) -> slint::SharedString {
    slint::SharedString::from(s.into())
}

// Image handles are bound to the UI thread: cache loaded icons here,
// keyed by file path. The polling thread only passes paths (Send).
thread_local! {
    static ICON_IMGS: RefCell<HashMap<String, slint::Image>> = RefCell::new(HashMap::new());
}

fn cached_icon(path: &str) -> slint::Image {
    ICON_IMGS.with(|c| {
        let mut c = c.borrow_mut();
        if let Some(img) = c.get(path) {
            return img.clone();
        }
        let img = slint::Image::load_from_path(std::path::Path::new(path)).unwrap_or_default();
        if c.len() > 400 {
            c.clear();
        }
        c.insert(path.to_string(), img.clone());
        img
    })
}

fn accent_pair(name: &str) -> (slint::Color, slint::Color) {
    match name {
        "mint" => (
            slint::Color::from_rgb_u8(46, 204, 113),
            slint::Color::from_rgb_u8(26, 188, 156),
        ),
        "sunset" => (
            slint::Color::from_rgb_u8(246, 116, 0),
            slint::Color::from_rgb_u8(218, 68, 83),
        ),
        "grape" => (
            slint::Color::from_rgb_u8(155, 89, 182),
            slint::Color::from_rgb_u8(232, 67, 147),
        ),
        "gold" => (
            slint::Color::from_rgb_u8(247, 181, 0),
            slint::Color::from_rgb_u8(246, 116, 0),
        ),
        _ => (
            slint::Color::from_rgb_u8(61, 174, 233),
            slint::Color::from_rgb_u8(123, 97, 255),
        ),
    }
}

/// Translucent area-chart fill matching the accent.
fn accent_fill(name: &str) -> slint::Color {
    match name {
        "mint" => slint::Color::from_argb_u8(46, 46, 204, 113),
        "sunset" => slint::Color::from_argb_u8(46, 246, 116, 0),
        "grape" => slint::Color::from_argb_u8(46, 155, 89, 182),
        "gold" => slint::Color::from_argb_u8(46, 247, 181, 0),
        _ => slint::Color::from_argb_u8(46, 61, 174, 233),
    }
}

fn settings_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    std::path::Path::new(&home).join(".config/freemon/settings.conf")
}

fn load_settings() -> (String, u64) {
    let mut accent = "breeze".to_string();
    let mut interval: u64 = 1;
    if let Ok(txt) = std::fs::read_to_string(settings_path()) {
        for line in txt.lines() {
            let mut kv = line.splitn(2, '=');
            match (
                kv.next().unwrap_or("").trim(),
                kv.next().unwrap_or("").trim(),
            ) {
                ("accent", v) if !v.is_empty() => accent = v.to_string(),
                ("interval", v) => {
                    if let Ok(n) = v.parse::<u64>() {
                        if (1..=10).contains(&n) {
                            interval = n;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    (accent, interval)
}

fn save_settings(accent: &str, interval: u64) {
    let p = settings_path();
    let _ = std::fs::create_dir_all(p.parent().unwrap());
    let _ = std::fs::write(&p, format!("accent={accent}\ninterval={interval}\n"));
}

fn main() -> anyhow::Result<()> {
    let ui = AppWindow::new()?;
    let weak = ui.as_weak();

    let search_filter: Arc<Mutex<String>> = Arc::new(Mutex::new(String::new()));
    let search_filter_cb = search_filter.clone();

    // ---- theme settings (persisted) ----
    let (accent_init, interval_init) = load_settings();
    // debug/test hook: start on a given page (0-9)
    if let Ok(p) = std::env::var("FREEMON_PAGE") {
        if let Ok(n) = p.parse::<i32>() {
            if (0..=9).contains(&n) {
                ui.set_current_page(n);
            }
        }
    }
    let interval: Arc<Mutex<u64>> = Arc::new(Mutex::new(interval_init));
    let interval_cb = interval.clone();
    {
        let (c1, c2) = accent_pair(&accent_init);
        let cf = accent_fill(&accent_init);
        ui.set_accent1(c1);
        ui.set_accent2(c2);
        ui.set_area_fill(cf);
        ui.set_accent_name(to_shared(accent_init.clone()));
        ui.set_refresh_secs(interval_init as i32);
    }
    {
        let w = weak.clone();
        ui.on_set_accent(move |name| {
            let name = name.to_string();
            let secs = *interval_cb.lock().unwrap();
            let (c1, c2) = accent_pair(&name);
            let cf = accent_fill(&name);
            save_settings(&name, secs);
            let _ = w.upgrade_in_event_loop(move |ui| {
                ui.set_accent1(c1);
                ui.set_accent2(c2);
                ui.set_area_fill(cf);
                ui.set_accent_name(to_shared(name));
            });
        });
    }
    {
        let w = weak.clone();
        let interval_cb2 = interval.clone();
        ui.on_set_interval(move |secs| {
            let secs = (secs as u64).clamp(1, 10);
            *interval_cb2.lock().unwrap() = secs;
            let acc = w.upgrade_in_event_loop(move |ui| {
                ui.set_refresh_secs(secs as i32);
            });
            let _ = acc;
            // persist with current accent
            let cur = load_settings().0;
            save_settings(&cur, secs);
            // update label immediately
            let w2 = w.clone();
            let _ = w2.upgrade_in_event_loop(move |ui| {
                ui.set_status_msg(to_shared(format!("refresh every {secs}s")));
            });
        });
    }

    // ---- callbacks: processes ----
    {
        let w = weak.clone();
        ui.on_kill_process(move |pid| match process::kill_pid(pid, true) {
            Ok(m) => set_status(&w, format!("killed {pid} ({m})")),
            Err(e) => set_status(&w, format!("kill {pid} failed: {e:#}")),
        });
    }
    {
        let w = weak.clone();
        ui.on_stop_process(move |pid| match process::kill_pid(pid, false) {
            Ok(m) => set_status(&w, format!("stopped {pid} ({m})")),
            Err(e) => set_status(&w, format!("stop {pid} failed: {e:#}")),
        });
    }
    {
        let w = weak.clone();
        ui.on_renice_process(move |pid, nice| match process::renice(pid, nice) {
            Ok(m) => set_status(&w, m),
            Err(e) => set_status(&w, format!("renice failed: {e:#}")),
        });
    }
    {
        ui.on_search_changed(move |t| {
            *search_filter_cb.lock().unwrap() = t.to_string();
        });
    }

    // ---- callbacks: services ----
    {
        let w = weak.clone();
        ui.on_service_action(move |name, action| {
            let name = name.to_string();
            let action = action.to_string();
            match services::service_action(&name, &action) {
                Ok(m) => {
                    set_status(&w, m);
                    refresh_services_ui(&w);
                }
                Err(e) => set_status(&w, format!("service error: {e:#}")),
            }
        });
    }
    {
        let w = weak.clone();
        ui.on_refresh_services(move || refresh_services_ui(&w));
    }

    // ---- callbacks: autostart ----
    {
        let w = weak.clone();
        ui.on_toggle_autostart(move |file, enabled| {
            match autostart::set_autostart_enabled(file.as_ref(), enabled) {
                Ok(m) => {
                    set_status(&w, m);
                    refresh_autostart_ui(&w);
                }
                Err(e) => set_status(&w, format!("autostart error: {e:#}")),
            }
        });
    }

    // ---- callbacks: cleaner / logs / apps ----
    {
        let w = weak.clone();
        ui.on_clean_kind(move |kind| match cleaner::clean_kind(kind.as_ref()) {
            Ok(m) => {
                set_status(&w, m.clone());
                refresh_cleaner_ui(&w);
            }
            Err(e) => set_status(&w, format!("clean failed: {e:#}")),
        });
    }
    {
        let w = weak.clone();
        ui.on_refresh_logs(move || {
            refresh_logs_ui(&w);
        });
    }
    {
        let w = weak.clone();
        ui.on_uninstall_app(move |id| match apps::uninstall_flatpak(id.as_ref()) {
            Ok(m) => {
                set_status(&w, m);
                refresh_apps_ui(&w);
            }
            Err(e) => set_status(&w, format!("uninstall failed: {e:#}")),
        });
    }

    // ---- static/slow data first paint ----
    refresh_services_ui(&weak);
    refresh_autostart_ui(&weak);
    refresh_apps_ui(&weak);
    refresh_cleaner_ui(&weak);
    refresh_logs_ui(&weak);

    // ---- 1s polling thread (interval is customizable) ----
    std::thread::spawn(move || {
        let mut st = system::SysState::new();
        let users = system::uid_name_map();
        let icon_db = icons::IconDb::build();
        let mut prev_disk: std::collections::HashMap<u32, (u64, u64)> =
            std::collections::HashMap::new();
        let mut last_gpu_use: f32 = -1.0;
        let mut last_gpu_sub = String::from("usage unavailable on this GPU");
        let mut last_intel_use: f32 = -1.0;
        let mut last_intel_sub = String::from("integrated · no driver data");
        let mut active_alerts: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut tick: u64 = 0;
        let mut cpu_hist: VecDeque<f32> = VecDeque::from(vec![0.0; 60]);
        let mut mem_hist: VecDeque<f32> = VecDeque::from(vec![0.0; 60]);
        let mut swap_hist: VecDeque<f32> = VecDeque::from(vec![0.0; 60]);
        let mut nd_hist: VecDeque<f32> = VecDeque::from(vec![0.0; 60]);
        let mut nu_hist: VecDeque<f32> = VecDeque::from(vec![0.0; 60]);
        loop {
            let secs = (*interval.lock().unwrap_or_else(|e| e.into_inner())).max(1);
            let snap = st.snapshot();
            let filter = search_filter.lock().unwrap().clone();
            let procs =
                process::top_processes(&st.sys, &users, &mut prev_disk, secs as f32, &filter, 100);
            let cores = st.core_usages();
            let temp = sensors::cpu_temp();
            let batt = if tick.is_multiple_of(5) {
                sensors::battery()
            } else {
                String::new()
            };
            let gpu = if tick == 0 {
                sensors::gpu_name()
            } else {
                String::new()
            };
            let clock = chrono::Local::now().format("%H:%M:%S").to_string();
            let proc_count = st.sys.processes().len() as i32;
            let mem_pct_now = pct(snap.mem_used_gb, snap.mem_total_gb);
            let swap_pct_now = pct(snap.swap_used_gb, snap.swap_total_gb);
            cpu_hist.push_back(snap.cpu_usage.clamp(0.0, 100.0));
            mem_hist.push_back(mem_pct_now);
            swap_hist.push_back(swap_pct_now);
            // normalize net rates to 0..100 against a 10 MB/s full scale
            nd_hist.push_back((snap.rx_kbps / 10240.0 * 100.0).clamp(0.0, 100.0) as f32);
            nu_hist.push_back((snap.tx_kbps / 10240.0 * 100.0).clamp(0.0, 100.0) as f32);
            for h in [
                &mut cpu_hist,
                &mut mem_hist,
                &mut swap_hist,
                &mut nd_hist,
                &mut nu_hist,
            ] {
                while h.len() > 60 {
                    h.pop_front();
                }
            }
            let ch: Vec<f32> = cpu_hist.iter().copied().collect();
            let mh: Vec<f32> = mem_hist.iter().copied().collect();
            let sh: Vec<f32> = swap_hist.iter().copied().collect();
            let ndh: Vec<f32> = nd_hist.iter().copied().collect();
            let nuh: Vec<f32> = nu_hist.iter().copied().collect();
            let net_rate = format!("↓ {:.0} KB/s · ↑ {:.0} KB/s", snap.rx_kbps, snap.tx_kbps);
            if tick.is_multiple_of(5) {
                // Prefer the dedicated GPU: one nvidia-smi call gives name+usage+temp.
                if let Some(info) = sensors::nvidia_info() {
                    if info.usage >= 0.0 {
                        last_gpu_use = info.usage;
                    }
                    last_gpu_sub = if info.temp > -100.0 {
                        format!("{:.0}°C · measured live", info.temp)
                    } else {
                        String::from("measured live")
                    };
                    let name = info.name.clone();
                    let sub = last_gpu_sub.clone();
                    let _ = weak.upgrade_in_event_loop(move |ui| {
                        ui.set_gpu_name(to_shared(name));
                        ui.set_gpu_sub(to_shared(sub));
                    });
                } else if let Some(v) = sensors::gpu_usage() {
                    last_gpu_use = v;
                    last_gpu_sub = String::from("measured live");
                }
                // Intel iGPU activity via frequency proxy (cheap sysfs reads).
                if let Some(ig) = sensors::intel_gpu() {
                    last_intel_use = ig.usage;
                    last_intel_sub = format!("{} · freq activity", ig.freq);
                }
            }
            let gpu_sub_now = last_gpu_sub.clone();
            let intel_use_now = last_intel_use;
            let intel_sub_now = last_intel_sub.clone();
            // alerts with rising-edge desktop notifications
            let temp_c = temp
                .trim_end_matches("°C")
                .trim()
                .parse::<i64>()
                .ok();
            let found = alerts::evaluate(&alerts::AlertInput {
                cpu: snap.cpu_usage,
                mem_pct: pct(snap.mem_used_gb, snap.mem_total_gb),
                disk_pct: snap.disk_pct,
                swap_pct: pct(snap.swap_used_gb, snap.swap_total_gb),
                temp_c,
            });
            let keys: std::collections::HashSet<String> =
                found.iter().map(|a| a.key.to_string()).collect();
            for a in &found {
                if !active_alerts.contains(a.key) {
                    alerts::notify(a.critical, "FreeMon alert", &a.text);
                }
            }
            active_alerts = keys;
            let alert_text = found
                .iter()
                .map(|a| format!("{} {}", if a.critical { "⛔" } else { "⚠" }, a.text))
                .collect::<Vec<_>>()
                .join("   ·   ");
            let alert_on = !found.is_empty();
            let gpu_use_now = last_gpu_use;
            let batt_now = sensors::battery_pct();
            let mounts_now = snap
                .mounts
                .iter()
                .map(|(mp, used, total)| {
                    let pct = (*used as f32 / *total as f32 * 100.0).clamp(0.0, 100.0);
                    (mp.clone(), system::fmt_bytes(*used), system::fmt_bytes(*total), pct)
                })
                .collect::<Vec<_>>();
            // resolve process icon paths (Strings cross threads; images load on UI thread)
            let mut proc_icon_paths: HashMap<i32, Option<String>> = HashMap::new();
            for p in procs.iter() {
                if proc_icon_paths.contains_key(&p.pid) {
                    continue;
                }
                proc_icon_paths.insert(
                    p.pid,
                    icon_db.find(&p.icon_key).map(|s| s.to_string()),
                );
            }

            let res = weak.upgrade_in_event_loop(move |ui| {
                ui.set_cpu_usage(snap.cpu_usage);
                ui.set_cpu_brand(to_shared(snap.cpu_brand));
                ui.set_cpu_count(snap.cpu_count as i32);
                ui.set_mem_used_gb(round2(snap.mem_used_gb));
                ui.set_mem_total_gb(round2(snap.mem_total_gb));
                ui.set_mem_pct(pct(snap.mem_used_gb, snap.mem_total_gb));
                ui.set_swap_used_gb(round2(snap.swap_used_gb));
                ui.set_swap_total_gb(round2(snap.swap_total_gb));
                ui.set_swap_pct(pct(snap.swap_used_gb, snap.swap_total_gb));
                ui.set_load_avg(to_shared(snap.load_avg));
                ui.set_uptime(to_shared(snap.uptime));
                ui.set_hostname(to_shared(snap.hostname));
                ui.set_kernel(to_shared(snap.kernel));
                ui.set_distro(to_shared(snap.distro));
                ui.set_desktop(to_shared(snap.desktop));
                ui.set_net_stat(to_shared(snap.net_stat));
                ui.set_disk_stat(to_shared(snap.disk_stat));
                ui.set_disk_pct(snap.disk_pct);
                ui.set_cpu_temp(to_shared(temp));
                ui.set_cpu_freq(to_shared(snap.cpu_freq));
                ui.set_mem_free(round2(snap.mem_free_gb));
                ui.set_clock(to_shared(clock));
                ui.set_proc_count(proc_count);
                ui.set_gpu_use(gpu_use_now);
                ui.set_gpu_sub(to_shared(gpu_sub_now));
                ui.set_intel_use(intel_use_now);
                ui.set_intel_sub(to_shared(intel_sub_now));
                ui.set_alert_on(alert_on);
                ui.set_alert_text(to_shared(alert_text));
                match batt_now {
                    Some((v, charging)) => {
                        ui.set_batt_pct(v);
                        ui.set_batt_charging(charging);
                    }
                    None => ui.set_batt_pct(-1.0),
                }
                if !batt.is_empty() {
                    ui.set_battery(to_shared(batt));
                }
                if !gpu.is_empty() {
                    ui.set_gpu_name(to_shared(gpu));
                }
                let mrows: Vec<MountRow> = mounts_now
                    .iter()
                    .map(|(mp, used, total, p)| MountRow {
                        mp: to_shared(mp.clone()),
                        info: to_shared(format!("{used} / {total}")),
                        pct: *p,
                    })
                    .collect();
                ui.set_mounts(std::rc::Rc::new(slint::VecModel::from(mrows)).into());

                let rows: Vec<ProcRow> = procs
                    .iter()
                    .map(|p| ProcRow {
                        name: to_shared(p.name.clone()),
                        pid: p.pid,
                        cpu: p.cpu,
                        mem: p.mem_pct,
                        mem_mb: (p.mem_mb * 10.0).round() / 10.0,
                        user: to_shared(p.user.clone()),
                        status: to_shared(p.status.clone()),
                        disk_str: to_shared(p.disk_str.clone()),
                        icon: match proc_icon_paths.get(&p.pid).and_then(|o| o.as_deref()) {
                            Some(path) => cached_icon(path),
                            None => Default::default(),
                        },
                    })
                    .collect();
                ui.set_processes(std::rc::Rc::new(slint::VecModel::from(rows)).into());

                let crows: Vec<CoreRow> = cores
                    .iter()
                    .map(|(l, u)| CoreRow {
                        label: to_shared(l.clone()),
                        usage: *u,
                    })
                    .collect();
                ui.set_cores(std::rc::Rc::new(slint::VecModel::from(crows)).into());

                ui.set_cpu_hist(std::rc::Rc::new(slint::VecModel::from(ch.clone())).into());
                ui.set_mem_hist(std::rc::Rc::new(slint::VecModel::from(mh.clone())).into());
                ui.set_swap_hist(std::rc::Rc::new(slint::VecModel::from(sh.clone())).into());
                ui.set_netdown_hist(std::rc::Rc::new(slint::VecModel::from(ndh)).into());
                ui.set_netup_hist(std::rc::Rc::new(slint::VecModel::from(nuh)).into());
                ui.set_net_rate(to_shared(net_rate));
                let (ca, cl, cly) = area_cmds(&ch);
                ui.set_cpu_area(to_shared(ca));
                ui.set_cpu_line(to_shared(cl));
                ui.set_cpu_ly(cly);
                let (ma, ml, mly) = area_cmds(&mh);
                ui.set_mem_area(to_shared(ma));
                ui.set_mem_line(to_shared(ml));
                ui.set_mem_ly(mly);
                let (sa, sl, sly) = area_cmds(&sh);
                ui.set_swap_area(to_shared(sa));
                ui.set_swap_line(to_shared(sl));
                ui.set_swap_ly(sly);
            });
            if res.is_err() {
                break; // window closed
            }
            tick += 1;
            std::thread::sleep(Duration::from_secs(secs));
        }
    });

    ui.run()?;
    Ok(())
}

fn round2(v: f32) -> f32 {
    (v * 100.0).round() / 100.0
}

/// Build SVG path commands for a 60-point area chart (300x90 viewbox).
/// Returns (area_fill_commands, line_commands, last_y).
fn area_cmds(data: &[f32]) -> (String, String, f32) {
    const HH: f32 = 90.0;
    let y = |v: f32| HH - (HH - 6.0) * v.clamp(0.0, 100.0) / 100.0;
    let mut area = format!("M 0 {HH}");
    let mut line = String::new();
    for (i, v) in data.iter().enumerate() {
        let x = i as f32 * 5.0;
        let yy = y(*v);
        if i == 0 {
            line = format!("M {x:.1} {yy:.1}");
        } else {
            line += &format!(" L {x:.1} {yy:.1}");
        }
        area += &format!(" L {x:.1} {yy:.1}");
    }
    area += &format!(" L 295 {HH} Z");
    (area, line, y(data.last().copied().unwrap_or(0.0)))
}

fn pct(used: f32, total: f32) -> f32 {
    if total <= 0.0 {
        0.0
    } else {
        (used / total * 100.0).clamp(0.0, 100.0)
    }
}

fn set_status(weak: &slint::Weak<AppWindow>, msg: String) {
    let _ = weak.upgrade_in_event_loop(move |ui| {
        ui.set_status_msg(to_shared(msg));
    });
}

fn refresh_services_ui(weak: &slint::Weak<AppWindow>) {
    let list = services::list_user_services();
    let act = list.iter().filter(|s| s.active).count() as i32;
    let tot = list.len() as i32;
    let _ = weak.upgrade_in_event_loop(move |ui| {
        ui.set_svc_act(act);
        ui.set_svc_tot(tot);
        let rows: Vec<ServiceRow> = list
            .iter()
            .map(|s| ServiceRow {
                name: to_shared(s.name.clone()),
                desc: to_shared(s.desc.clone()),
                state: to_shared(s.state.clone()),
                active: s.active,
            })
            .collect();
        ui.set_services(std::rc::Rc::new(slint::VecModel::from(rows)).into());
    });
}

fn refresh_autostart_ui(weak: &slint::Weak<AppWindow>) {
    let list = autostart::list_autostart();
    let act = list.iter().filter(|a| a.enabled).count() as i32;
    let tot = list.len() as i32;
    let _ = weak.upgrade_in_event_loop(move |ui| {
        ui.set_auto_act(act);
        ui.set_auto_tot(tot);
        let rows: Vec<AutostartRow> = list
            .iter()
            .map(|a| AutostartRow {
                name: to_shared(a.name.clone()),
                file: to_shared(a.file.clone()),
                enabled: a.enabled,
            })
            .collect();
        ui.set_autostart(std::rc::Rc::new(slint::VecModel::from(rows)).into());
    });
}

fn refresh_apps_ui(weak: &slint::Weak<AppWindow>) {
    let list = apps::list_flatpak_apps();
    let max = list
        .iter()
        .map(|a| a.size_mb)
        .fold(0.0f32, f32::max)
        .max(1.0);
    let tot = list.len() as i32;
    let _ = weak.upgrade_in_event_loop(move |ui| {
        ui.set_app_tot(tot);
        let rows: Vec<AppRow> = list
            .iter()
            .map(|a| AppRow {
                name: to_shared(a.name.clone()),
                id: to_shared(a.id.clone()),
                size: to_shared(a.size.clone()),
                origin: to_shared(a.origin.clone()),
                pct: (a.size_mb / max * 100.0).clamp(0.0, 100.0),
            })
            .collect();
        ui.set_apps(std::rc::Rc::new(slint::VecModel::from(rows)).into());
    });
}

fn refresh_cleaner_ui(weak: &slint::Weak<AppWindow>) {
    let items = cleaner::sizes();
    let max = items.iter().map(|(_, b)| *b).max().unwrap_or(1).max(1) as f32;
    let _ = weak.upgrade_in_event_loop(move |ui| {
        let rows: Vec<SizeRow> = items
            .iter()
            .map(|(label, bytes)| SizeRow {
                label: to_shared(label.clone()),
                info: to_shared(system::fmt_bytes(*bytes)),
                pct: (*bytes as f32 / max * 100.0).clamp(0.0, 100.0),
            })
            .collect();
        ui.set_size_rows(std::rc::Rc::new(slint::VecModel::from(rows)).into());
        ui.set_cleaner_info(to_shared(cleaner::cleaner_summary()));
    });
}

fn refresh_logs_ui(weak: &slint::Weak<AppWindow>) {
    let txt = logs::tail_logs();
    let (err, warn, tot) = logs::analyze(&txt);
    let _ = weak.upgrade_in_event_loop(move |ui| {
        ui.set_logs_text(to_shared(txt));
        ui.set_log_err(err);
        ui.set_log_warn(warn);
        ui.set_log_tot(tot);
    });
}

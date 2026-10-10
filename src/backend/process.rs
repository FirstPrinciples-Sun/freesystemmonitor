use std::collections::{HashMap, HashSet};
use sysinfo::System;

#[derive(Clone)]
pub struct ProcInfo {
    pub pid: i32,
    pub name: String,
    pub cpu: f32,
    pub mem_mb: f32,
    pub mem_pct: f32,
    pub user: String,
    pub status: String,
    pub disk_str: String,
    pub disk_kbps: f32,
    pub icon_key: String,
    pub depth: i32,
    pub kids: bool,
    pub collapsed: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    Cpu,
    Mem,
    Disk,
    Name,
    Pid,
    Tree,
}

impl SortMode {
    pub fn from_str(s: &str) -> Self {
        match s {
            "Memory" => Self::Mem,
            "Disk" => Self::Disk,
            "Name" => Self::Name,
            "PID" => Self::Pid,
            "Tree" => Self::Tree,
            _ => Self::Cpu,
        }
    }

    pub fn index(&self) -> i32 {
        match self {
            Self::Cpu => 0,
            Self::Mem => 1,
            Self::Disk => 2,
            Self::Name => 3,
            Self::Pid => 4,
            Self::Tree => 5,
        }
    }
}

pub struct Ctx<'a> {
    pub sys: &'a System,
    pub users: &'a HashMap<u32, String>,
    pub prev_disk: &'a mut HashMap<u32, (u64, u64)>,
}

pub fn top_processes(
    ctx: Ctx<'_>,
    secs: f32,
    filter: &str,
    limit: usize,
    sort: SortMode,
    collapsed: &HashSet<u32>,
) -> Vec<ProcInfo> {
    use sysinfo::Pid;
    let Ctx { sys, users, prev_disk } = ctx;
    // drop dead pids so the map doesn't grow forever
    prev_disk.retain(|k, _| sys.processes().contains_key(&Pid::from_u32(*k)));
    if sort == SortMode::Tree && filter.trim().is_empty() {
        return build_tree(sys, users, prev_disk, secs, collapsed);
    }
    let total_mem = sys.total_memory().max(1) as f32;
    let f = filter.trim().to_lowercase();
    let mut v: Vec<ProcInfo> = sys
        .processes()
        .iter()
        .filter_map(|(pid, p)| {
            let name = p.name().to_string_lossy().into_owned();
            if !f.is_empty() {
                let pid_s = pid.as_u32().to_string();
                if !name.to_lowercase().contains(&f) && !pid_s.contains(&f) {
                    return None;
                }
            }
            let mem_mb = p.memory() as f32 / 1_048_576.0;
            let mem_pct = p.memory() as f32 / total_mem * 100.0;
            let uid = p
                .user_id()
                .map(|u| u.to_string().parse::<u32>().unwrap_or(99999))
                .unwrap_or(99999);
            let user = users.get(&uid).cloned().unwrap_or_else(|| "?".into());
            let du = p.disk_usage();
            let (r, w) = (du.total_read_bytes, du.total_written_bytes);
            let kbps = match prev_disk.get(&pid.as_u32()) {
                Some((pr, pw)) => ((r.saturating_sub(*pr) + w.saturating_sub(*pw)) as f32
                    / 1024.0
                    / secs.max(0.5))
                .max(0.0),
                None => 0.0,
            };
            prev_disk.insert(pid.as_u32(), (r, w));
            // icon lookup key: exe stem ("firefox") beats comm name ("firefox-bin")
            let icon_key = p
                .exe()
                .and_then(|e| e.file_stem().map(|s| s.to_string_lossy().to_lowercase()))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| name.to_lowercase());
            Some(ProcInfo {
                pid: pid.as_u32() as i32,
                name,
                cpu: p.cpu_usage(),
                mem_mb,
                mem_pct,
                user,
                status: format!("{:?}", p.status()),
                disk_str: fmt_rate(kbps),
                disk_kbps: kbps,
                icon_key,
                depth: 0,
                kids: false,
                collapsed: false,
            })
        })
        .collect();
    // sort by the selected column
    match sort {
        SortMode::Cpu => v.sort_by(|a, b| {
            b.cpu
                .partial_cmp(&a.cpu)
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        SortMode::Mem => v.sort_by(|a, b| {
            b.mem_mb
                .partial_cmp(&a.mem_mb)
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        SortMode::Disk => v.sort_by(|a, b| {
            b.disk_kbps
                .partial_cmp(&a.disk_kbps)
                .unwrap_or(std::cmp::Ordering::Equal)
        }),
        SortMode::Name => v.sort_by_key(|a| a.name.to_lowercase()),
        SortMode::Pid => v.sort_by_key(|a| a.pid),
        SortMode::Tree => {} // handled by build_tree before filtering
    }
    v.truncate(limit);
    v
}

/// Depth-first process tree (parent before children, siblings by CPU).
/// Nodes in `collapsed` show as folded and hide their descendants.
pub fn build_tree(
    sys: &System,
    users: &HashMap<u32, String>,
    prev_disk: &mut HashMap<u32, (u64, u64)>,
    secs: f32,
    collapsed: &HashSet<u32>,
) -> Vec<ProcInfo> {
    use sysinfo::Pid;
    let total_mem = sys.total_memory().max(1) as f32;
    // one info per process
    let mut info: HashMap<u32, ProcInfo> = HashMap::new();
    for (pid, p) in sys.processes().iter() {
        let id = pid.as_u32();
        let name = p.name().to_string_lossy().into_owned();
        let mem_mb = p.memory() as f32 / 1_048_576.0;
        let uid = p
            .user_id()
            .map(|u| u.to_string().parse::<u32>().unwrap_or(99999))
            .unwrap_or(99999);
        let du = p.disk_usage();
        let (r, w) = (du.total_read_bytes, du.total_written_bytes);
        let kbps = match prev_disk.get(&id) {
            Some((pr, pw)) => ((r.saturating_sub(*pr) + w.saturating_sub(*pw)) as f32 / 1024.0
                / secs.max(0.5))
            .max(0.0),
            None => 0.0,
        };
        prev_disk.insert(id, (r, w));
        let icon_key = p
            .exe()
            .and_then(|e| e.file_stem().map(|s| s.to_string_lossy().to_lowercase()))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| name.to_lowercase());
        info.insert(
            id,
            ProcInfo {
                pid: id as i32,
                name,
                cpu: p.cpu_usage(),
                mem_mb,
                mem_pct: p.memory() as f32 / total_mem * 100.0,
                user: users.get(&uid).cloned().unwrap_or_else(|| "?".into()),
                status: format!("{:?}", p.status()),
                disk_str: fmt_rate(kbps),
                disk_kbps: kbps,
                icon_key,
                depth: 0,
                kids: false,
                collapsed: false,
            },
        );
    }
    // children map; orphans and pid 0/1 attach to the virtual root
    const ROOT: u32 = u32::MAX;
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for (pid, p) in sys.processes().iter() {
        let id = pid.as_u32();
        let parent = p.parent().map(|pp| pp.as_u32()).filter(|pu| info.contains_key(pu));
        children.entry(parent.unwrap_or(ROOT)).or_default().push(id);
    }
    for kids in children.values_mut() {
        kids.sort_by(|a, b| {
            let ca = info.get(a).map(|i| i.cpu).unwrap_or(0.0);
            let cb = info.get(b).map(|i| i.cpu).unwrap_or(0.0);
            cb.partial_cmp(&ca).unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    // iterative DFS with depth cap
    let mut out = vec![];
    let mut stack: Vec<(u32, i32)> = children
        .get(&ROOT)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .rev()
        .map(|id| (id, 0))
        .collect();
    // drop the placeholder Pid import use (kept for retain above)
    let _ = Pid::from_u32(0);
    while let Some((id, depth)) = stack.pop() {
        let Some(node) = info.get_mut(&id) else {
            continue;
        };
        let has_kids = children.get(&id).map(|k| !k.is_empty()).unwrap_or(false);
        let is_collapsed = collapsed.contains(&id);
        node.depth = depth.min(8);
        node.kids = has_kids;
        node.collapsed = is_collapsed;
        let row = node.clone();
        out.push(row);
        if has_kids && !is_collapsed && depth < 8 {
            if let Some(kids) = children.get(&id) {
                for kid in kids.iter().rev() {
                    stack.push((*kid, depth + 1));
                }
            }
        }
    }
    out
}

pub fn kill_pid(pid: i32, force: bool) -> anyhow::Result<String> {
    use sysinfo::{Pid, Signal, System};
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let p = Pid::from_u32(pid as u32);
    match sys.process(p) {
        Some(proc_) => {
            let sig = if force { Signal::Kill } else { Signal::Term };
            if proc_.kill_with(sig).unwrap_or(false) {
                Ok(format!(
                    "sent {} to {pid}",
                    if force { "SIGKILL" } else { "SIGTERM" }
                ))
            } else {
                anyhow::bail!("process refused signal (need root?)")
            }
        }
        None => anyhow::bail!("no such process {pid}"),
    }
}

pub fn renice(pid: i32, nice: i32) -> anyhow::Result<String> {
    let out = std::process::Command::new("renice")
        .args(["-n", &nice.to_string(), "-p", &pid.to_string()])
        .output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        anyhow::bail!("{}", String::from_utf8_lossy(&out.stderr).trim())
    }
}

pub fn read_nice(pid: i32) -> Option<i32> {
    let out = std::process::Command::new("ps")
        .args(["-o", "ni=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse::<i32>()
        .ok()
}

fn fmt_rate(kbps: f32) -> String {
    if kbps < 0.5 {
        "—".into()
    } else if kbps < 1024.0 {
        format!("{kbps:.0} KB/s")
    } else {
        format!("{:.1} MB/s", kbps / 1024.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sysinfo::ProcessesToUpdate;

    fn live_system() -> System {
        let mut sys = System::new_all();
        sys.refresh_all();
        std::thread::sleep(std::time::Duration::from_millis(300));
        sys.refresh_processes(ProcessesToUpdate::All, true);
        sys
    }

    #[test]
    fn sort_modes_order_correctly() {
        use std::collections::{HashMap, HashSet};
        let sys = live_system();
        let users: HashMap<u32, String> = HashMap::new();
        let mut prev = HashMap::new();
        let empty = HashSet::new();
        let by_pid = top_processes(Ctx { sys: &sys, users: &users, prev_disk: &mut prev }, 1.0, "", 50, SortMode::Pid, &empty);
        assert!(!by_pid.is_empty());
        let pids: Vec<i32> = by_pid.iter().map(|p| p.pid).collect();
        for w in pids.windows(2) {
            assert!(w[0] <= w[1]);
        }
        let by_name = top_processes(Ctx { sys: &sys, users: &users, prev_disk: &mut prev }, 1.0, "", 50, SortMode::Name, &empty);
        let names: Vec<String> = by_name.iter().map(|p| p.name.to_lowercase()).collect();
        for w in names.windows(2) {
            assert!(w[0] <= w[1], "{} should sort before {}", w[0], w[1]);
        }
    }

    #[test]
    fn read_nice_self() {
        let me = std::process::id() as i32;
        let n = read_nice(me);
        assert!(n.is_some(), "should read own nice value");
    }

    #[test]
    fn tree_parents_come_first() {
        use std::collections::{HashMap, HashSet};
        let sys = live_system();
        let users: HashMap<u32, String> = HashMap::new();
        let mut prev = HashMap::new();
        let tree = build_tree(&sys, &users, &mut prev, 1.0, &HashSet::new());
        assert!(!tree.is_empty());
        let pos: HashMap<i32, usize> =
            tree.iter().enumerate().map(|(i, p)| (p.pid, i)).collect();
        // every child appears after its parent (when the parent is listed)
        let sysmap = sys.processes();
        for p in &tree {
            assert!(p.depth >= 0 && p.depth <= 8);
            if let Some(proc_) = sysmap.get(&sysinfo::Pid::from_u32(p.pid as u32)) {
                if let Some(pp) = proc_.parent() {
                    let pu = pp.as_u32() as i32;
                    if let (Some(&pi), Some(&ci)) = (pos.get(&pu), pos.get(&p.pid)) {
                        assert!(pi < ci, "parent {} must precede {}", pu, p.pid);
                    }
                }
            }
        }
    }
}

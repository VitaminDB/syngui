//! Осмотр интерфейса снаружи (агенты, проверки): сокет процесса
//! `$XDG_RUNTIME_DIR/syngui-inspect/<pid>.sock`, запрос — строка, ответ —
//! строка JSON.
//!
//! - `tree` — элементы окон: роль, подпись, значение, состояние, рамка в
//!   логических пикселях окна (`[x, y, w, h]` от левого верхнего угла поверхности);
//! - `click <id>` — нажать элемент через дерево доступности (как скринридер);
//! - `set <id> <текст>` — набрать текст в поле.
//!
//! Ничего не стоит, пока не спрашивают: дерево доступности строится и так,
//! снимок с него делается только по запросу (окно перестраивает его на ближайшем кадре).

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use super::{A11yId, A11yNode, A11yTree, Action, Role};

/// Номер запроса: окно, чей снимок старее, перестраивает дерево и публикует.
static WANT: AtomicU64 = AtomicU64::new(0);

struct WinSnap {
    seq: u64,
    size: (f32, f32),
    json_nodes: String,
}

static SNAPS: Mutex<Option<HashMap<u64, WinSnap>>> = Mutex::new(None);
/// Действия для окон: (окно или 0 — любое, узел, действие).
static ACTIONS: Mutex<Vec<(u64, A11yId, Action)>> = Mutex::new(Vec::new());

/// Окно должно перестроить и опубликовать дерево (`last` — номер его снимка).
pub fn wanted(last: u64) -> bool {
    WANT.load(Ordering::Acquire) > last
}

pub fn current() -> u64 {
    WANT.load(Ordering::Acquire)
}

/// Отложенные действия для окна `key`.
pub fn take_actions(key: u64) -> Vec<(A11yId, Action)> {
    let Ok(mut g) = ACTIONS.lock() else { return Vec::new() };
    let mut out = Vec::new();
    g.retain(|(k, id, a)| {
        if *k == key || *k == 0 {
            out.push((*id, a.clone()));
            false
        } else {
            true
        }
    });
    out
}

/// Снимок дерева окна `key` (вызывается после `A11yTree::sync`).
pub fn publish(key: u64, seq: u64, size: (f32, f32), tree: &A11yTree) {
    let mut s = String::from("[");
    if let Some(root) = tree.root() {
        let mut first = true;
        walk(tree, root, 0, &mut s, &mut first);
    }
    s.push(']');
    if let Ok(mut g) = SNAPS.lock() {
        g.get_or_insert_with(HashMap::new).insert(key, WinSnap { seq, size, json_nodes: s });
    }
}

/// Окно закрылось.
pub fn forget(key: u64) {
    if let Ok(mut g) = SNAPS.lock() {
        if let Some(m) = g.as_mut() {
            m.remove(&key);
        }
    }
}

fn walk(tree: &A11yTree, id: A11yId, depth: u32, out: &mut String, first: &mut bool) {
    let Some(n) = tree.get_node(id) else { return };
    // за краем прокрутки/обрезки — не отдаём (нажатие попало бы в другое место)
    if n.offscreen {
        return;
    }
    if id != A11yId::SYNTHETIC_ROOT {
        if !*first {
            out.push(',');
        }
        *first = false;
        node_json(n, depth, out);
    }
    for c in &n.children {
        walk(tree, *c, depth + 1, out, first);
    }
}

fn role_name(r: Role) -> String {
    match r {
        Role::Heading(l) => format!("heading{l}"),
        other => format!("{other:?}").to_lowercase(),
    }
}

fn node_json(n: &A11yNode, depth: u32, out: &mut String) {
    let b = n.bounds;
    out.push_str(&format!(
        "{{\"id\":{},\"depth\":{},\"role\":\"{}\",\"bounds\":[{:.0},{:.0},{:.0},{:.0}]",
        n.id.0,
        depth,
        role_name(n.role),
        b.origin.x,
        b.origin.y,
        b.size.width,
        b.size.height
    ));
    let p = &n.properties;
    for (k, v) in [("label", &p.label), ("value", &p.value), ("description", &p.description), ("placeholder", &p.placeholder), ("shortcut", &p.keyboard_shortcut)] {
        if let Some(v) = v.as_ref().filter(|v| !v.is_empty()) {
            out.push_str(&format!(",\"{k}\":\"{}\"", esc(v)));
        }
    }
    let st = &n.state;
    let mut flags = Vec::new();
    for (on, f) in [(st.disabled, "disabled"), (st.focused, "focused"), (st.pressed, "pressed"), (st.selected, "selected")] {
        if on {
            flags.push(f);
        }
    }
    match st.checked {
        Some(true) => flags.push("checked"),
        Some(false) => flags.push("unchecked"),
        None => {}
    }
    match st.expanded {
        Some(true) => flags.push("expanded"),
        Some(false) => flags.push("collapsed"),
        None => {}
    }
    if !flags.is_empty() {
        out.push_str(&format!(",\"state\":\"{}\"", flags.join(" ")));
    }
    out.push('}');
}

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\r' => o.push_str("\\r"),
            '\t' => o.push_str("\\t"),
            c if (c as u32) < 0x20 => o.push_str(&format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o
}

/// Запустить сокет (один раз на процесс; без XDG_RUNTIME_DIR — нет).
#[cfg(all(unix, not(target_os = "android"), not(target_arch = "wasm32")))]
pub fn start() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        if std::env::var_os("SYNGUI_NO_INSPECT").is_some() {
            return;
        }
        let Some(rt) = std::env::var_os("XDG_RUNTIME_DIR") else { return };
        let dir = std::path::PathBuf::from(rt).join("syngui-inspect");
        if std::fs::create_dir_all(&dir).is_err() {
            return;
        }
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
        // Сокеты умерших процессов.
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let alive = e.file_name().to_str().and_then(|n| n.strip_suffix(".sock")).and_then(|p| p.parse::<i32>().ok()).is_some_and(|p| unsafe { libc_kill(p, 0) } == 0);
                if !alive {
                    let _ = std::fs::remove_file(e.path());
                }
            }
        }
        let path = dir.join(format!("{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let Ok(listener) = std::os::unix::net::UnixListener::bind(&path) else { return };
        let _ = std::thread::Builder::new().name("syngui-inspect".into()).spawn(move || {
            for conn in listener.incoming().flatten() {
                let _ = std::thread::Builder::new().name("syngui-inspect-c".into()).spawn(move || serve(conn));
            }
        });
    });
}

#[cfg(not(all(unix, not(target_os = "android"), not(target_arch = "wasm32"))))]
pub fn start() {}

#[cfg(all(unix, not(target_os = "android"), not(target_arch = "wasm32")))]
extern "C" {
    #[link_name = "kill"]
    fn libc_kill(pid: i32, sig: i32) -> i32;
}

#[cfg(all(unix, not(target_os = "android"), not(target_arch = "wasm32")))]
fn serve(conn: std::os::unix::net::UnixStream) {
    use std::io::{BufRead, BufReader, Write};
    let _ = conn.set_read_timeout(Some(Duration::from_secs(30)));
    let Ok(wr) = conn.try_clone() else { return };
    let mut wr = wr;
    let mut rd = BufReader::new(conn);
    let mut line = String::new();
    while rd.read_line(&mut line).map(|n| n > 0).unwrap_or(false) {
        let req = line.trim().to_string();
        line.clear();
        let reply = handle(&req);
        if wr.write_all(reply.as_bytes()).and_then(|_| wr.write_all(b"\n")).is_err() {
            break;
        }
    }
}

fn handle(req: &str) -> String {
    let mut parts = req.splitn(3, ' ');
    match parts.next().unwrap_or("") {
        "tree" => tree_json(),
        op @ ("click" | "set" | "focus") => {
            let Some(id) = parts.next().and_then(|s| s.parse::<u64>().ok()) else { return err("нужен id узла") };
            let action = match op {
                "click" => Action::Click,
                "focus" => Action::Focus,
                _ => Action::SetValue(parts.next().unwrap_or("").to_string()),
            };
            if let Ok(mut g) = ACTIONS.lock() {
                g.push((0, A11yId(id), action));
            }
            // Окно выполнит на ближайшем кадре.
            WANT.fetch_add(1, Ordering::AcqRel);
            crate::async_runtime::run_on_main_thread(|| {});
            let t0 = Instant::now();
            while t0.elapsed() < Duration::from_millis(1500) {
                if ACTIONS.lock().map(|g| g.is_empty()).unwrap_or(true) {
                    return "{\"ok\":true}".into();
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            if let Ok(mut g) = ACTIONS.lock() {
                g.clear();
            }
            err("окно не выполнило действие (не рисуется?)")
        }
        _ => err("запросы: tree, click <id>, set <id> <текст>, focus <id>"),
    }
}

fn err(m: &str) -> String {
    format!("{{\"ok\":false,\"error\":\"{}\"}}", esc(m))
}

/// Свежее дерево всех окон процесса.
fn tree_json() -> String {
    let want = WANT.fetch_add(1, Ordering::AcqRel) + 1;
    crate::async_runtime::run_on_main_thread(|| {});
    let t0 = Instant::now();
    loop {
        let fresh = SNAPS.lock().ok().map(|g| g.as_ref().map(|m| !m.is_empty() && m.values().all(|s| s.seq >= want)).unwrap_or(false)).unwrap_or(false);
        if fresh || t0.elapsed() > Duration::from_millis(800) {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let g = match SNAPS.lock() {
        Ok(g) => g,
        Err(_) => return err("снимок недоступен"),
    };
    let mut s = format!("{{\"ok\":true,\"pid\":{},\"windows\":[", std::process::id());
    if let Some(m) = g.as_ref() {
        let mut keys: Vec<&u64> = m.keys().collect();
        keys.sort();
        for (i, k) in keys.into_iter().enumerate() {
            let w = &m[k];
            if i > 0 {
                s.push(',');
            }
            s.push_str(&format!("{{\"key\":{k},\"fresh\":{},\"size\":[{:.0},{:.0}],\"nodes\":{}}}", w.seq >= want, w.size.0, w.size.1, w.json_nodes));
        }
    }
    s.push_str("]}");
    s
}

#[cfg(test)]
mod tests {
    #[test]
    fn escape() {
        assert_eq!(super::esc("a\"b\\c\nd\u{1}"), "a\\\"b\\\\c\\nd\\u0001");
    }
}

/// Опубликованные узлы окна (для тестов).
#[cfg(test)]
pub(crate) fn snapshot_json(key: u64) -> Option<String> {
    SNAPS.lock().ok()?.as_ref()?.get(&key).map(|w| w.json_nodes.clone())
}

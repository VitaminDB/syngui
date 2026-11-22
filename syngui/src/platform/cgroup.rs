//! Отрыв процесса от чужой cgroup systemd.
//!
//! `setsid`, `nohup`, `disown` отвязывают процесс от терминала и группы
//! процессов, но не от cgroup: процесс, запущенный из службы systemd (например
//! из сессии агента, которую держит `foo.service`), остаётся в её cgroup, и
//! `systemctl restart foo` (KillMode=control-group) убивает его вместе со
//! всеми потомками. Выход — перенести процесс в собственный transient scope
//! пользовательского менеджера (`StartTransientUnit` с `PIDs=[pid]`), как
//! делает `systemd-run --user --scope`.
//!
//! - [`move_to_own_scope`] — перенести текущий процесс в `<name>-<pid>.scope`;
//! - [`escape_service_cgroup`] — то же, но только если процесс сейчас живёт в
//!   cgroup службы (`*.service`): для приложений, которые могли запустить из
//!   службы, при обычном запуске из рабочего стола ничего не меняется.
//!
//! Без systemd (или без `busctl`) функции возвращают ошибку и ничего не
//! делают.

use std::io;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Путь cgroup v2 текущего процесса (`/user.slice/.../foo.service`).
pub fn current_cgroup() -> Option<String> {
    let s = std::fs::read_to_string("/proc/self/cgroup").ok()?;
    s.lines().find_map(|l| l.strip_prefix("0::").map(str::to_string))
}

/// Последний компонент cgroup — имя юнита (`foo.service`, `session-2.scope`).
pub fn current_unit() -> Option<String> {
    current_cgroup().and_then(|p| p.rsplit('/').next().map(str::to_string))
}

/// Имя юнита из произвольной строки: допустимы `[A-Za-z0-9:_.-]`.
fn sanitize(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, ':' | '_' | '.' | '-') { c } else { '-' })
        .collect();
    if s.is_empty() { "app".into() } else { s }
}

/// Перенести текущий процесс в собственный scope `<name>-<pid>.scope`
/// пользовательского менеджера systemd. Возвращает имя scope. Потомки,
/// запущенные после переноса, рождаются уже в нём.
pub fn move_to_own_scope(name: &str) -> io::Result<String> {
    let pid = std::process::id();
    let unit = format!("{}-{pid}.scope", sanitize(name));
    if current_unit().as_deref() == Some(unit.as_str()) {
        return Ok(unit);
    }
    let out = Command::new("busctl")
        .args([
            "--user",
            "call",
            "org.freedesktop.systemd1",
            "/org/freedesktop/systemd1",
            "org.freedesktop.systemd1.Manager",
            "StartTransientUnit",
            "ssa(sv)a(sa(sv))",
            &unit,
            "fail",
            "3",
            "PIDs",
            "au",
            "1",
            &pid.to_string(),
            "CollectMode",
            "s",
            "inactive-or-failed",
            "Description",
            "s",
            &format!("{name} (вне cgroup запустившей службы)"),
            "0",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()?;
    if !out.status.success() {
        return Err(io::Error::other(format!(
            "StartTransientUnit {unit}: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    // Перенос делает задание менеджера — дождаться, пока процесс окажется в scope.
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        if current_unit().as_deref() == Some(unit.as_str()) {
            return Ok(unit);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    Err(io::Error::other(format!("процесс не перенесён в {unit}")))
}

/// Если текущий процесс живёт в cgroup службы (`*.service` — его запустили
/// из службы или из сессии, которую она держит), перенести его в собственный
/// scope, чтобы остановка или перезапуск службы его не убили. `Ok(None)` —
/// переносить не нужно (scope рабочего стола, сессии и т. п.).
pub fn escape_service_cgroup(name: &str) -> io::Result<Option<String>> {
    match current_unit() {
        Some(u) if u.ends_with(".service") => move_to_own_scope(name).map(Some),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_unit_name() {
        assert_eq!(sanitize("syngui-holder-synthos"), "syngui-holder-synthos");
        assert_eq!(sanitize("мой app"), "----app");
        assert_eq!(sanitize(""), "app");
    }
}

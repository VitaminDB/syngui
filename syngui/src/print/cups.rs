//! Связь с CUPS: список принтеров (`lpstat`), их описание (PPD по HTTP с
//! сервера CUPS), отправка задания (`lp`), очередь и отмена заданий.
//!
//! Без CUPS (телефон, нет пакета) список пуст — остаётся печать в PDF.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::time::Duration;

use super::ppd::{self, Ppd};

/// Принтер CUPS.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Printer {
    /// Имя очереди (`lp -d ИМЯ`).
    pub name: String,
    /// Описание («HP LaserJet Pro MFP 3101»); пусто — как имя.
    pub description: String,
    pub location: String,
    pub is_default: bool,
    pub state: PrinterState,
    /// Строка состояния от CUPS (`Alerts`, причина остановки).
    pub message: String,
    /// Адрес устройства (`ipp://…`, `usb://…`), если CUPS его сообщил.
    pub uri: String,
}

impl Printer {
    pub fn title(&self) -> &str {
        if self.description.is_empty() {
            &self.name
        } else {
            &self.description
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PrinterState {
    #[default]
    Idle,
    Printing,
    Disabled,
}

fn lp_command(program: &str) -> Command {
    let mut c = Command::new(program);
    c.env("LANG", "C").env("LC_ALL", "C").stdin(Stdio::null()).stderr(Stdio::piped());
    c
}

fn run(program: &str, args: &[&str]) -> Result<String, String> {
    let out = lp_command(program).args(args).output().map_err(|e| format!("{program}: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() { format!("{program}: {}", out.status) } else { err });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Есть ли CUPS (клиентские программы) на этой системе.
pub fn available() -> bool {
    lp_command("lpstat").arg("-r").stdout(Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
}

/// Принтеры CUPS. Ошибка — CUPS не установлен или не запущен.
pub fn printers() -> Result<Vec<Printer>, String> {
    let text = run("lpstat", &["-l", "-p"])?;
    let default = run("lpstat", &["-d"]).unwrap_or_default();
    let default = default.split_once(':').map(|(_, n)| n.trim().to_string()).unwrap_or_default();
    let uris = run("lpstat", &["-v"]).unwrap_or_default();
    let mut list = parse_printers(&text);
    for p in &mut list {
        p.is_default = p.name == default;
        p.uri = parse_uri(&uris, &p.name).unwrap_or_default();
    }
    list.sort_by(|a, b| b.is_default.cmp(&a.is_default).then_with(|| a.title().to_lowercase().cmp(&b.title().to_lowercase())));
    Ok(list)
}

fn parse_uri(text: &str, name: &str) -> Option<String> {
    for line in text.lines() {
        // `device for NAME: URI`
        let rest = line.strip_prefix("device for ")?;
        let (n, uri) = rest.split_once(':')?;
        if n.trim() == name {
            return Some(uri.trim().to_string());
        }
    }
    None
}

pub(crate) fn parse_printers(text: &str) -> Vec<Printer> {
    let mut list: Vec<Printer> = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("printer ") {
            let name = rest.split_whitespace().next().unwrap_or_default().to_string();
            let state = if rest.contains(" disabled") {
                PrinterState::Disabled
            } else if rest.contains(" now printing") {
                PrinterState::Printing
            } else {
                PrinterState::Idle
            };
            list.push(Printer { name, state, ..Default::default() });
            continue;
        }
        let Some(p) = list.last_mut() else { continue };
        let t = line.trim();
        if let Some(v) = t.strip_prefix("Description:") {
            p.description = v.trim().to_string();
        } else if let Some(v) = t.strip_prefix("Location:") {
            p.location = v.trim().to_string();
        } else if let Some(v) = t.strip_prefix("Alerts:") {
            let v = v.trim();
            if v != "none" && p.message.is_empty() {
                p.message = v.to_string();
            }
        } else if line.starts_with('\t') && !t.contains(':') && p.message.is_empty() && p.state == PrinterState::Disabled && t != "reason unknown" {
            // Причина остановки — строкой сразу под заголовком.
            p.message = t.to_string();
        }
    }
    list
}

/// Адрес сервера CUPS (`CUPS_SERVER` или localhost:631).
fn server() -> String {
    std::env::var("CUPS_SERVER").ok().filter(|s| !s.is_empty() && !s.starts_with('/')).unwrap_or_else(|| "localhost:631".into())
}

fn http_get(path: &str) -> Result<Vec<u8>, String> {
    let req = format!("GET {path} HTTP/1.0\r\nHost: localhost\r\nUser-Agent: syngui\r\n\r\n");
    let mut buf = Vec::new();
    let host = server();
    let via_tcp = (|| -> std::io::Result<()> {
        use std::net::ToSocketAddrs;
        let addr = host.to_socket_addrs()?.next().ok_or_else(|| std::io::Error::other("нет адреса"))?;
        let mut s = std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(2))?;
        s.set_read_timeout(Some(Duration::from_secs(5)))?;
        s.write_all(req.as_bytes())?;
        s.read_to_end(&mut buf)?;
        Ok(())
    })();
    #[cfg(unix)]
    if via_tcp.is_err() {
        buf.clear();
        let mut s = std::os::unix::net::UnixStream::connect("/run/cups/cups.sock").map_err(|e| e.to_string())?;
        s.set_read_timeout(Some(Duration::from_secs(5))).ok();
        s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
        s.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    via_tcp.map_err(|e| e.to_string())?;
    let split = buf.windows(4).position(|w| w == b"\r\n\r\n").ok_or("CUPS: неполный ответ")?;
    let head = String::from_utf8_lossy(&buf[..split]);
    let status = head.split_whitespace().nth(1).unwrap_or("");
    if status != "200" {
        return Err(format!("CUPS: HTTP {status}"));
    }
    Ok(buf[split + 4..].to_vec())
}

/// Описание принтера (PPD). Ошибка — у очереди нет PPD (сырая очередь) или CUPS недоступен.
pub fn printer_ppd(name: &str) -> Result<Ppd, String> {
    let body = http_get(&format!("/printers/{}.ppd", url_escape(name)))?;
    Ok(ppd::parse(&String::from_utf8_lossy(&body)))
}

fn url_escape(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Отправить файл на печать: `options` — пары `-o КЛЮЧ=ЗНАЧЕНИЕ`.
/// Возвращает номер задания (`HP-12`).
pub fn submit(printer: &str, file: &std::path::Path, title: &str, copies: u32, options: &[(String, String)]) -> Result<String, String> {
    let mut c = lp_command("lp");
    c.arg("-d").arg(printer).arg("-t").arg(title).arg("-n").arg(copies.max(1).to_string());
    for (k, v) in options {
        c.arg("-o").arg(format!("{k}={v}"));
    }
    c.arg("--").arg(file);
    let out = c.output().map_err(|e| format!("lp: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() { format!("lp: {}", out.status) } else { err });
    }
    // `request id is HP-12 (1 file(s))`
    let s = String::from_utf8_lossy(&out.stdout);
    Ok(s.split_whitespace().skip_while(|w| *w != "is").nth(1).unwrap_or("").to_string())
}

/// Задание в очереди.
#[derive(Debug, Clone, PartialEq)]
pub struct Job {
    /// `HP-12`.
    pub id: String,
    pub printer: String,
    pub user: String,
    pub size: u64,
}

/// Незавершённые задания (все принтеры или один).
pub fn jobs(printer: Option<&str>) -> Result<Vec<Job>, String> {
    let mut args = vec!["-o"];
    if let Some(p) = printer {
        args.push(p);
    }
    let text = run("lpstat", &args)?;
    let names: Vec<String> = printers().unwrap_or_default().into_iter().map(|p| p.name).collect();
    Ok(text
        .lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let id = it.next()?.to_string();
            let user = it.next().unwrap_or_default().to_string();
            let size = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
            let printer = names
                .iter()
                .filter(|n| id.starts_with(&format!("{n}-")))
                .max_by_key(|n| n.len())
                .cloned()
                .unwrap_or_else(|| id.rsplit_once('-').map(|(p, _)| p.to_string()).unwrap_or_default());
            Some(Job { id, printer, user, size })
        })
        .collect())
}

/// Отменить задание.
pub fn cancel_job(id: &str) -> Result<(), String> {
    run("cancel", &[id]).map(|_| ())
}

/// Сделать принтер принтером по умолчанию (для пользователя: `lpoptions -d`).
pub fn set_default(name: &str) -> Result<(), String> {
    run("lpoptions", &["-d", name]).map(|_| ())
}

/// Настройки принтера по умолчанию, сохранённые пользователем (`lpoptions -p ИМЯ`).
pub fn user_defaults(name: &str) -> Vec<(String, String)> {
    let Ok(text) = run("lpoptions", &["-p", name]) else { return Vec::new() };
    split_lpoptions(&text)
}

/// Сохранить настройки принтера по умолчанию для пользователя (`~/.cups/lpoptions`).
pub fn set_user_defaults(name: &str, options: &[(String, String)]) -> Result<(), String> {
    if options.is_empty() {
        return Ok(());
    }
    let mut args: Vec<String> = vec!["-p".into(), name.into()];
    for (k, v) in options {
        args.push("-o".into());
        args.push(format!("{k}={v}"));
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run("lpoptions", &refs).map(|_| ())
}

/// `a=b c='d e' f` → пары.
fn split_lpoptions(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut chars = text.trim().chars().peekable();
    while chars.peek().is_some() {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        let mut key = String::new();
        while let Some(&c) = chars.peek() {
            if c == '=' || c.is_whitespace() {
                break;
            }
            key.push(c);
            chars.next();
        }
        let mut val = String::new();
        if chars.peek() == Some(&'=') {
            chars.next();
            let quote = match chars.peek() {
                Some(&q @ ('\'' | '"')) => {
                    chars.next();
                    Some(q)
                }
                _ => None,
            };
            while let Some(c) = chars.next() {
                match quote {
                    Some(q) if c == q => break,
                    None if c.is_whitespace() => break,
                    _ => val.push(c),
                }
            }
        }
        if !key.is_empty() {
            out.push((key, val));
        }
    }
    out
}

/// Включить/выключить очередь (`cupsenable`/`cupsdisable`). Нужны права администратора CUPS.
pub fn set_enabled(name: &str, enabled: bool) -> Result<(), String> {
    run(if enabled { "cupsenable" } else { "cupsdisable" }, &[name]).map(|_| ())
}

/// Пробная страница CUPS.
pub fn print_test_page(name: &str) -> Result<String, String> {
    let candidates = ["/usr/share/cups/data/testprint", "/usr/share/cups/data/testprint.ps"];
    let file = candidates.iter().map(std::path::Path::new).find(|p| p.exists()).ok_or("нет пробной страницы CUPS")?;
    submit(name, file, "Test page", 1, &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printers_from_lpstat() {
        let text = "printer HP is idle.  enabled since Tue\n\tForm mounted:\n\tDescription: HP LaserJet\n\tAlerts: none\n\tLocation: 192.168.1.4\n\
printer Pana is disabled since Mon -\n\tPaused\n\tDescription: Panasonic\n\tAlerts: cups-missing-filter-warning\n";
        let p = parse_printers(text);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].title(), "HP LaserJet");
        assert_eq!(p[0].location, "192.168.1.4");
        assert_eq!(p[0].state, PrinterState::Idle);
        assert!(p[0].message.is_empty());
        assert_eq!(p[1].state, PrinterState::Disabled);
        assert_eq!(p[1].message, "Paused");
    }

    #[test]
    fn lpoptions_split() {
        let v = split_lpoptions("copies=1 PageSize=A4 printer-info='HP LaserJet' job-sheets=none,none");
        assert_eq!(v[1], ("PageSize".into(), "A4".into()));
        assert_eq!(v[2], ("printer-info".into(), "HP LaserJet".into()));
        assert_eq!(v.len(), 4);
    }

    #[test]
    fn uri_lookup() {
        let t = "device for HP: ipp://192.168.1.4/ipp/print\ndevice for Sam: usb://Samsung/ML?serial=1\n";
        assert_eq!(parse_uri(t, "Sam").unwrap(), "usb://Samsung/ML?serial=1");
    }
}

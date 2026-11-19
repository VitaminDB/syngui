//! Хранитель терминалов: отдельный процесс, который держит PTY и их процессы,
//! пока приложение с интерфейсом перезапускается или падает (как сервер
//! tmux/zellij). Приложение подключает [`TerminalSession`] к хранителю
//! через [`TerminalConfig::holder`]; при следующем запуске сессия с тем же
//! ключом подключается к живому PTY — экран восстанавливается из последнего
//! вывода, а программа (оболочка, редактор, агент) продолжает работать.
//!
//! Хранитель запускается самим приложением при первом подключении
//! ([`HolderSpec::spawn`] — обычно `["/proc/self/exe", "--pty-holder", <сокет>]`,
//! а в `main` приложение вызывает [`run_holder`]), живёт, пока есть хотя бы
//! один терминал или подключение, и завершается сам.
//!
//! Протокол — кадры `[тег u8][длина u32 LE][данные]` по unix-сокету.
//!
//! [`TerminalSession`]: super::TerminalSession
//! [`TerminalConfig::holder`]: super::TerminalConfig::holder

use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, Receiver, RecvTimeoutError};

use super::pty::PtySession;

/// Как подключать терминал к хранителю.
#[derive(Clone, Debug)]
pub struct HolderSpec {
    /// Путь unix-сокета хранителя.
    pub socket: PathBuf,
    /// Ключ терминала: тот же ключ при следующем запуске — подключение к тому
    /// же PTY. Должен быть уникален в пределах хранителя.
    pub key: String,
    /// Произвольные данные приложения (проект, заголовок…), хранятся вместе
    /// с терминалом и возвращаются [`list`] — по ним приложение находит
    /// терминалы, которых нет в его сохранённом составе вкладок.
    pub meta: String,
    /// Команда запуска хранителя, если он не отвечает (`argv`, первый
    /// элемент — программа). Пусто — не запускать.
    pub spawn: Vec<String>,
    /// `false` — только подключиться к существующему терминалу: нет такого
    /// ключа — терминал сразу «завершён», новый процесс не запускается.
    pub create: bool,
}

/// Терминал в хранителе (ответ [`list`]).
#[derive(Clone, Debug)]
pub struct HeldInfo {
    pub key: String,
    pub pid: u32,
    pub meta: String,
    /// К терминалу сейчас подключено приложение.
    pub attached: bool,
}

// Клиент → хранитель
const T_OPEN: u8 = 1;
const T_LIST: u8 = 2;
const T_INPUT: u8 = 3;
const T_RESIZE: u8 = 4;
const T_KILL: u8 = 5;
const T_META: u8 = 6;
const T_KILL_KEY: u8 = 7;
// Хранитель → клиент
const T_OK: u8 = 10;
const T_NOTFOUND: u8 = 11;
const T_OUTPUT: u8 = 12;
const T_EXIT: u8 = 13;
const T_LISTED: u8 = 14;
const T_ERROR: u8 = 15;

/// Сколько последнего вывода хранить для восстановления экрана.
const REPLAY_BYTES: usize = 1 << 20;
/// Хранитель без терминалов и подключений завершается через это время.
const IDLE_EXIT: Duration = Duration::from_secs(5);

// ── кадры ───────────────────────────────────────────────────────────────

fn write_frame(w: &mut impl Write, tag: u8, payload: &[u8]) -> io::Result<()> {
    let mut buf = Vec::with_capacity(5 + payload.len());
    buf.push(tag);
    buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    buf.extend_from_slice(payload);
    w.write_all(&buf)?;
    w.flush()
}

fn read_frame(r: &mut impl Read) -> io::Result<(u8, Vec<u8>)> {
    let mut head = [0u8; 5];
    r.read_exact(&mut head)?;
    let len = u32::from_le_bytes([head[1], head[2], head[3], head[4]]) as usize;
    if len > 64 << 20 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "кадр слишком большой"));
    }
    let mut payload = vec![0u8; len];
    r.read_exact(&mut payload)?;
    Ok((head[0], payload))
}

#[derive(Default)]
struct Enc(Vec<u8>);

impl Enc {
    fn u8(mut self, v: u8) -> Self {
        self.0.push(v);
        self
    }
    fn u16(mut self, v: u16) -> Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn u32(mut self, v: u32) -> Self {
        self.0.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn str(self, s: &str) -> Self {
        let mut e = self.u32(s.len() as u32);
        e.0.extend_from_slice(s.as_bytes());
        e
    }
}

struct Dec<'a>(&'a [u8]);

impl Dec<'_> {
    fn take(&mut self, n: usize) -> io::Result<&[u8]> {
        if self.0.len() < n {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "обрезанный кадр"));
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Ok(a)
    }
    fn u8(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> io::Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> io::Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn str(&mut self) -> io::Result<String> {
        let n = self.u32()? as usize;
        Ok(String::from_utf8_lossy(self.take(n)?).into_owned())
    }
}

/// Параметры открытия терминала.
pub(super) struct OpenRequest<'a> {
    pub command: &'a str,
    pub args: &'a [String],
    pub cwd: Option<&'a Path>,
    pub env: &'a [(String, String)],
    pub cols: u16,
    pub rows: u16,
}

fn encode_open(spec: &HolderSpec, req: &OpenRequest<'_>) -> Vec<u8> {
    let mut e = Enc::default()
        .str(&spec.key)
        .u8(spec.create as u8)
        .str(req.command)
        .u32(req.args.len() as u32);
    for a in req.args {
        e = e.str(a);
    }
    e = e
        .str(&req.cwd.map(|p| p.to_string_lossy().into_owned()).unwrap_or_default())
        .u32(req.env.len() as u32);
    for (k, v) in req.env {
        e = e.str(k).str(v);
    }
    e.u16(req.cols).u16(req.rows).str(&spec.meta).0
}

// ── клиент ──────────────────────────────────────────────────────────────

/// Терминал, процесс которого держит хранитель. Drop — только отключение:
/// процесс продолжает жить; завершить его — [`HeldPty::kill`].
pub(super) struct HeldPty {
    stream: UnixStream,
    pid: u32,
    alive: Arc<AtomicBool>,
}

impl HeldPty {
    /// Подключиться к терминалу `spec.key` (или создать его). `Ok(None)` —
    /// такого терминала нет, а создавать не просили.
    pub fn open(
        spec: &HolderSpec,
        req: &OpenRequest<'_>,
    ) -> io::Result<Option<(Self, Receiver<Vec<u8>>)>> {
        let mut stream = connect_or_spawn(&spec.socket, &spec.spawn)?;
        write_frame(&mut stream, T_OPEN, &encode_open(spec, req))?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        let (tag, payload) = read_frame(&mut stream)?;
        stream.set_read_timeout(None)?;
        let pid = match tag {
            T_OK => Dec(&payload).u32()?,
            T_NOTFOUND => return Ok(None),
            T_ERROR => {
                return Err(io::Error::other(String::from_utf8_lossy(&payload).into_owned()))
            }
            _ => return Err(io::Error::new(io::ErrorKind::InvalidData, "неожиданный ответ")),
        };
        let alive = Arc::new(AtomicBool::new(true));
        let (tx, rx) = bounded::<Vec<u8>>(64);
        let mut reader = stream.try_clone()?;
        let alive_r = alive.clone();
        thread::Builder::new()
            .name("syngui-held-reader".into())
            .spawn(move || {
                while let Ok((tag, payload)) = read_frame(&mut reader) {
                    match tag {
                        T_OUTPUT => {
                            if tx.send(payload).is_err() {
                                break;
                            }
                        }
                        T_EXIT => break,
                        _ => {}
                    }
                }
                alive_r.store(false, Ordering::Release);
            })?;
        Ok(Some((Self { stream, pid, alive }, rx)))
    }

    pub fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        write_frame(&mut self.stream, T_INPUT, bytes)
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        let _ = write_frame(&mut &self.stream, T_RESIZE, &Enc::default().u16(cols).u16(rows).0);
    }

    pub fn set_meta(&self, meta: &str) {
        let _ = write_frame(&mut &self.stream, T_META, meta.as_bytes());
    }

    /// Завершить процесс терминала (SIGHUP, затем SIGKILL — в хранителе).
    pub fn kill(&self) {
        let _ = write_frame(&mut &self.stream, T_KILL, &[]);
    }

    pub fn pid(&self) -> Option<u32> {
        (self.pid > 0).then_some(self.pid)
    }

    pub fn is_alive(&self) -> bool {
        self.alive.load(Ordering::Acquire)
    }

    /// Запущена ли команда поверх оболочки: группа переднего плана tty
    /// (`tpgid` из `/proc/<pid>/stat`) отличается от оболочки.
    pub fn has_foreground_child(&self) -> bool {
        foreground_differs(self.pid)
    }
}

impl Drop for HeldPty {
    fn drop(&mut self) {
        let _ = self.stream.shutdown(std::net::Shutdown::Both);
    }
}

fn foreground_differs(pid: u32) -> bool {
    #[cfg(target_os = "linux")]
    {
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
            return false;
        };
        // После «(comm)»: state ppid pgrp session tty_nr tpgid
        let Some(rest) = stat.rsplit_once(')').map(|(_, r)| r) else {
            return false;
        };
        let tpgid = rest.split_whitespace().nth(5).and_then(|v| v.parse::<i64>().ok());
        matches!(tpgid, Some(t) if t > 0 && t != i64::from(pid))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pid;
        false
    }
}

fn connect_or_spawn(socket: &Path, spawn: &[String]) -> io::Result<UnixStream> {
    if let Ok(s) = UnixStream::connect(socket) {
        return Ok(s);
    }
    let Some((prog, args)) = spawn.split_first() else {
        return UnixStream::connect(socket);
    };
    spawn_detached(prog, args)?;
    let start = Instant::now();
    loop {
        match UnixStream::connect(socket) {
            Ok(s) => return Ok(s),
            Err(e) if start.elapsed() > Duration::from_secs(5) => return Err(e),
            Err(_) => thread::sleep(Duration::from_millis(30)),
        }
    }
}

/// Запуск хранителя отдельной сессией (`setsid`): он не получает сигналы
/// группы приложения и переживает его падение.
fn spawn_detached(prog: &str, args: &[String]) -> io::Result<()> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    let mut cmd = Command::new(prog);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: в pre_exec только async-signal-safe вызовы (setsid, close_range/close).
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            // Хранитель живёт дольше приложения: унаследованные дескрипторы
            // без CLOEXEC (блокировка single-instance, сокеты, файлы) держали
            // бы их после выхода приложения — новый запуск считал бы, что
            // приложение ещё работает.
            close_inherited_fds();
            Ok(())
        });
    }
    let mut child = cmd.spawn()?;
    // Снять зомби, когда хранитель завершится (родитель — мы).
    thread::Builder::new()
        .name("syngui-holder-reap".into())
        .spawn(move || {
            let _ = child.wait();
        })?;
    Ok(())
}

/// Закрыть все дескрипторы от 3 и выше (в дочернем процессе до exec).
unsafe fn close_inherited_fds() {
    #[cfg(any(target_os = "linux", target_os = "android"))]
    if libc::syscall(libc::SYS_close_range, 3u32, u32::MAX, 0u32) == 0 {
        return;
    }
    for fd in 3..4096 {
        libc::close(fd);
    }
}

/// Терминалы хранителя. Хранитель не запущен — пустой список.
pub fn list(socket: &Path) -> io::Result<Vec<HeldInfo>> {
    let mut stream = match UnixStream::connect(socket) {
        Ok(s) => s,
        Err(_) => return Ok(Vec::new()),
    };
    stream.set_read_timeout(Some(Duration::from_secs(3)))?;
    write_frame(&mut stream, T_LIST, &[])?;
    let (tag, payload) = read_frame(&mut stream)?;
    if tag != T_LISTED {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "неожиданный ответ"));
    }
    let mut d = Dec(&payload);
    let n = d.u32()?;
    let mut out = Vec::with_capacity(n as usize);
    for _ in 0..n {
        out.push(HeldInfo {
            key: d.str()?,
            pid: d.u32()?,
            meta: d.str()?,
            attached: d.u8()? != 0,
        });
    }
    Ok(out)
}

/// Завершить терминал хранителя по ключу (без подключения к нему).
pub fn kill(socket: &Path, key: &str) -> io::Result<()> {
    let mut stream = UnixStream::connect(socket)?;
    write_frame(&mut stream, T_KILL_KEY, key.as_bytes())
}

// ── хранитель ───────────────────────────────────────────────────────────

struct Client {
    id: u64,
    stream: UnixStream,
}

struct Held {
    pid: u32,
    meta: Mutex<String>,
    pty: Mutex<Option<PtySession>>,
    size: Mutex<(u16, u16)>,
    /// Последний вывод и подключённый клиент — под одним замком: повтор
    /// экрана при подключении и пересылка нового вывода не перемешиваются.
    out: Mutex<(Vec<u8>, Option<Client>)>,
}

#[derive(Default)]
struct Holder {
    sessions: Mutex<HashMap<String, Arc<Held>>>,
    connections: AtomicUsize,
    next_client: AtomicU64,
}

/// Запустить хранитель на сокете `socket` (блокирует до завершения). Если на
/// сокете уже отвечает другой хранитель — сразу `Ok(())`.
pub fn run_holder(socket: &Path) -> io::Result<()> {
    if UnixStream::connect(socket).is_ok() {
        return Ok(());
    }
    if let Some(dir) = socket.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let _ = std::fs::remove_file(socket);
    let listener = UnixListener::bind(socket)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600));
    }
    listener.set_nonblocking(true)?;
    // Сигналы не трогаем: SIG_IGN унаследовали бы оболочки терминалов, и
    // SIGHUP при закрытии вкладки перестал бы их завершать. От tty хранитель
    // сигналов не получает — он запущен своей сессией (setsid).
    let holder = Arc::new(Holder::default());
    let mut idle_since = Some(Instant::now());
    log::info!("[syngui holder] слушает {}", socket.display());
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(false);
                holder.connections.fetch_add(1, Ordering::AcqRel);
                let h = holder.clone();
                let _ = thread::Builder::new()
                    .name("syngui-holder-conn".into())
                    .spawn(move || {
                        let _ = serve_connection(&h, stream);
                        h.connections.fetch_sub(1, Ordering::AcqRel);
                    });
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                log::warn!("[syngui holder] accept: {e}");
                thread::sleep(Duration::from_millis(200));
            }
        }
        let busy = holder.connections.load(Ordering::Acquire) > 0
            || !holder.sessions.lock().map(|s| s.is_empty()).unwrap_or(true);
        if busy {
            idle_since = None;
        } else {
            let since = *idle_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= IDLE_EXIT {
                break;
            }
        }
    }
    // Сокет удаляем, только если он всё ещё наш.
    let _ = std::fs::remove_file(socket);
    log::info!("[syngui holder] нет терминалов — завершение");
    Ok(())
}

fn serve_connection(holder: &Arc<Holder>, mut stream: UnixStream) -> io::Result<()> {
    let (tag, payload) = read_frame(&mut stream)?;
    match tag {
        T_LIST => {
            let sessions = holder.sessions.lock().map(|s| {
                s.iter()
                    .map(|(k, h)| {
                        let attached = h.out.lock().map(|o| o.1.is_some()).unwrap_or(false);
                        let meta = h.meta.lock().map(|m| m.clone()).unwrap_or_default();
                        (k.clone(), h.pid, meta, attached)
                    })
                    .collect::<Vec<_>>()
            });
            let sessions = sessions.unwrap_or_default();
            let mut e = Enc::default().u32(sessions.len() as u32);
            for (k, pid, meta, attached) in &sessions {
                e = e.str(k).u32(*pid).str(meta).u8(*attached as u8);
            }
            write_frame(&mut stream, T_LISTED, &e.0)
        }
        T_KILL_KEY => {
            let key = String::from_utf8_lossy(&payload).into_owned();
            let held = holder.sessions.lock().ok().and_then(|s| s.get(&key).cloned());
            if let Some(h) = held {
                kill_held(&h);
            }
            Ok(())
        }
        T_OPEN => serve_terminal(holder, stream, &payload),
        _ => Ok(()),
    }
}

fn kill_held(h: &Held) {
    let pty = h.pty.lock().ok().and_then(|mut p| p.take());
    drop(pty); // Drop PtySession: SIGHUP группе и оболочке, затем SIGKILL
}

fn serve_terminal(holder: &Arc<Holder>, mut stream: UnixStream, payload: &[u8]) -> io::Result<()> {
    let mut d = Dec(payload);
    let key = d.str()?;
    let create = d.u8()? != 0;
    let command = d.str()?;
    let nargs = d.u32()?;
    let mut args = Vec::with_capacity(nargs as usize);
    for _ in 0..nargs {
        args.push(d.str()?);
    }
    let cwd = d.str()?;
    let nenv = d.u32()?;
    let mut env = Vec::with_capacity(nenv as usize);
    for _ in 0..nenv {
        env.push((d.str()?, d.str()?));
    }
    let cols = d.u16()?.max(1);
    let rows = d.u16()?.max(1);
    let meta = d.str()?;

    let existing = holder.sessions.lock().ok().and_then(|s| s.get(&key).cloned());
    let held = match existing {
        Some(h) => h,
        None if !create => return write_frame(&mut stream, T_NOTFOUND, &[]),
        None => {
            let cwd = (!cwd.is_empty()).then(|| PathBuf::from(&cwd));
            let (pty, rx) = match PtySession::open(&command, &args, cols, rows, cwd, &env) {
                Ok(p) => p,
                Err(e) => return write_frame(&mut stream, T_ERROR, e.to_string().as_bytes()),
            };
            let h = Arc::new(Held {
                pid: pty.pid().unwrap_or(0),
                meta: Mutex::new(meta),
                pty: Mutex::new(Some(pty)),
                size: Mutex::new((cols, rows)),
                out: Mutex::new((Vec::new(), None)),
            });
            if let Ok(mut s) = holder.sessions.lock() {
                s.insert(key.clone(), h.clone());
            }
            spawn_output_pump(holder.clone(), key.clone(), h.clone(), rx);
            h
        }
    };

    let client_id = holder.next_client.fetch_add(1, Ordering::AcqRel);
    {
        let mut out = held.out.lock().map_err(|_| io::Error::other("poisoned"))?;
        write_frame(&mut stream, T_OK, &Enc::default().u32(held.pid).0)?;
        if !out.0.is_empty() {
            write_frame(&mut stream, T_OUTPUT, &out.0)?;
        }
        // Новое подключение вытесняет прежнее (второе окно, «зависшее» старое).
        if let Some(old) = out.1.take() {
            let _ = old.stream.shutdown(std::net::Shutdown::Both);
        }
        out.1 = Some(Client {
            id: client_id,
            stream: stream.try_clone()?,
        });
    }
    // Размер окна мог измениться, а программам после повтора вывода нужна
    // полная перерисовка: SIGWINCH даже при том же размере (через ±1 строку).
    if let Some(p) = held.pty.lock().ok().as_ref().and_then(|p| p.as_ref()) {
        let mut size = held.size.lock().unwrap_or_else(|e| e.into_inner());
        if *size == (cols, rows) {
            p.resize(cols, rows.saturating_sub(1).max(1));
        }
        p.resize(cols, rows);
        *size = (cols, rows);
    }

    loop {
        let (tag, payload) = match read_frame(&mut stream) {
            Ok(f) => f,
            Err(_) => break,
        };
        match tag {
            T_INPUT => {
                if let Ok(mut p) = held.pty.lock() {
                    if let Some(p) = p.as_mut() {
                        let _ = p.write(&payload);
                    }
                }
            }
            T_RESIZE => {
                let mut d = Dec(&payload);
                if let (Ok(c), Ok(r)) = (d.u16(), d.u16()) {
                    if let Some(p) = held.pty.lock().ok().as_ref().and_then(|p| p.as_ref()) {
                        p.resize(c.max(1), r.max(1));
                    }
                    if let Ok(mut s) = held.size.lock() {
                        *s = (c.max(1), r.max(1));
                    }
                }
            }
            T_META => {
                if let Ok(mut m) = held.meta.lock() {
                    *m = String::from_utf8_lossy(&payload).into_owned();
                }
            }
            T_KILL => kill_held(&held),
            _ => {}
        }
    }
    // Отключение: терминал живёт дальше, отцепляем только этого клиента.
    if let Ok(mut out) = held.out.lock() {
        if out.1.as_ref().is_some_and(|c| c.id == client_id) {
            out.1 = None;
        }
    }
    Ok(())
}

fn spawn_output_pump(holder: Arc<Holder>, key: String, held: Arc<Held>, rx: Receiver<Vec<u8>>) {
    let _ = thread::Builder::new()
        .name("syngui-holder-pump".into())
        .spawn(move || {
            loop {
                match rx.recv_timeout(Duration::from_millis(500)) {
                    Ok(chunk) => {
                        let Ok(mut out) = held.out.lock() else { break };
                        let (buf, client) = &mut *out;
                        buf.extend_from_slice(&chunk);
                        if buf.len() > REPLAY_BYTES {
                            // Режем по границе строки, чтобы не начинать
                            // повтор с середины escape-последовательности.
                            let cut = buf.len() - REPLAY_BYTES;
                            let cut = buf[cut..]
                                .iter()
                                .position(|b| *b == b'\n')
                                .map_or(cut, |p| cut + p + 1);
                            buf.drain(..cut);
                        }
                        let failed = client
                            .as_mut()
                            .is_some_and(|c| write_frame(&mut c.stream, T_OUTPUT, &chunk).is_err());
                        if failed {
                            *client = None;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
            // Процесс завершился: убрать терминал и сообщить клиенту.
            if let Ok(mut s) = holder.sessions.lock() {
                if s.get(&key).is_some_and(|h| Arc::ptr_eq(h, &held)) {
                    s.remove(&key);
                }
            }
            kill_held(&held);
            if let Ok(mut out) = held.out.lock() {
                if let Some(mut c) = out.1.take() {
                    let _ = write_frame(&mut c.stream, T_EXIT, &[]);
                    let _ = c.stream.shutdown(std::net::Shutdown::Both);
                }
            }
        });
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::super::{TerminalConfig, TerminalSession};
    use super::*;

    fn wait_until(timeout: Duration, mut f: impl FnMut() -> bool) -> bool {
        let start = Instant::now();
        while start.elapsed() < timeout {
            if f() {
                return true;
            }
            thread::sleep(Duration::from_millis(20));
        }
        f()
    }

    fn screen(s: &TerminalSession) -> String {
        s.with_state_ref(|st| {
            let mut out = String::new();
            for r in 0..st.grid.rows() {
                out.extend(st.grid.line(r).iter().map(|c| c.ch));
                out.push('\n');
            }
            out
        })
    }

    fn held(socket: &Path, key: &str, create: bool) -> TerminalSession {
        let cfg = TerminalConfig {
            command: "/bin/sh".into(),
            args: vec![],
            env: vec![("PS1".into(), "$ ".into())],
            holder: Some(HolderSpec {
                socket: socket.to_path_buf(),
                key: key.into(),
                meta: format!("meta-{key}"),
                spawn: vec![],
                create,
            }),
            ..TerminalConfig::default()
        };
        let s = TerminalSession::new(cfg).unwrap();
        s.resize(80, 24);
        s
    }

    fn proc_alive(pid: u32) -> bool {
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .map(|st| !st.rsplit(')').next().unwrap_or("").trim_start().starts_with('Z'))
            .unwrap_or(false)
    }

    /// Терминал переживает «падение» приложения: handle отпущен без
    /// terminate — процесс жив, повторное подключение по ключу видит прежний
    /// экран и тот же процесс, ввод работает; terminate завершает процесс.
    #[test]
    fn terminal_survives_client_and_reattaches() {
        let dir = std::env::temp_dir().join(format!("syngui-holder-test-{}", std::process::id()));
        let socket = dir.join("h.sock");
        let sock = socket.clone();
        thread::spawn(move || run_holder(&sock).unwrap());
        assert!(wait_until(Duration::from_secs(3), || UnixStream::connect(&socket).is_ok()));

        let s = held(&socket, "k1", true);
        assert!(s.is_held());
        s.write(b"echo MARK-$((40+2))\n");
        assert!(wait_until(Duration::from_secs(3), || screen(&s).contains("MARK-42")));
        let pid = s.pid().unwrap();
        drop(s); // как падение GUI: отключение без завершения

        thread::sleep(Duration::from_millis(200));
        assert!(proc_alive(pid), "оболочка умерла вместе с клиентом");
        let listed = list(&socket).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].key, "k1");
        assert_eq!(listed[0].meta, "meta-k1");
        assert_eq!(listed[0].pid, pid);
        assert!(!listed[0].attached);

        let s2 = held(&socket, "k1", false);
        assert!(wait_until(Duration::from_secs(3), || screen(&s2).contains("MARK-42")), "{}", screen(&s2));
        assert_eq!(s2.pid(), Some(pid));
        s2.write(b"echo AGAIN-$((1+1))\n");
        assert!(wait_until(Duration::from_secs(3), || screen(&s2).contains("AGAIN-2")));

        // Неизвестный ключ без create — терминал сразу завершён, процесс не создан.
        let s3 = held(&socket, "nope", false);
        assert!(wait_until(Duration::from_secs(3), || s3.has_exited()));
        assert_eq!(list(&socket).unwrap().len(), 1);

        s2.terminate();
        assert!(wait_until(Duration::from_secs(4), || !proc_alive(pid)), "terminate не завершил оболочку");
        assert!(wait_until(Duration::from_secs(3), || s2.has_exited()));
        assert!(wait_until(Duration::from_secs(3), || list(&socket).unwrap().is_empty()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Выход процесса в хранителе — клиент получает «завершён», без паники.
    #[test]
    fn held_process_exit_reaches_client() {
        let dir = std::env::temp_dir().join(format!("syngui-holder-test2-{}", std::process::id()));
        let socket = dir.join("h.sock");
        let sock = socket.clone();
        thread::spawn(move || run_holder(&sock).unwrap());
        assert!(wait_until(Duration::from_secs(3), || UnixStream::connect(&socket).is_ok()));
        let s = held(&socket, "x", true);
        s.write(b"exit\n");
        assert!(wait_until(Duration::from_secs(3), || s.has_exited()));
        assert!(wait_until(Duration::from_secs(3), || list(&socket).unwrap().is_empty()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

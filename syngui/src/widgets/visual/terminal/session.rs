use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crossbeam_channel::{Receiver, RecvTimeoutError};
use vte::Parser;

use crate::core::sync::Mutex;
use crate::signal::RwSignal;

use super::config::TerminalConfig;
use super::grid::Grid;
use super::parser::Performer;
use super::pty::{default_shell, PtyError, PtySession};
use super::selection::SelectionState;

/// Где живёт PTY: в приложении или в процессе-хранителе.
enum Backend {
    Local(PtySession),
    #[cfg(unix)]
    Held(super::holder::HeldPty),
}

impl Backend {
    fn write(&mut self, bytes: &[u8]) -> Result<(), String> {
        match self {
            Backend::Local(p) => p.write(bytes).map_err(|e| e.to_string()),
            #[cfg(unix)]
            Backend::Held(p) => p.write(bytes).map_err(|e| e.to_string()),
        }
    }
    fn resize(&self, cols: u16, rows: u16) {
        match self {
            Backend::Local(p) => p.resize(cols, rows),
            #[cfg(unix)]
            Backend::Held(p) => p.resize(cols, rows),
        }
    }
    fn pid(&self) -> Option<u32> {
        match self {
            Backend::Local(p) => p.pid(),
            #[cfg(unix)]
            Backend::Held(p) => p.pid(),
        }
    }
    fn is_alive(&self) -> bool {
        match self {
            Backend::Local(p) => p.is_alive(),
            #[cfg(unix)]
            Backend::Held(p) => p.is_alive(),
        }
    }
    fn has_foreground_child(&self) -> bool {
        match self {
            Backend::Local(p) => p.has_foreground_child(),
            #[cfg(unix)]
            Backend::Held(p) => p.has_foreground_child(),
        }
    }
}

pub(super) struct SessionState {
    pub(super) grid: Grid,
    pub(super) parser: Parser,
    pub(super) selection: SelectionState,
    pub(super) scroll_offset: usize,
    pub(super) focused: bool,
    pub(super) last_on_alt: bool,
    pub(super) title: Option<String>,
    pub(super) title_revision: u64,
}

struct SessionShared {
    state: Mutex<SessionState>,
    pty: Mutex<Option<Backend>>,
    revision: AtomicU64,
    title_signal: Mutex<Option<RwSignal<String>>>,
    exit_signal: Mutex<Option<RwSignal<bool>>>,
    exited: AtomicBool,
    stop_flag: AtomicBool,
    pump_join: Mutex<Option<JoinHandle<()>>>,
    pending_config: Mutex<Option<TerminalConfig>>,
    /// Ввод до запуска PTY (он стартует, когда терминал впервые показан) —
    /// отправляется при запуске, а не теряется.
    pending_input: Mutex<Vec<u8>>,
    autofocus_consumed: AtomicBool,
}

#[derive(Clone)]
pub struct TerminalSession {
    inner: Arc<SessionShared>,
}

const PUMP_BATCH_BYTES: usize = 16 * 1024;
const PUMP_RECV_TIMEOUT: Duration = Duration::from_millis(50);

impl TerminalSession {
    pub fn new(mut config: TerminalConfig) -> Result<Self, PtyError> {
        if config.command.is_empty() {
            config.command = default_shell();
        }
        let mut grid = Grid::new(1, 1);
        grid.set_max_scrollback(config.scrollback);
        let state = SessionState {
            grid,
            parser: Parser::new(),
            selection: SelectionState::default(),
            scroll_offset: 0,
            focused: false,
            last_on_alt: false,
            title: None,
            title_revision: 0,
        };
        let shared = Arc::new(SessionShared {
            state: Mutex::new(state),
            pty: Mutex::new(None),
            revision: AtomicU64::new(0),
            title_signal: Mutex::new(None),
            exit_signal: Mutex::new(None),
            exited: AtomicBool::new(false),
            stop_flag: AtomicBool::new(false),
            pump_join: Mutex::new(None),
            pending_input: Mutex::new(Vec::new()),
            pending_config: Mutex::new(Some(config)),
            autofocus_consumed: AtomicBool::new(false),
        });
        Ok(Self { inner: shared })
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }

    pub fn try_consume_autofocus(&self) -> bool {
        self.inner
            .autofocus_consumed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub fn reset_autofocus(&self) {
        self.inner
            .autofocus_consumed
            .store(false, Ordering::Release);
    }

    fn ensure_spawned(&self, cols: u16, rows: u16) {
        let config = match self.inner.pending_config.lock() {
            Ok(mut g) => match g.take() {
                Some(c) => c,
                None => return,
            },
            Err(_) => return,
        };
        let cols = cols.max(1);
        let rows = rows.max(1);
        let (pty, rx) = match open_backend(&config, cols, rows) {
            Some(pair) => pair,
            None => {
                mark_exited(&self.inner);
                return;
            }
        };
        if let Ok(mut s) = self.inner.state.lock() {
            s.grid.resize(cols as usize, rows as usize);
        }
        if let Ok(mut pty_slot) = self.inner.pty.lock() {
            let mut pty = pty;
            let queued = self
                .inner
                .pending_input
                .lock()
                .map(|mut q| std::mem::take(&mut *q))
                .unwrap_or_default();
            if !queued.is_empty() {
                if let Err(e) = pty.write(&queued) {
                    log::error!("[syngui terminal] pty write error: {e}");
                }
            }
            *pty_slot = Some(pty);
        }
        let pump = spawn_pump(Arc::downgrade(&self.inner), rx);
        if let Ok(mut guard) = self.inner.pump_join.lock() {
            *guard = Some(pump);
        }
        self.inner.revision.fetch_add(1, Ordering::Release);
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        let cols = cols.max(1);
        let rows = rows.max(1);

        let needs_spawn = self
            .inner
            .pending_config
            .lock()
            .map(|g| g.is_some())
            .unwrap_or(false);
        if needs_spawn {
            self.ensure_spawned(cols, rows);
            return;
        }

        let mut changed = false;
        if let Ok(mut s) = self.inner.state.lock() {
            if s.grid.cols() != cols as usize || s.grid.rows() != rows as usize {
                s.grid.resize(cols as usize, rows as usize);
                // Увеличение возвращает строки из истории на экран.
                s.scroll_offset = s.scroll_offset.min(s.grid.scrollback_len());
                changed = true;
            }
        }
        if changed {
            if let Ok(pty) = self.inner.pty.lock() {
                if let Some(p) = pty.as_ref() {
                    p.resize(cols, rows);
                }
            }
            self.inner.revision.fetch_add(1, Ordering::Release);
        }
    }

    pub fn write(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        if let Ok(mut pty) = self.inner.pty.lock() {
            match pty.as_mut() {
                Some(p) => {
                    if let Err(e) = p.write(bytes) {
                        log::error!("[syngui terminal] pty write error: {e}");
                    }
                }
                // PTY ещё не запущен — в очередь
                None if !self.inner.exited.load(Ordering::Acquire) => {
                    if let Ok(mut q) = self.inner.pending_input.lock() {
                        q.extend_from_slice(bytes);
                    }
                }
                None => {}
            }
        }
    }

    /// Завершить процесс терминала. Для локального PTY — то же, что
    /// отпустить последний handle; для терминала в хранителе — единственный
    /// способ его завершить (Drop лишь отключается, процесс живёт дальше).
    /// Вызывать при закрытии вкладки пользователем.
    pub fn terminate(&self) {
        if let Ok(mut cfg) = self.inner.pending_config.lock() {
            // Ещё не запущен — и не запускать.
            if cfg.take().is_some() {
                mark_exited(&self.inner);
                return;
            }
        }
        let pty = self.inner.pty.lock().ok().and_then(|mut p| p.take());
        match pty {
            #[cfg(unix)]
            Some(Backend::Held(p)) => p.kill(),
            Some(Backend::Local(p)) => drop(p),
            None => {}
        }
    }

    /// PTY держит процесс-хранитель (терминал переживёт перезапуск приложения).
    pub fn is_held(&self) -> bool {
        #[cfg(unix)]
        {
            matches!(self.inner.pty.lock().ok().as_deref(), Some(Some(Backend::Held(_))))
        }
        #[cfg(not(unix))]
        {
            false
        }
    }

    /// Обновить данные приложения у терминала в хранителе ([`super::holder::HolderSpec::meta`]).
    pub fn set_holder_meta(&self, meta: &str) {
        #[cfg(unix)]
        if let Ok(mut cfg) = self.inner.pending_config.lock() {
            if let Some(spec) = cfg.as_mut().and_then(|c| c.holder.as_mut()) {
                spec.meta = meta.to_string();
                return;
            }
        }
        #[cfg(unix)]
        if let Ok(p) = self.inner.pty.lock() {
            if let Some(Backend::Held(h)) = p.as_ref() {
                h.set_meta(meta);
            }
        }
        #[cfg(not(unix))]
        let _ = meta;
    }

    /// Выделен ли текст (для пункта «Копировать» в меню приложения).
    pub fn has_selection(&self) -> bool {
        self.with_state_ref(|st| st.selection.is_active())
    }

    pub fn revision(&self) -> u64 {
        self.inner.revision.load(Ordering::Acquire)
    }

    pub fn is_alive(&self) -> bool {
        if let Ok(pty) = self.inner.pty.lock() {
            pty.as_ref().map(|p| p.is_alive()).unwrap_or(false)
        } else {
            false
        }
    }

    /// `true`, если в терминале прямо сейчас выполняется команда (foreground
    /// process group tty отличается от shell'а — см.
    /// [`PtySession::has_foreground_child`]). Для не-спавнутого или мёртвого
    /// PTY — `false` («простаивает»).
    pub fn is_busy(&self) -> bool {
        if let Ok(pty) = self.inner.pty.lock() {
            pty.as_ref()
                .map(|p| p.has_foreground_child())
                .unwrap_or(false)
        } else {
            false
        }
    }

    pub fn title(&self) -> Option<String> {
        self.inner.state.lock().ok().and_then(|s| s.title.clone())
    }

    /// Процесс терминала завершился (или не запустился): сигнал станет `true`.
    /// Ставится из потока чтения PTY.
    pub fn set_exit_signal(&self, signal: RwSignal<bool>) {
        if let Ok(mut guard) = self.inner.exit_signal.lock() {
            *guard = Some(signal);
        }
        if self.inner.exited.load(Ordering::Acquire) {
            signal.set(true);
        }
    }

    /// Процесс терминала завершился (или не запустился).
    pub fn has_exited(&self) -> bool {
        self.inner.exited.load(Ordering::Acquire)
    }

    /// pid запущенной программы (оболочки); `None` — PTY ещё не открыт или закрыт.
    pub fn pid(&self) -> Option<u32> {
        self.inner
            .pty
            .lock()
            .ok()
            .and_then(|p| p.as_ref().and_then(|p| p.pid()))
    }

    /// Текущий каталог программы терминала (Linux: `/proc/<pid>/cwd`).
    pub fn cwd(&self) -> Option<std::path::PathBuf> {
        let pid = self.pid()?;
        #[cfg(target_os = "linux")]
        {
            std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = pid;
            None
        }
    }

    /// Предел строк истории (меняется на лету).
    pub fn set_scrollback(&self, lines: usize) {
        if let Ok(mut s) = self.inner.state.lock() {
            s.grid.set_max_scrollback(lines);
            s.scroll_offset = s.scroll_offset.min(s.grid.scrollback_len());
        }
        self.inner.revision.fetch_add(1, Ordering::Release);
    }

    pub fn set_title_signal(&self, signal: RwSignal<String>) {
        if let Ok(mut guard) = self.inner.title_signal.lock() {
            *guard = Some(signal);
        }
        if let Some(title) = self.title() {
            signal.set(title);
        }
    }

    pub(super) fn with_state<R>(&self, f: impl FnOnce(&mut SessionState) -> R) -> R {
        let mut s = self
            .inner
            .state
            .lock()
            .expect("TerminalSession.state poisoned");
        f(&mut *s)
    }

    pub(super) fn with_state_ref<R>(&self, f: impl FnOnce(&SessionState) -> R) -> R {
        let s = self
            .inner
            .state
            .lock()
            .expect("TerminalSession.state poisoned");
        f(&*s)
    }
}

impl std::fmt::Debug for TerminalSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalSession")
            .field("revision", &self.inner.revision.load(Ordering::Relaxed))
            .field("alive", &self.is_alive())
            .finish()
    }
}

/// Открыть PTY: через хранитель, если он задан (не ответил и не запустился —
/// локальный PTY, чтобы терминал всё равно работал), иначе локальный.
/// `None` — открыть не удалось или в хранителе нет терминала с таким ключом
/// (а создавать не просили).
fn open_backend(config: &TerminalConfig, cols: u16, rows: u16) -> Option<(Backend, Receiver<Vec<u8>>)> {
    #[cfg(unix)]
    if let Some(spec) = config.holder.as_ref() {
        let req = super::holder::OpenRequest {
            command: &config.command,
            args: &config.args,
            cwd: config.cwd.as_deref(),
            env: &config.env,
            cols,
            rows,
        };
        match super::holder::HeldPty::open(spec, &req) {
            Ok(Some((pty, rx))) => return Some((Backend::Held(pty), rx)),
            Ok(None) => return None,
            Err(e) => {
                log::warn!("[syngui terminal] хранитель терминалов недоступен ({e}) — PTY в приложении");
                if !spec.create {
                    return None;
                }
            }
        }
    }
    match PtySession::open(&config.command, &config.args, cols, rows, config.cwd.clone(), &config.env) {
        Ok((pty, rx)) => Some((Backend::Local(pty), rx)),
        Err(e) => {
            log::error!("[syngui terminal] open pty failed: {e}");
            None
        }
    }
}

/// Поток разбора вывода PTY держит сессию только через [`Weak`]: сильная
/// ссылка из него делала бы сессию бессмертной — закрытая вкладка не
/// освобождала бы `SessionShared`, и PTY с процессами жили бы дальше без
/// интерфейса. А когда такой процесс всё же завершался, последний `Arc`
/// отпускался в самом этом потоке, `Drop` делал `join()` своего же потока —
/// паника «failed to join thread: Resource deadlock avoided», с
/// `panic = "abort"` это роняло всё приложение.
fn spawn_pump(shared: Weak<SessionShared>, rx: Receiver<Vec<u8>>) -> JoinHandle<()> {
    thread::Builder::new()
        .name("syngui-term-pump".into())
        .spawn(move || pump_loop(shared, rx))
        .expect("spawn term pump thread")
}

fn pump_loop(weak: Weak<SessionShared>, rx: Receiver<Vec<u8>>) {
    loop {
        let msg = rx.recv_timeout(PUMP_RECV_TIMEOUT);
        // Сессию берём только на время обработки: если её уже освободили,
        // выходим. Временный `Arc` может оказаться последним — тогда `Drop`
        // выполнится здесь, и он не ждёт собственный поток (см. `Drop`).
        let Some(shared) = weak.upgrade() else {
            break;
        };
        if shared.stop_flag.load(Ordering::Acquire) {
            break;
        }
        match msg {
            Ok(chunk) => process_chunk(&shared, chunk, &rx),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                // Сам PtySession не сбрасываем здесь под замком: его Drop
                // ждёт reader-поток и шлёт сигнал ребёнку — делаем это вне
                // замка, чтобы GUI-поток не стоял на `pty.lock()`.
                let pty = shared.pty.lock().ok().and_then(|mut p| p.take());
                drop(pty);
                shared.revision.fetch_add(1, Ordering::Release);
                mark_exited(&shared);
                break;
            }
        }
    }
}

fn mark_exited(shared: &SessionShared) {
    shared.exited.store(true, Ordering::Release);
    if let Ok(guard) = shared.exit_signal.lock() {
        if let Some(sig) = guard.as_ref() {
            sig.set(true);
        }
    }
}

fn process_chunk(shared: &SessionShared, first: Vec<u8>, rx: &Receiver<Vec<u8>>) {
    let mut buf = first;
    while buf.len() < PUMP_BATCH_BYTES {
        match rx.try_recv() {
            Ok(more) => buf.extend_from_slice(&more),
            Err(_) => break,
        }
    }

    let mut title_to_push: Option<String> = None;
    if let Ok(mut s) = shared.state.lock() {
        let prev_title_rev = s.title_revision;
        // SAFETY-NOTE: одновременно мутируем grid через performer и parser
        let SessionState {
            grid,
            parser,
            title,
            title_revision,
            ..
        } = &mut *s;
        let mut performer = Performer::new(grid, title);
        for b in &buf {
            parser.advance(&mut performer, *b);
        }
        let _ = prev_title_rev;
        if let Some(t) = title.as_ref() {
            *title_revision = title_revision.wrapping_add(1);
            title_to_push = Some(t.clone());
        }
    }

    if let Some(t) = title_to_push {
        if let Ok(guard) = shared.title_signal.lock() {
            if let Some(sig) = guard.as_ref() {
                sig.set(t);
            }
        }
    }
    shared.revision.fetch_add(1, Ordering::Release);
}

impl Drop for SessionShared {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Release);

        // Закрытие PTY: SIGHUP группе терминала и SIGKILL оболочке
        // (см. `Drop for PtySession`).
        let pty = self.pty.lock().ok().and_then(|mut p| p.take());
        drop(pty);

        let join = self.pump_join.lock().ok().and_then(|mut g| g.take());
        if let Some(join) = join {
            // Последнюю ссылку мог отпустить сам pump-поток — ждать себя
            // нельзя (join своего потока паникует), он и так завершается.
            if join.thread().id() != thread::current().id() {
                let _ = join.join();
            }
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::time::Instant;

    fn session(script: &str) -> TerminalSession {
        let cfg = TerminalConfig {
            command: "/bin/sh".into(),
            args: vec!["-c".into(), script.into()],
            ..TerminalConfig::default()
        };
        let s = TerminalSession::new(cfg).expect("session");
        s.resize(80, 24); // первый resize запускает PTY
        s
    }

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

    fn proc_alive(pid: u32) -> bool {
        // Зомби считаем мёртвым: reap-поток снимает его чуть позже.
        match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
            Ok(stat) => !stat
                .rsplit(')')
                .next()
                .is_some_and(|rest| rest.trim_start().starts_with('Z')),
            Err(_) => false,
        }
    }

    /// Закрытая вкладка (последний handle отпущен) завершает процессы —
    /// раньше pump-поток держал сильную ссылку, и PTY жил без интерфейса.
    #[test]
    fn dropping_last_handle_kills_process_group() {
        let s = session("sleep 300 & echo $! > /dev/null; sleep 300");
        let pid = s.pid().expect("pid");
        assert!(proc_alive(pid));
        let weak = Arc::downgrade(&s.inner);
        drop(s);
        assert!(weak.upgrade().is_none(), "сессия должна освободиться");
        assert!(
            wait_until(Duration::from_secs(3), || !proc_alive(pid)),
            "оболочка {pid} пережила закрытие вкладки"
        );
    }

    /// Завершение дочернего процесса не роняет приложение (раньше — abort
    /// от join собственного потока в `Drop`), сессия помечается вышедшей.
    #[test]
    fn child_exit_marks_exited_without_panic() {
        let s = session("exit 0");
        assert!(wait_until(Duration::from_secs(3), || s.has_exited()));
        assert!(!s.is_alive());
        drop(s);
    }

    /// Убийство процесса снаружи (как `kill` из другой сессии) — то же.
    #[test]
    fn external_kill_is_survived() {
        let s = session("sleep 300");
        let pid = s.pid().expect("pid");
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGKILL);
        }
        assert!(wait_until(Duration::from_secs(3), || s.has_exited()));
        drop(s);
    }

    /// Сценарий падения synthos 19.11: вкладку убрали из интерфейса, затем
    /// её оболочку убили снаружи. Раньше pump-поток отпускал последнюю
    /// ссылку и паниковал на `join` самого себя (abort всего приложения).
    #[test]
    fn orphaned_session_killed_externally_does_not_panic_pump() {
        use std::sync::atomic::AtomicUsize;
        static PUMP_PANICS: AtomicUsize = AtomicUsize::new(0);
        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if thread::current().name() == Some("syngui-term-pump") {
                PUMP_PANICS.fetch_add(1, Ordering::SeqCst);
            }
            prev(info);
        }));
        let s = session("sleep 300");
        let pid = s.pid().expect("pid");
        drop(s);
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGKILL);
        }
        thread::sleep(Duration::from_millis(400));
        assert_eq!(PUMP_PANICS.load(Ordering::SeqCst), 0);
    }

    /// Гонка «последний handle отпущен, пока pump держит временную ссылку»:
    /// много коротких сессий, отпускаемых в момент выхода процесса.
    #[test]
    fn drop_racing_with_exit_does_not_panic() {
        for i in 0..40 {
            let s = session("exit 0");
            thread::sleep(Duration::from_millis((i % 8) as u64 * 3));
            drop(s);
        }
        thread::sleep(Duration::from_millis(200));
    }
}

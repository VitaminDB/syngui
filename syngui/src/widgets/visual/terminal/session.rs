use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
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
    pty: Mutex<Option<PtySession>>,
    revision: AtomicU64,
    title_signal: Mutex<Option<RwSignal<String>>>,
    stop_flag: AtomicBool,
    pump_join: Mutex<Option<JoinHandle<()>>>,
    pending_config: Mutex<Option<TerminalConfig>>,
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
        let state = SessionState {
            grid: Grid::new(1, 1),
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
            stop_flag: AtomicBool::new(false),
            pump_join: Mutex::new(None),
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
        self.inner.autofocus_consumed.store(false, Ordering::Release);
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
        let (pty, rx) = match PtySession::open(
            &config.command,
            &config.args,
            cols,
            rows,
            config.cwd.clone(),
            &config.env,
        ) {
            Ok(pair) => pair,
            Err(e) => {
                log::error!("[syngui terminal] open pty failed: {e}");
                return;
            }
        };
        if let Ok(mut s) = self.inner.state.lock() {
            s.grid.resize(cols as usize, rows as usize);
        }
        if let Ok(mut pty_slot) = self.inner.pty.lock() {
            *pty_slot = Some(pty);
        }
        let pump = spawn_pump(self.inner.clone(), rx);
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
            if let Some(p) = pty.as_mut() {
                if let Err(e) = p.write(bytes) {
                    log::error!("[syngui terminal] pty write error: {e}");
                }
            }
        }
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

    pub fn title(&self) -> Option<String> {
        self.inner.state.lock().ok().and_then(|s| s.title.clone())
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

fn spawn_pump(shared: Arc<SessionShared>, rx: Receiver<Vec<u8>>) -> JoinHandle<()> {
    thread::Builder::new()
        .name("syngui-term-pump".into())
        .spawn(move || pump_loop(shared, rx))
        .expect("spawn term pump thread")
}

fn pump_loop(shared: Arc<SessionShared>, rx: Receiver<Vec<u8>>) {
    while !shared.stop_flag.load(Ordering::Acquire) {
        match rx.recv_timeout(PUMP_RECV_TIMEOUT) {
            Ok(chunk) => {
                process_chunk(&shared, chunk, &rx);
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => {
                if let Ok(mut pty) = shared.pty.lock() {
                    *pty = None;
                }
                shared.revision.fetch_add(1, Ordering::Release);
                break;
            }
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

        if let Ok(mut pty) = self.pty.lock() {
            pty.take();
        }

        if let Ok(mut guard) = self.pump_join.lock() {
            if let Some(join) = guard.take() {
                let _ = join.join();
            }
        }
    }
}

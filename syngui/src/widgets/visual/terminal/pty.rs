use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};

use crossbeam_channel::{bounded, Receiver};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

#[derive(Debug)]
pub enum PtyError {
    Open(String),
    Spawn(String),
    TakeWriter(String),
    CloneReader(String),
    Write(std::io::Error),
}

impl std::fmt::Display for PtyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PtyError::Open(e) => write!(f, "openpty failed: {e}"),
            PtyError::Spawn(e) => write!(f, "spawn command failed: {e}"),
            PtyError::TakeWriter(e) => write!(f, "take_writer failed: {e}"),
            PtyError::CloneReader(e) => write!(f, "clone_reader failed: {e}"),
            PtyError::Write(e) => write!(f, "write failed: {e}"),
        }
    }
}

impl std::error::Error for PtyError {}

impl From<std::io::Error> for PtyError {
    fn from(e: std::io::Error) -> Self {
        PtyError::Write(e)
    }
}

pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    #[allow(dead_code)]
    reader_alive: Arc<AtomicBool>,
    reader_join: Option<JoinHandle<()>>,
}

impl PtySession {
    pub fn open(
        cmd_program: &str,
        cmd_args: &[String],
        cols: u16,
        rows: u16,
        cwd: Option<PathBuf>,
        env: &[(String, String)],
    ) -> Result<(Self, Receiver<Vec<u8>>), PtyError> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| PtyError::Open(e.to_string()))?;

        let mut builder = CommandBuilder::new(cmd_program);
        for arg in cmd_args {
            builder.arg(arg);
        }
        if let Some(c) = cwd {
            builder.cwd(c);
        }
        builder.env("TERM", "xterm-256color");
        builder.env("COLORTERM", "truecolor");

        for var in [
            "TERM_PROGRAM",
            "TERM_PROGRAM_VERSION",
            "ZED_TERM",
            "VSCODE_INJECTION",
            "VSCODE_PID",
            "VSCODE_GIT_IPC_HANDLE",
            "VSCODE_GIT_ASKPASS_NODE",
            "VSCODE_GIT_ASKPASS_MAIN",
            "VSCODE_IPC_HOOK",
            "VSCODE_IPC_HOOK_CLI",
            "VTE_VERSION",
        ] {
            builder.env_remove(var);
        }
        for (k, v) in env {
            builder.env(k, v);
        }

        let child = pair
            .slave
            .spawn_command(builder)
            .map_err(|e| PtyError::Spawn(e.to_string()))?;

        drop(pair.slave);

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| PtyError::TakeWriter(e.to_string()))?;
        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| PtyError::CloneReader(e.to_string()))?;

        let (tx, rx) = bounded::<Vec<u8>>(64);
        let reader_alive = Arc::new(AtomicBool::new(true));
        let alive_for_thread = reader_alive.clone();

        let debug_log: Option<Arc<std::sync::Mutex<std::fs::File>>> =
            std::env::var("SYNGUI_TERMINAL_DEBUG_LOG")
                .ok()
                .and_then(|path| {
                    std::fs::OpenOptions::new()
                        .create(true)
                        .write(true)
                        .truncate(true)
                        .open(&path)
                        .ok()
                        .map(|f| {
                            log::info!("[syngui-pty] debug log: {path}");
                            Arc::new(std::sync::Mutex::new(f))
                        })
                });

        let reader_join = thread::Builder::new()
            .name("syngui-pty-reader".into())
            .spawn(move || {
                let mut buf = [0u8; 4096];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            if let Some(f) = debug_log.as_ref() {
                                use std::io::Write;
                                if let Ok(mut g) = f.lock() {
                                    let _ = g.write_all(&buf[..n]);
                                    let _ = g.flush();
                                }
                            }
                            if tx.send(buf[..n].to_vec()).is_err() {
                                break;
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(e) => {
                            log::debug!("[syngui-pty] reader error: {e}");
                            break;
                        }
                    }
                }
                alive_for_thread.store(false, Ordering::Release);
                drop(tx);
            })
            .expect("spawn pty reader thread");

        Ok((
            Self {
                master: pair.master,
                writer,
                child,
                reader_alive,
                reader_join: Some(reader_join),
            },
            rx,
        ))
    }

    pub fn write(&mut self, bytes: &[u8]) -> Result<(), PtyError> {
        self.writer.write_all(bytes)?;
        self.writer.flush()?;
        Ok(())
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        let _ = self.master.resize(PtySize {
            cols,
            rows,
            pixel_width: 0,
            pixel_height: 0,
        });
    }

    pub fn is_alive(&self) -> bool {
        self.reader_alive.load(Ordering::Acquire)
    }

    /// `true`, если у tty есть foreground-процесс, отличный от самого shell'а —
    /// т.е. в терминале прямо сейчас выполняется команда. Сравниваем foreground
    /// process group tty (`tcgetpgrp` master-fd) с pid дочернего shell'а: shell
    /// в prompt'е сам является foreground-группой (pgid == его pid), а
    /// запущенный job получает собственную process group → id расходятся.
    /// Требует job control в shell'е (интерактивные bash/zsh/fish — всегда);
    /// при любой ошибке или до spawn'а PTY — `false`.
    #[cfg(unix)]
    pub fn has_foreground_child(&self) -> bool {
        match (self.master.process_group_leader(), self.child.process_id()) {
            (Some(fg), Some(shell)) => i64::from(fg) != i64::from(shell),
            _ => false,
        }
    }

    /// На платформах без tcgetpgrp (Windows) занятость не определяется.
    #[cfg(not(unix))]
    pub fn has_foreground_child(&self) -> bool {
        false
    }

    #[allow(dead_code)]
    pub fn try_wait_exit(&mut self) -> Option<u32> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(status.exit_code()),
            _ => None,
        }
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        if let Some(join) = self.reader_join.take() {
            let _ = join.join();
        }
    }
}

pub fn default_shell() -> String {
    #[cfg(unix)]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into())
    }
    #[cfg(windows)]
    {
        std::env::var("ComSpec").unwrap_or_else(|_| "powershell.exe".into())
    }
    #[cfg(not(any(unix, windows)))]
    {
        "/bin/sh".into()
    }
}

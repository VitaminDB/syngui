#[cfg(all(
    feature = "single-instance",
    not(target_arch = "wasm32"),
    not(target_os = "android")
))]
pub(crate) use desktop::SingleInstanceLock;
#[cfg(all(
    feature = "single-instance",
    not(target_arch = "wasm32"),
    not(target_os = "android")
))]
pub use desktop::{forward_to_running_instance, notify_running_instance};

use std::cell::RefCell;

thread_local! {
    /// Обработчик аргументов, пересланных вторым запуском (живёт на главном потоке).
    static INSTANCE_ARGS_HANDLER: RefCell<Option<std::rc::Rc<dyn Fn(Vec<String>)>>> =
        const { RefCell::new(None) };
}

/// Задать обработчик аргументов командной строки, которые прислал повторный
/// запуск приложения (`App::with_single_instance`): `app --open X` при уже
/// открытом окне не открывает второе, а передаёт `["--open", "X"]` сюда.
/// Вызывается на главном потоке, после чего окно поднимается. Аргументы — без
/// имени программы; пустой список — запуск без аргументов. Ставить из
/// замыкания `run` (главный поток).
pub fn on_instance_args(handler: impl Fn(Vec<String>) + 'static) {
    INSTANCE_ARGS_HANDLER.with(|h| *h.borrow_mut() = Some(std::rc::Rc::new(handler)));
}

#[allow(dead_code)]
pub(crate) fn dispatch_instance_args(args: Vec<String>) {
    let handler = INSTANCE_ARGS_HANDLER.with(|h| h.borrow().clone());
    match handler {
        Some(f) => f(args),
        None if !args.is_empty() => {
            log::info!("[syngui] аргументы повторного запуска без обработчика: {args:?}")
        }
        None => {}
    }
}

#[cfg(all(
    feature = "single-instance",
    not(target_arch = "wasm32"),
    not(target_os = "android")
))]
mod desktop {
    use super::super::user_event::SynGuiUserEvent;
    use interprocess::local_socket::{
        traits::{Listener as _, ListenerExt as _, Stream as _},
        GenericNamespaced, ListenerNonblockingMode, ListenerOptions, Stream, ToNsName,
    };
    use single_instance::SingleInstance;
    use std::{
        io::{ErrorKind, Read, Write},
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        thread::{self, JoinHandle},
        time::Duration,
    };
    use winit::event_loop::EventLoopProxy;

    const ACTIVATE_MSG: &[u8] = b"activate\n";

    pub(crate) struct SingleInstanceLock {
        _lock: SingleInstance,
        shutdown: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
    }

    impl SingleInstanceLock {
        pub fn try_acquire(
            app_id: &str,
            proxy: EventLoopProxy<SynGuiUserEvent>,
        ) -> Result<Option<Self>, Box<dyn std::error::Error>> {
            let lock = SingleInstance::new(&session_id(app_id))
                .map_err(|e| format!("single-instance lock failed: {e}"))?;
            if !lock.is_single() {
                return Ok(None);
            }

            let socket_name = socket_name(app_id);
            let name = socket_name.as_str().to_ns_name::<GenericNamespaced>()?;
            let listener = ListenerOptions::new()
                .name(name)
                .create_sync()
                .map_err(|e| format!("listener bind ({}): {}", socket_name, e))?;
            listener.set_nonblocking(ListenerNonblockingMode::Both)?;

            let shutdown = Arc::new(AtomicBool::new(false));
            let shutdown_clone = shutdown.clone();

            let thread = thread::Builder::new()
                .name("syngui-single-instance".into())
                .spawn(move || run_listener(listener, proxy, shutdown_clone))?;

            Ok(Some(Self {
                _lock: lock,
                shutdown,
                thread: Some(thread),
            }))
        }
    }

    impl Drop for SingleInstanceLock {
        fn drop(&mut self) {
            self.shutdown.store(true, Ordering::Relaxed);
            if let Some(handle) = self.thread.take() {
                let _ = handle.join();
            }
        }
    }

    fn run_listener(
        listener: interprocess::local_socket::Listener,
        proxy: EventLoopProxy<SynGuiUserEvent>,
        shutdown: Arc<AtomicBool>,
    ) {
        for incoming in listener.incoming() {
            if shutdown.load(Ordering::Relaxed) {
                break;
            }
            match incoming {
                Ok(stream) => {
                    // Читаем в своём потоке: клиент может писать с задержкой,
                    // а слушатель не должен на нём стоять.
                    let proxy = proxy.clone();
                    let _ = thread::Builder::new()
                        .name("syngui-single-instance-conn".into())
                        .spawn(move || handle_connection(stream, proxy));
                }
                Err(err) if err.kind() == ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(150));
                }
                Err(err) => {
                    log::warn!("single-instance listener error: {err}");
                    thread::sleep(Duration::from_millis(500));
                }
            }
        }
    }

    /// Сообщение: `activate\n`, затем аргументы через `\0` до конца потока.
    fn handle_connection(stream: Stream, proxy: EventLoopProxy<SynGuiUserEvent>) {
        let mut stream = stream;
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_recv_timeout(Some(Duration::from_secs(2)));
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            match stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    buf.extend_from_slice(&chunk[..n]);
                    if buf.len() > 1 << 20 {
                        break;
                    }
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        let Some(rest) = buf.strip_prefix(b"activate") else {
            return;
        };
        let args: Vec<String> = match rest.strip_prefix(b"\n") {
            Some(a) if !a.is_empty() => a
                .split(|b| *b == 0)
                .map(|s| String::from_utf8_lossy(s).into_owned())
                .collect(),
            _ => Vec::new(),
        };
        crate::async_runtime::run_on_main_thread(move || {
            super::dispatch_instance_args(args);
        });
        let _ = proxy.send_event(SynGuiUserEvent::Activate);
    }

    pub fn notify_running_instance(app_id: &str) -> std::io::Result<()> {
        forward_to_running_instance(app_id, &[])
    }

    /// Передать аргументы уже запущенному экземпляру (`App::with_single_instance`
    /// с тем же `app_id`) и поднять его окно. `Err` — экземпляр не запущен.
    /// Удобно звать в `main` до тяжёлой инициализации: удалось — выйти.
    pub fn forward_to_running_instance(app_id: &str, args: &[String]) -> std::io::Result<()> {
        let socket_name = socket_name(app_id);
        let name = socket_name
            .as_str()
            .to_ns_name::<GenericNamespaced>()
            .map_err(|e| std::io::Error::new(ErrorKind::InvalidInput, e))?;
        let mut stream = Stream::connect(name)?;
        let mut msg = ACTIVATE_MSG.to_vec();
        msg.extend_from_slice(args.join("\0").as_bytes());
        stream.write_all(&msg)?;
        stream.flush()?;
        Ok(())
    }

    fn socket_name(app_id: &str) -> String {
        format!("{}.sock", session_id(app_id))
    }

    /// Имя экземпляра в пределах сеанса. Замок и сокет — абстрактные (общие на
    /// всю машину), поэтому программа в другом сеансе (песочница synlink,
    /// вложенный synwm со своим `XDG_RUNTIME_DIR`) иначе находила бы окно живого
    /// сеанса и отдавала запуск ему. Стандартный каталог `/run/user/UID` — имя
    /// без изменений (как было), любой другой — с меткой каталога.
    pub(crate) fn session_id(app_id: &str) -> String {
        session_id_for(app_id, std::env::var_os("XDG_RUNTIME_DIR").as_deref().map(std::path::Path::new))
    }

    fn session_id_for(app_id: &str, runtime: Option<&std::path::Path>) -> String {
        let default = std::path::PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() }));
        match runtime {
            Some(r) if !r.as_os_str().is_empty() && r != default => {
                // FNV-1a: устойчиво между запусками и версиями (не DefaultHasher)
                let mut h: u64 = 0xcbf29ce484222325;
                for b in r.as_os_str().as_encoded_bytes() {
                    h ^= *b as u64;
                    h = h.wrapping_mul(0x100000001b3);
                }
                format!("{app_id}-{:08x}", h as u32)
            }
            _ => app_id.to_string(),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::session_id_for;
        use std::path::Path;

        #[test]
        fn live_session_keeps_name_other_runtime_gets_own() {
            let live = format!("/run/user/{}", unsafe { libc::getuid() });
            assert_eq!(session_id_for("synthos", Some(Path::new(&live))), "synthos");
            assert_eq!(session_id_for("synthos", None), "synthos");
            let a = session_id_for("synthos", Some(Path::new("/run/user/1000/synlink-sandbox/a/run")));
            let b = session_id_for("synthos", Some(Path::new("/run/user/1000/synlink-sandbox/b/run")));
            assert!(a.starts_with("synthos-") && a != b);
            assert_eq!(a, session_id_for("synthos", Some(Path::new("/run/user/1000/synlink-sandbox/a/run"))));
        }
    }
}

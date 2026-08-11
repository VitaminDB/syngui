#[cfg(all(
    feature = "single-instance",
    not(target_arch = "wasm32"),
    not(target_os = "android")
))]
pub(crate) use desktop::{notify_running_instance, SingleInstanceLock};

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
            let lock = SingleInstance::new(app_id)
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
                Ok(mut stream) => {
                    let mut buf = [0u8; 32];
                    let n = stream.read(&mut buf).unwrap_or(0);
                    if buf.get(..n).map_or(false, |b| b.starts_with(b"activate")) {
                        let _ = proxy.send_event(SynGuiUserEvent::Activate);
                    }
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

    pub fn notify_running_instance(app_id: &str) -> std::io::Result<()> {
        let socket_name = socket_name(app_id);
        let name = socket_name
            .as_str()
            .to_ns_name::<GenericNamespaced>()
            .map_err(|e| std::io::Error::new(ErrorKind::InvalidInput, e))?;
        let mut stream = Stream::connect(name)?;
        stream.write_all(ACTIVATE_MSG)?;
        stream.flush()?;
        Ok(())
    }

    fn socket_name(app_id: &str) -> String {
        format!("{app_id}.sock")
    }
}

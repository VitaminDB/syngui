use std::sync::{mpsc, Arc, Mutex, OnceLock};

type MainThreadCallback = Box<dyn FnOnce() + Send + 'static>;

struct MainChannel {
    tx: mpsc::Sender<MainThreadCallback>,
    rx: crate::core::sync::Mutex<mpsc::Receiver<MainThreadCallback>>,
}

static MAIN_CHANNEL: OnceLock<MainChannel> = OnceLock::new();
static WINDOW: Mutex<Option<Arc<dyn crate::signal::RedrawNotifier>>> = Mutex::new(None);
/// Пробудитель event loop'а (EventLoopProxy::send_event). Нужен отдельно от
/// request_redraw: без живого surface (фон на Android, свёрнутое окно) redraw
/// не приходит, и очередь колбэков зависала бы до следующего кадра.
static WAKER: Mutex<Option<Box<dyn Fn() + Send>>> = Mutex::new(None);

fn channel() -> &'static MainChannel {
    MAIN_CHANNEL.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        MainChannel {
            tx,
            rx: crate::core::sync::Mutex::new(rx),
        }
    })
}

pub fn run_on_main_thread(f: impl FnOnce() + Send + 'static) {
    let ch = channel();
    let _ = ch.tx.send(Box::new(f));
    // Сначала будим сам loop (работает и без окна/кадров), затем — redraw,
    // чтобы изменения сигналов сразу попали в кадр, когда окно живо.
    if let Ok(guard) = WAKER.lock() {
        if let Some(wake) = guard.as_ref() {
            wake();
        }
    }
    if let Some(window) = WINDOW.lock().ok().and_then(|g| g.clone()) {
        window.request_redraw();
    }
}

/// Установить пробудитель event loop'а для `run_on_main_thread` (обычно
/// замыкание над `EventLoopProxy::send_event`). Ставится раннером при старте.
pub fn set_main_thread_waker(wake: impl Fn() + Send + 'static) {
    if let Ok(mut guard) = WAKER.lock() {
        *guard = Some(Box::new(wake));
    }
}

pub fn main_thread_sender() -> mpsc::Sender<MainThreadCallback> {
    channel().tx.clone()
}

#[cfg(feature = "tokio")]
static TOKIO_HANDLE: OnceLock<tokio::runtime::Handle> = OnceLock::new();

#[cfg(feature = "tokio")]
fn tokio_handle() -> &'static tokio::runtime::Handle {
    TOKIO_HANDLE.get_or_init(|| {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create tokio runtime");
        let handle = rt.handle().clone();
        std::thread::Builder::new()
            .name("syngui-tokio".into())
            .spawn(move || {
                rt.block_on(std::future::pending::<()>());
            })
            .expect("Failed to spawn tokio runtime thread");
        handle
    })
}

#[cfg(feature = "tokio")]
pub fn spawn<F>(future: F) -> tokio::task::JoinHandle<F::Output>
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio_handle().spawn(future)
}

#[cfg(feature = "tokio")]
pub fn set_tokio_handle(handle: tokio::runtime::Handle) -> Result<(), tokio::runtime::Handle> {
    TOKIO_HANDLE.set(handle)
}

#[cfg(feature = "winit")]
pub(crate) fn set_async_window(window: Arc<crate::window::Window>) {
    if let Ok(mut guard) = WINDOW.lock() {
        *guard = Some(window as Arc<dyn crate::signal::RedrawNotifier>);
    }
}

pub fn set_async_notifier(notifier: Arc<dyn crate::signal::RedrawNotifier>) {
    if let Ok(mut guard) = WINDOW.lock() {
        *guard = Some(notifier);
    }
}

pub(crate) fn clear_async_window() {
    if let Ok(mut guard) = WINDOW.lock() {
        *guard = None;
    }
}

pub(crate) fn poll_main_thread_callbacks() {
    let ch = channel();
    if let Ok(rx) = ch.rx.try_lock() {
        while let Ok(callback) = rx.try_recv() {
            callback();
        }
    }
}

/// Выполнить накопленные main-thread колбэки ВНЕ event loop'а — для
/// headless-раннеров (smoke-тесты, CI): без окна `run_on_main_thread`
/// складывает колбэки в канал, и кросс-поточные `RwSignal::set` применяются
/// только после дренажа. Вызывать из main-потока.
pub fn drain_main_thread_callbacks() {
    poll_main_thread_callbacks();
}

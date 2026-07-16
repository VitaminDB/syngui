#[cfg(feature = "tokio")]
use crate::async_runtime::{run_on_main_thread, spawn};
#[cfg(feature = "tokio")]
use crate::signal::{create_effect_with_cleanup, use_signal, RwSignal};
#[cfg(feature = "tokio")]
use std::future::Future;
#[cfg(feature = "tokio")]
use std::sync::Arc;
use crate::core::sync::Mutex;

#[cfg(feature = "tokio")]
pub fn use_async<T, F, Fut>(factory: F) -> (RwSignal<Option<T>>, RwSignal<bool>)
where
    T: Clone + Send + PartialEq + 'static,
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = T> + Send + 'static,
{
    let data = use_signal::<Option<T>>(None);
    let loading = use_signal(false);

    let abort_handle: Arc<Mutex<Option<tokio::task::AbortHandle>>> =
        Arc::new(Mutex::new(None));

    let abort_for_effect = abort_handle.clone();

    create_effect_with_cleanup(move || {
        if let Ok(mut handle) = abort_for_effect.lock() {
            if let Some(h) = handle.take() {
                h.abort();
            }
        }

        let future = factory();

        loading.set_always(true);

        let join_handle = spawn(async move {
            let result = future.await;
            run_on_main_thread(move || {
                data.set(Some(result));
                loading.set(false);
            });
        });

        let new_abort = join_handle.abort_handle();
        if let Ok(mut handle) = abort_for_effect.lock() {
            *handle = Some(new_abort);
        }

        let abort_for_cleanup = abort_for_effect.clone();
        Some(Box::new(move || {
            if let Ok(mut handle) = abort_for_cleanup.lock() {
                if let Some(h) = handle.take() {
                    h.abort();
                }
            }
        }) as Box<dyn Fn()>)
    });

    (data, loading)
}

#[cfg(not(target_arch = "wasm32"))]
mod inner {
    pub use std::sync::Mutex;
}

#[cfg(target_arch = "wasm32")]
mod inner {
    use std::cell::UnsafeCell;
    use std::fmt;
    use std::ops::{Deref, DerefMut};

    pub struct Mutex<T: ?Sized> {
        inner: UnsafeCell<T>,
    }

    // Safety: wasm32 is single-threaded, no data races possible
    unsafe impl<T: ?Sized> Send for Mutex<T> {}
    unsafe impl<T: ?Sized> Sync for Mutex<T> {}

    impl<T> Mutex<T> {
        pub fn new(val: T) -> Self {
            Self { inner: UnsafeCell::new(val) }
        }
    }

    impl<T: ?Sized> Mutex<T> {
        pub fn lock(&self) -> Result<MutexGuard<'_, T>, std::sync::PoisonError<MutexGuard<'_, T>>> {
            // Safety: wasm32 is single-threaded, no concurrent access
            Ok(MutexGuard { data: unsafe { &mut *self.inner.get() } })
        }

        pub fn try_lock(&self) -> Result<MutexGuard<'_, T>, std::sync::TryLockError<MutexGuard<'_, T>>> {
            // Safety: wasm32 is single-threaded, always succeeds
            Ok(MutexGuard { data: unsafe { &mut *self.inner.get() } })
        }
    }

    impl<T: ?Sized + fmt::Debug> fmt::Debug for Mutex<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("Mutex").finish()
        }
    }

    pub struct MutexGuard<'a, T: ?Sized> {
        data: &'a mut T,
    }

    impl<T: ?Sized> Deref for MutexGuard<'_, T> {
        type Target = T;
        fn deref(&self) -> &T {
            self.data
        }
    }

    impl<T: ?Sized> DerefMut for MutexGuard<'_, T> {
        fn deref_mut(&mut self) -> &mut T {
            self.data
        }
    }

    impl<T: ?Sized + fmt::Display> fmt::Display for MutexGuard<'_, T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            (**self).fmt(f)
        }
    }
}

pub use inner::Mutex;

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static CONTEXT_STORE: RefCell<HashMap<TypeId, Box<dyn Any>>> =
        RefCell::new(HashMap::new());
}

pub fn provide_context<T: Clone + 'static>(value: T) {
    CONTEXT_STORE.with(|store| {
        store.borrow_mut().insert(TypeId::of::<T>(), Box::new(value));
    });
}

pub fn use_context<T: Clone + 'static>() -> T {
    CONTEXT_STORE.with(|store| {
        store
            .borrow()
            .get(&TypeId::of::<T>())
            .and_then(|v| v.downcast_ref::<T>())
            .cloned()
            .unwrap_or_else(|| {
                panic!(
                    "Context not provided for type: {}. Call provide_context::<{}>() first.",
                    std::any::type_name::<T>(),
                    std::any::type_name::<T>(),
                )
            })
    })
}

pub fn try_use_context<T: Clone + 'static>() -> Option<T> {
    CONTEXT_STORE.with(|store| {
        store
            .borrow()
            .get(&TypeId::of::<T>())
            .and_then(|v| v.downcast_ref::<T>())
            .cloned()
    })
}

pub fn remove_context<T: 'static>() {
    CONTEXT_STORE.with(|store| {
        store.borrow_mut().remove(&TypeId::of::<T>());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct TestCtx {
        value: i32,
    }

    #[derive(Clone, Debug, PartialEq)]
    struct AnotherCtx(String);

    #[test]
    fn provide_and_use() {
        provide_context(TestCtx { value: 42 });
        let ctx = use_context::<TestCtx>();
        assert_eq!(ctx.value, 42);
        remove_context::<TestCtx>();
    }

    #[test]
    fn try_use_returns_none_when_missing() {
        remove_context::<AnotherCtx>();
        assert!(try_use_context::<AnotherCtx>().is_none());
    }

    #[test]
    fn overwrite_replaces_value() {
        provide_context(TestCtx { value: 1 });
        provide_context(TestCtx { value: 2 });
        assert_eq!(use_context::<TestCtx>().value, 2);
        remove_context::<TestCtx>();
    }

    #[test]
    fn multiple_types_coexist() {
        provide_context(TestCtx { value: 10 });
        provide_context(AnotherCtx("hello".into()));
        assert_eq!(use_context::<TestCtx>().value, 10);
        assert_eq!(use_context::<AnotherCtx>().0, "hello");
        remove_context::<TestCtx>();
        remove_context::<AnotherCtx>();
    }

    #[test]
    #[should_panic(expected = "Context not provided")]
    fn use_context_panics_when_missing() {
        remove_context::<AnotherCtx>();
        let _ = use_context::<AnotherCtx>();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    Escape,
    Enter,
    Tab,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Left,
    Right,
    Up,
    Down,
    Shift,
    Ctrl,
    Alt,
    Meta,
    Space,
    /// Медиа-клавиши (пульт Android TV, мультимедийная клавиатура).
    MediaPlayPause,
    MediaStop,
    MediaNext,
    MediaPrevious,
    MediaRewind,
    MediaFastForward,
    /// Клавиша «меню»/ContextMenu.
    ContextMenu,
    Unknown(u32),
}

thread_local! {
    static KEY_SCANCODE: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
}

/// Физический код клавиши текущего `KeyDown`/`KeyUp` — на Linux это код evdev
/// (`KEY_ESC` = 1, `KEY_A` = 30, …), различает левые и правые модификаторы и
/// клавиши, которых нет в [`Key`] (знаки, цифровой блок). Действителен, пока
/// событие разносится по дереву (обработчики вызываются синхронно); вне его —
/// `None`. Нужен эмуляторам и удалённым экранам: гостю уходит клавиша, а не символ.
pub fn current_scancode() -> Option<u32> {
    KEY_SCANCODE.with(|c| c.get())
}

/// Поставить код для [`current_scancode`] (окно — на время разноски события).
pub fn set_current_scancode(code: Option<u32>) {
    KEY_SCANCODE.with(|c| c.set(code));
}

static KEYBOARD_GRAB: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Захват клавиатуры приложением (экран виртуальной машины, удалённый стол):
/// окно перестаёт само разбирать F11 (полный экран), F12 и Ctrl+Alt+C
/// (инспектор), Tab (переход фокуса) — все клавиши уходят дереву как есть.
pub fn set_keyboard_grab(on: bool) {
    KEYBOARD_GRAB.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn keyboard_grab() -> bool {
    KEYBOARD_GRAB.load(std::sync::atomic::Ordering::Relaxed)
}

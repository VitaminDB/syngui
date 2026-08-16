//! Биндинг `ext-background-effect-v1` — стандартный протокол «эффектов фона»
//! (сейчас это размытие), которым KWin 6.7+ заменил `org_kde_kwin_blur`.
//!
//! Крейт `wayland-protocols` этот staging-протокол ещё не включает, поэтому код
//! генерируется прямо из XML в `syngui/protocols/`.

#![allow(clippy::all)]

use wayland_client;
use wayland_client::protocol::*;

pub mod __interfaces {
    use wayland_client::protocol::__interfaces::*;
    wayland_scanner::generate_interfaces!("protocols/ext-background-effect-v1.xml");
}
use self::__interfaces::*;

wayland_scanner::generate_client_code!("protocols/ext-background-effect-v1.xml");

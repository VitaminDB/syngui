//! Цели сборки и профили.

use anyhow::{bail, Result};
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    LinuxX86_64,
    LinuxAarch64,
    Android,
    Ios,
}

impl Target {
    pub const ALL: [Target; 4] = [Target::LinuxX86_64, Target::LinuxAarch64, Target::Android, Target::Ios];

    pub fn parse(s: &str) -> Result<Target> {
        Ok(match s {
            "linux-x86_64" | "x86_64" | "linux-amd64" => Target::LinuxX86_64,
            "linux-aarch64" | "aarch64" | "linux-arm64" | "phone" => Target::LinuxAarch64,
            "linux" => Target::host_linux(),
            "android" | "apk" => Target::Android,
            "ios" | "iphone" | "ipa" => Target::Ios,
            _ => bail!("неизвестная цель {s}: linux-x86_64, linux-aarch64, android, ios"),
        })
    }

    pub fn host_linux() -> Target {
        if cfg!(target_arch = "aarch64") {
            Target::LinuxAarch64
        } else {
            Target::LinuxX86_64
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Target::LinuxX86_64 => "linux-x86_64",
            Target::LinuxAarch64 => "linux-aarch64",
            Target::Android => "android",
            Target::Ios => "ios",
        }
    }

    /// Архитектура Linux (pacman/uname).
    pub fn arch(self) -> &'static str {
        match self {
            Target::LinuxX86_64 => "x86_64",
            _ => "aarch64",
        }
    }

    /// Тройка rustc для Linux-целей.
    pub fn triple(self) -> &'static str {
        match self {
            Target::LinuxX86_64 => "x86_64-unknown-linux-gnu",
            Target::LinuxAarch64 => "aarch64-unknown-linux-gnu",
            Target::Android => "aarch64-linux-android",
            Target::Ios => "aarch64-apple-ios",
        }
    }

    /// Сборка на этой машине без кросс-окружения.
    pub fn is_host(self) -> bool {
        self == Target::host_linux()
    }
}

impl fmt::Display for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Профиль cargo: `fast-release` локально, `release` (LTO) — для публикации, `dev` — отладка.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Dev,
    Fast,
    Release,
}

impl Profile {
    pub fn cargo_args(self) -> Vec<String> {
        match self {
            Profile::Dev => vec![],
            Profile::Fast => vec!["--profile".into(), "fast-release".into()],
            Profile::Release => vec!["--release".into()],
        }
    }

    /// Каталог в target/.
    pub fn dir(self) -> &'static str {
        match self {
            Profile::Dev => "debug",
            Profile::Fast => "fast-release",
            Profile::Release => "release",
        }
    }
}

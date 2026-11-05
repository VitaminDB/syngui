//! `syngui` — сборка, упаковка и запуск приложений на syngui (как `flutter build` / `cargo tauri`).
//!
//! ```text
//! syngui doctor [-t android]                       что есть для сборки
//! syngui info [-p крейт]                           что утилита знает о приложении
//! syngui build -t linux-x86_64|linux-aarch64|android|ios [--release] [-p крейт]
//! syngui package -f tar|arch|aur|apk|ipa [-t …] [-p крейт]
//! syngui run [-d local|android|ios|phone|ssh:host] [-p крейт]
//! syngui keygen [-p крейт]                         релизный ключ Android в ~/.secrets/<крейт>-android
//! syngui init [-p крейт]                           заготовка [package.metadata.syngui]
//! ```

mod android;
mod doctor;
mod icon;
mod ios;
mod linux;
mod package;
mod project;
mod target;
mod util;

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use project::App;
use std::path::PathBuf;
use std::process::Command;
use target::{Profile, Target};

#[derive(Parser)]
#[command(name = "syngui", version, about = "Сборка, упаковка и запуск приложений на syngui: Linux x86_64/aarch64, Android, iOS")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Args, Clone)]
struct Sel {
    /// Крейт приложения (по умолчанию — крейт текущего каталога)
    #[arg(short = 'p', long = "package")]
    package: Option<String>,
    /// Cargo.toml рабочего пространства или крейта
    #[arg(long)]
    manifest_path: Option<PathBuf>,
}

#[derive(Args, Clone)]
struct Prof {
    /// Публикация: cargo --release (LTO); APK — только релизным ключом
    #[arg(long, conflicts_with = "debug")]
    release: bool,
    /// Отладочная сборка (профиль dev)
    #[arg(long)]
    debug: bool,
}

impl Prof {
    fn profile(&self) -> Profile {
        if self.release {
            Profile::Release
        } else if self.debug {
            Profile::Dev
        } else {
            Profile::Fast
        }
    }
}

#[derive(Subcommand)]
enum Cmd {
    /// Проверить окружение: Rust-цели, NDK/SDK, JDK, iOS SDK, sysroot aarch64, инструменты
    Doctor {
        /// Цели (по умолчанию все)
        #[arg(short, long)]
        target: Vec<String>,
    },
    /// Показать, что утилита знает о приложении (id, имя, значок, программы, ключ)
    Info {
        #[command(flatten)]
        sel: Sel,
    },
    /// Собрать приложение под цель
    Build {
        /// linux-x86_64, linux-aarch64, android, ios (можно несколько; по умолчанию Linux этой машины)
        #[arg(short, long)]
        target: Vec<String>,
        #[command(flatten)]
        sel: Sel,
        #[command(flatten)]
        prof: Prof,
        /// Доп. аргументы cargo (Linux): после --
        #[arg(last = true)]
        cargo_args: Vec<String>,
    },
    /// Упаковать: tar (архив релиза, основа -bin), arch (пакет pacman), aur (PKGBUILD -git и -bin), apk, ipa
    Package {
        #[arg(short, long)]
        format: String,
        /// Цели для tar/arch (по умолчанию Linux этой машины; all — x86_64 и aarch64)
        #[arg(short, long)]
        target: Vec<String>,
        #[command(flatten)]
        sel: Sel,
        /// Быстрая сборка fast-release вместо --release (проверить упаковку локально)
        #[arg(long)]
        fast: bool,
        /// Каталог для aur (по умолчанию target/syngui/aur)
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Собрать и запустить: здесь, на Android (adb), iPhone (plumesign), телефоне с synshell (synport) или по ssh
    Run {
        /// local, android, ios, phone, ssh:<user@host>
        #[arg(short, long, default_value = "local")]
        device: String,
        #[command(flatten)]
        sel: Sel,
        #[command(flatten)]
        prof: Prof,
        /// Не собирать, взять последнюю сборку
        #[arg(long)]
        no_build: bool,
        /// Не показывать журнал (android)
        #[arg(long)]
        no_log: bool,
        /// Аргументы программы (local): после --
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Создать релизный ключ подписи Android в ~/.secrets/<крейт>-android (700/600, RSA 4096, 100 лет)
    Keygen {
        #[command(flatten)]
        sel: Sel,
        /// Владелец сертификата
        #[arg(long, default_value = "CN=Alexeyev Vitaly, O=syn, C=KZ")]
        dname: String,
    },
    /// Добавить заготовку [package.metadata.syngui] в Cargo.toml крейта
    Init {
        #[command(flatten)]
        sel: Sel,
    },
}

fn targets(v: &[String]) -> Result<Vec<Target>> {
    if v.is_empty() {
        return Ok(vec![Target::host_linux()]);
    }
    let mut out = Vec::new();
    for s in v.iter().flat_map(|s| s.split(',')) {
        let ts = match s {
            "all" => Target::ALL.to_vec(),
            "all-linux" | "linux-all" => vec![Target::LinuxX86_64, Target::LinuxAarch64],
            _ => vec![Target::parse(s)?],
        };
        for t in ts {
            if !out.contains(&t) {
                out.push(t);
            }
        }
    }
    Ok(out)
}

fn load(sel: &Sel) -> Result<App> {
    App::load(sel.manifest_path.as_deref(), sel.package.as_deref())
}

fn main() {
    // zsh кладёт в `_` путь к программе в своей внутренней кодировке: кириллица в пути
    // (~/Storage/Проекты) даёт невалидный UTF-8, и build.rs с env::vars() (libm) падают.
    // Такие переменные детям не передаём.
    for (k, v) in std::env::vars_os() {
        if v.to_str().is_none() || k.to_str().is_none() {
            std::env::remove_var(&k);
        }
    }
    let cli = Cli::parse();
    if let Err(e) = real_main(cli) {
        eprintln!("\x1b[1;31mошибка:\x1b[0m {e:#}");
        std::process::exit(1);
    }
}

fn real_main(cli: Cli) -> Result<()> {
    match cli.cmd {
        Cmd::Doctor { target } => {
            let t = if target.is_empty() { Target::ALL.to_vec() } else { targets(&target)? };
            if !doctor::run(&t) {
                std::process::exit(1);
            }
        }
        Cmd::Info { sel } => info(&load(&sel)?)?,
        Cmd::Build { target, sel, prof, cargo_args } => {
            let app = load(&sel)?;
            let mut done = Vec::new();
            for t in targets(&target)? {
                let p = build(&app, t, prof.profile(), &cargo_args)?;
                done.push((t, p));
            }
            for (t, p) in done {
                println!("{t}: {}", p.display());
            }
        }
        Cmd::Package { format, target, sel, fast, out } => {
            let app = load(&sel)?;
            let profile = if fast { Profile::Fast } else { Profile::Release };
            let publish = !fast;
            let mut results = Vec::new();
            match format.as_str() {
                "tar" | "bin" => {
                    for t in targets(&target)? {
                        ensure_linux(t)?;
                        results.push(package::tar(&app, t, profile)?);
                    }
                }
                "arch" | "pacman" => {
                    for t in targets(&target)? {
                        ensure_linux(t)?;
                        results.push(package::arch(&app, t, profile)?);
                    }
                }
                "aur" => results.push(package::aur(&app, out.as_deref())?),
                "apk" | "android" => {
                    let apk = android::build(&app, profile, publish, None)?;
                    results.push(dist(&app, &apk, publish)?);
                }
                "ipa" | "ios" => {
                    let ipa = ios::build(&app, profile)?;
                    results.push(dist(&app, &ipa, true)?);
                }
                f => bail!("неизвестный формат {f}: tar, arch, aur, apk, ipa"),
            }
            for r in results {
                println!("{}", r.display());
            }
        }
        Cmd::Run { device, sel, prof, no_build, no_log, args } => {
            let app = load(&sel)?;
            run(&app, &device, prof.profile(), no_build, !no_log, &args)?;
        }
        Cmd::Keygen { sel, dname } => android::keygen(&load(&sel)?, &dname)?,
        Cmd::Init { sel } => init(&load(&sel)?)?,
    }
    Ok(())
}

fn ensure_linux(t: Target) -> Result<()> {
    if matches!(t, Target::Android | Target::Ios) {
        bail!("формат tar/arch — для Linux-целей; для {t}: --format {}", if t == Target::Android { "apk" } else { "ipa" });
    }
    Ok(())
}

/// Копия артефакта в target/syngui/dist + .sha256.
fn dist(app: &App, file: &std::path::Path, sum: bool) -> Result<PathBuf> {
    let d = app.dist_dir();
    let out = d.join(file.file_name().unwrap());
    util::copy(file, &out)?;
    if sum {
        util::sha256_file(&out)?;
    }
    Ok(out)
}

fn build(app: &App, t: Target, profile: Profile, extra: &[String]) -> Result<PathBuf> {
    match t {
        Target::LinuxX86_64 | Target::LinuxAarch64 => linux::build(app, t, profile, extra),
        Target::Android => android::build(app, profile, false, None),
        Target::Ios => ios::build(app, profile),
    }
}

fn run(app: &App, device: &str, profile: Profile, no_build: bool, log: bool, args: &[String]) -> Result<()> {
    match device {
        "local" | "host" | "here" => {
            let bin = app.bin()?;
            let mut c = linux::cargo(app, "run", profile);
            c.arg("--bin").arg(&bin);
            if let Some(f) = &app.meta.linux.features {
                c.arg("--features").arg(f.join(","));
            }
            c.arg("--").args(args);
            util::run(c)
        }
        "android" => {
            let apk = if no_build {
                app.work_dir("android").join(format!("{}-{}.apk", app.krate, app.version))
            } else {
                // под ABI подключённого устройства (эмулятор — x86_64)
                let abi = android::device_abi(&android::device()?)?;
                android::build(app, profile, false, Some(vec![abi]))?
            };
            if !apk.is_file() {
                bail!("нет {} — соберите без --no-build", apk.display());
            }
            android::install_run(app, &apk, log)
        }
        "ios" | "iphone" => {
            let ipa = if no_build { app.work_dir("ios").join(format!("{}-{}.ipa", app.krate, app.version)) } else { ios::build(app, profile)? };
            ios::install(&ipa)
        }
        d if d == "phone" || d.starts_with("ssh:") => {
            let t = Target::LinuxAarch64;
            let bin_path = if no_build { app.target_dir.join(if t.is_host() { String::new() } else { t.triple().into() }).join(profile.dir()).join(app.bin()?) } else { linux::build(app, t, profile, &[])? };
            install_ssh(app, d, &bin_path)
        }
        d => bail!("неизвестное устройство {d}: local, android, ios, phone, ssh:<user@host>"),
    }
}

/// synport: $SYNPORT, PATH или synmobile/tools/synport рядом с проектами.
fn synport() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("SYNPORT") {
        return Some(PathBuf::from(p));
    }
    util::which("synport").or_else(|| {
        let h = util::home();
        [h.join("Storage/Проекты/2027/synmobile/tools/synport/target/fast-release/synport"), h.join("Projects/2027/synmobile/tools/synport/target/fast-release/synport")].into_iter().find(|p| p.is_file())
    })
}

/// Поставить программу (и ярлык со значком) на устройство с Linux по ssh в /usr/local.
fn install_ssh(app: &App, device: &str, bin_path: &std::path::Path) -> Result<()> {
    let bin = app.bin()?;
    let stage = app.work_dir("stage").join("device");
    linux::stage(app, bin_path, &stage, Some(Target::LinuxAarch64))?;
    let tarball = app.work_dir("stage").join(format!("{bin}-device.tar"));
    let mut c = Command::new("tar");
    c.arg("-cf").arg(&tarball).arg("-C").arg(stage.join("usr")).arg(".");
    util::run(c)?;
    let remote_tar = format!("/tmp/{bin}-syngui.tar");
    // /usr/local: телефон ставит программы мимо pacman; старый процесс не мешает (mv поверх)
    let script = format!("set -e; d=$(mktemp -d); tar -xf {remote_tar} -C $d; install -Dm755 $d/bin/{bin} /usr/local/bin/{bin}.new && mv -f /usr/local/bin/{bin}.new /usr/local/bin/{bin}; rm -f $d/bin/{bin}; cp -r $d/. /usr/local/; rm -rf $d {remote_tar}; echo установлено: /usr/local/bin/{bin}");
    util::step(format!("установка на {device}"));
    if device == "phone" {
        let sp = synport().context("нет synport (SYNPORT=<путь>) — им идёт связь с телефоном; или --device ssh:root@<адрес>")?;
        // synport ищет свои файлы от корня synmobile
        let root = sp.ancestors().nth(5).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
        let mut c = Command::new(&sp);
        c.current_dir(&root).arg("scp").arg(&tarball).arg(format!("phone:{remote_tar}"));
        util::run(c)?;
        let mut c = Command::new(&sp);
        c.current_dir(&root).arg("ssh").arg(script);
        util::run(c)?;
    } else {
        let host = device.trim_start_matches("ssh:");
        let mut c = Command::new("scp");
        c.arg("-q").arg(&tarball).arg(format!("{host}:{remote_tar}"));
        util::run(c)?;
        let mut c = Command::new("ssh");
        c.arg(host).arg(script);
        util::run(c)?;
    }
    Ok(())
}

fn info(app: &App) -> Result<()> {
    let yn = |b: bool| if b { "есть" } else { "нет" };
    println!("крейт:        {} {}", app.krate, app.version);
    println!("каталог:      {}", app.manifest_dir.display());
    println!("имя:          {}", app.display_name());
    println!("id:           {}", app.app_id());
    println!("программы:    {}", if app.bins.is_empty() { "—".into() } else { app.bins.join(", ") });
    println!("основная:     {}", app.bin().unwrap_or_else(|e| format!("? ({e})")));
    println!("библиотека:   {}", app.lib.clone().unwrap_or_else(|| "—".into()));
    println!("значок:       {}", app.icon().map(|p| p.display().to_string()).unwrap_or_else(|| "нет (будет буква)".into()));
    println!("ярлык:        {}", app.desktop_file().map(|p| p.display().to_string()).unwrap_or_else(|| "нет".into()));
    println!("репозиторий:  {}", app.repository.clone().unwrap_or_else(|| "—".into()));
    let a = &app.meta.android;
    println!("android:      id {}, lib {}, ABI {}, minSdk {}, versionCode {}",
        a.id.clone().unwrap_or_else(|| app.app_id()),
        a.lib.clone().or_else(|| app.lib.clone()).unwrap_or_else(|| "—".into()),
        a.abis.clone().unwrap_or_else(|| vec!["arm64-v8a".into()]).join(","),
        a.min_sdk.unwrap_or(30),
        app.version_code());
    let kd = android::key_dir(app);
    println!("ключ android: {} ({})", kd.display(), yn(kd.join("release.jks").is_file() && kd.join("password").is_file()));
    println!("ios:          bundle {}", app.meta.ios.bundle_id.clone().unwrap_or_else(|| app.app_id()));
    println!("артефакты:    {}", app.dist_dir().display());
    Ok(())
}

fn init(app: &App) -> Result<()> {
    let path = app.manifest_dir.join("Cargo.toml");
    let text = std::fs::read_to_string(&path)?;
    if text.contains("[package.metadata.syngui]") {
        bail!("в {} уже есть [package.metadata.syngui]", path.display());
    }
    let bin = app.bin().unwrap_or_else(|_| app.krate.clone());
    let block = format!(
        r#"
# syngui build/package/run — метаданные приложения (все поля необязательны).
[package.metadata.syngui]
name = "{name}"
id = "{id}"
# icon = "data/icons/{bin}.svg"

[package.metadata.syngui.android]
# permissions = ["INTERNET"]
# features = ["android"]
# ffmpeg = true

[package.metadata.syngui.ios]
# status_bar_hidden = true

[package.metadata.syngui.linux]
# depends = ["gcc-libs", "fontconfig", "libxkbcommon", "wayland", "vulkan-icd-loader"]
"#,
        name = app.display_name(),
        id = app.app_id(),
    );
    let mut t = text;
    if !t.ends_with('\n') {
        t.push('\n');
    }
    t.push_str(&block);
    std::fs::write(&path, t)?;
    println!("{}: добавлен [package.metadata.syngui]", path.display());
    Ok(())
}

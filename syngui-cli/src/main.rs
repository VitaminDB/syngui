//! `syngui` — сборка, упаковка и запуск приложений на syngui (как `flutter build` / `cargo tauri`).
//!
//! ```text
//! syngui doctor [-t android]                       что есть для сборки
//! syngui info [-p крейт]                           что утилита знает о приложении
//! syngui build -t linux-x86_64|linux-aarch64|android|ios [--release] [-p крейт]
//! syngui package -f tar|arch|aur|apk|ipa [-t …] [-p крейт]
//! syngui run [-d local|android|ios|phone|ssh:host] [-p крейт]
//! syngui release 0.2.0 [--apply|--publish]        выпуск (по умолчанию — сухой прогон)
//! syngui keygen [-p крейт]                         релизный ключ Android в ~/.secrets/<крейт>-android
//! syngui init [-p крейт]                           заготовка [package.metadata.syngui]
//! ```

mod android;
mod device;
mod doctor;
mod icon;
mod ios;
mod linux;
mod package;
mod project;
mod publish;
mod release;
mod target;
mod util;

use anyhow::{bail, Result};
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
        /// linux-x86_64 в контейнере со старой glibc (2.31): запуск вне Arch — Ubuntu 20.04+, Debian 11+
        #[arg(long)]
        portable: bool,
        /// Всё рабочее пространство по syngui-release.toml (Linux): программы всех пакетов выпуска, без упаковки
        #[arg(short, long)]
        workspace: bool,
        /// С -w: только эти пакеты выпуска (через запятую)
        #[arg(long, value_delimiter = ',')]
        only: Vec<String>,
        /// С -w: описание выпуска вместо syngui-release.toml
        #[arg(long)]
        release_file: Option<PathBuf>,
        /// С -w: не делать strip копий программ (по умолчанию strip, как в пакетах)
        #[arg(long)]
        no_strip: bool,
        /// Доп. аргументы cargo (Linux): после --
        #[arg(last = true)]
        cargo_args: Vec<String>,
    },
    /// Упаковать: tar (архив релиза, основа -bin), arch (пакет pacman), aur (PKGBUILD -git и -bin), apk, ipa, zip (Windows)
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
        /// Выпуск всего рабочего пространства по syngui-release.toml (несколько пакетов; arch, tar, aur)
        #[arg(short, long)]
        workspace: bool,
        /// Только эти пакеты выпуска (через запятую)
        #[arg(long, value_delimiter = ',')]
        only: Vec<String>,
        /// Описание выпуска вместо syngui-release.toml
        #[arg(long)]
        release_file: Option<PathBuf>,
        /// Поставить собранные пакеты одной транзакцией (sudo pacman -U; пароль можно подать в stdin)
        #[arg(long)]
        install: bool,
        /// Не убирать символы из программ
        #[arg(long)]
        no_strip: bool,
        /// tar/arch для linux-x86_64 — переносимая сборка в контейнере (glibc 2.31)
        #[arg(long)]
        portable: bool,
        /// С --install: поставить пакеты на устройство (phone, ssh:<хост>) вместо этой машины
        #[arg(short, long)]
        device: Option<String>,
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
        /// Телефон/ssh: поставить пакетом pacman (package -f arch), а не в /usr/local
        #[arg(long)]
        pkg: bool,
        /// Телефон/ssh: не собирать и не ставить — только запустить установленную программу
        #[arg(long)]
        no_install: bool,
        /// Телефон/ssh: только поставить, не запускать
        #[arg(long)]
        no_launch: bool,
        /// Возможности cargo (поверх метаданных)
        #[arg(long, value_delimiter = ',')]
        features: Vec<String>,
        /// cargo --no-default-features
        #[arg(long)]
        no_default_features: bool,
        /// Переменные окружения программы на устройстве (KEY=VAL, можно несколько)
        #[arg(long = "env")]
        env: Vec<String>,
        /// Аргументы программы: после --
        #[arg(last = true)]
        args: Vec<String>,
    },
    /// Выпуск: версия в Cargo.toml, коммит, тег; --publish — архивы, APK/IPA, push, релиз GitHub, AUR.
    /// Без флагов — сухой прогон
    Release {
        /// Новая версия (0.2.0)
        version: String,
        #[command(flatten)]
        sel: Sel,
        /// Поднять версию, закоммитить и поставить тег (локально, без публикации)
        #[arg(long)]
        apply: bool,
        /// Опубликовать: собрать артефакты (--release), git push, gh release create, AUR с суммами
        #[arg(long)]
        publish: bool,
        /// Без APK/IPA
        #[arg(long)]
        no_mobile: bool,
        /// Описание выпуска вместо syngui-release.toml
        #[arg(long)]
        release_file: Option<PathBuf>,
        /// Приложить к релизу GitHub готовые файлы (каталог — его содержимое; можно несколько)
        #[arg(long)]
        assets: Vec<PathBuf>,
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
        Cmd::Build { target, sel, prof, portable, workspace: true, only, release_file, no_strip, cargo_args } => {
            if portable || !cargo_args.is_empty() {
                bail!("с -w: --portable и аргументы cargo не поддерживаются (особая сборка крейтов — в syngui-release.toml)");
            }
            let r = release::Release::load(sel.manifest_path.as_deref(), release_file.as_deref())?;
            for t in targets(&target)? {
                ensure_linux(t)?;
                // копии в target/syngui/release (strip — как в пакетах; target/ cargo не трогается)
                let dir = r.build_bins(&only, t, prof.profile(), !no_strip)?;
                println!("{t}: {}", dir.display());
            }
        }
        Cmd::Build { target, sel, prof, portable, cargo_args, .. } => {
            let app = load(&sel)?;
            let mut done = Vec::new();
            for t in targets(&target)? {
                let p = build(&app, t, prof.profile(), &cargo_args, portable)?;
                done.push((t, p));
            }
            for (t, p) in done {
                println!("{t}: {}", p.display());
            }
        }
        Cmd::Package { format, target, sel, fast, out, workspace, only, release_file, install, no_strip, portable, device } => {
            let dev = match &device {
                Some(d) => Some(device::Device::parse(d)?),
                None => None,
            };
            let profile = if fast { Profile::Fast } else { Profile::Release };
            if workspace {
                let r = release::Release::load(sel.manifest_path.as_deref(), release_file.as_deref())?;
                let mut results = Vec::new();
                match format.as_str() {
                    "arch" | "pacman" => {
                        for t in targets(&target)? {
                            ensure_linux(t)?;
                            let (pkgs, _) = r.arch(&only, t, profile, !no_strip)?;
                            if install {
                                match &dev {
                                    Some(d) => device::install_packages(d, &pkgs)?,
                                    None if t.is_host() => release::install(&pkgs)?,
                                    None => util::warn(format!("--install: пакеты {t} не для этой машины — укажите --device phone|ssh:<хост>")),
                                }
                            }
                            results.extend(pkgs);
                        }
                    }
                    "tar" | "bin" => {
                        for t in targets(&target)? {
                            ensure_linux(t)?;
                            results.extend(r.tars(&only, t, profile, !no_strip)?);
                        }
                    }
                    "aur" => {
                        let out = out.unwrap_or_else(|| r.target_dir.join("syngui/aur"));
                        results.extend(r.aur(&only, &out)?);
                    }
                    f => bail!("--workspace: формат {f} не поддержан — arch, tar, aur"),
                }
                if format != "aur" {
                    results.push(package::sha256sums(&r.target_dir.join("syngui/dist"))?);
                }
                for p in results {
                    println!("{}", p.display());
                }
                return Ok(());
            }
            let app = load(&sel)?;
            let publish = !fast;
            let mut results = Vec::new();
            match format.as_str() {
                "tar" | "bin" => {
                    for t in targets(&target)? {
                        ensure_linux(t)?;
                        results.push(package::tar(&app, t, profile, portable, &[])?);
                    }
                }
                "arch" | "pacman" => {
                    for t in targets(&target)? {
                        ensure_linux(t)?;
                        let p = package::arch(&app, t, profile, portable, &[])?;
                        if install {
                            match &dev {
                                Some(d) => device::install_packages(d, std::slice::from_ref(&p))?,
                                None if t.is_host() => release::install(std::slice::from_ref(&p))?,
                                None => util::warn(format!("--install: пакет {t} не для этой машины — укажите --device phone|ssh:<хост>")),
                            }
                        }
                        results.push(p);
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
                "zip" | "windows" => results.push(package::zip(&app, profile)?),
                f => bail!("неизвестный формат {f}: tar, arch, aur, apk, ipa, zip"),
            }
            if format != "aur" {
                results.push(package::sha256sums(&app.dist_dir())?);
            }
            for r in results {
                println!("{}", r.display());
            }
        }
        Cmd::Run { device, sel, prof, no_build, no_log, pkg, no_install, no_launch, features, no_default_features, env, args } => {
            let app = load(&sel)?;
            let mut extra = Vec::new();
            if no_default_features {
                extra.push("--no-default-features".to_string());
            }
            if !features.is_empty() {
                extra.push("--features".into());
                extra.push(features.join(","));
            }
            let o = RunOpts { no_build, log: !no_log, pkg, no_install, no_launch, extra, env };
            run(&app, &device, prof.profile(), &o, &args)?;
        }
        Cmd::Release { version, sel, apply, publish, no_mobile, release_file, assets } => publish::run(publish::Opts {
            version: &version,
            manifest: sel.manifest_path.as_deref(),
            package: sel.package.as_deref(),
            apply,
            publish,
            no_mobile,
            release_file: release_file.as_deref(),
            assets,
        })?,
        Cmd::Keygen { sel, dname } => android::keygen(&load(&sel)?, &dname)?,
        Cmd::Init { sel } => init(&load(&sel)?)?,
    }
    Ok(())
}

fn ensure_linux(t: Target) -> Result<()> {
    if matches!(t, Target::Android | Target::Ios | Target::WindowsX86_64) {
        let f = match t {
            Target::Android => "apk",
            Target::Ios => "ipa",
            _ => "zip",
        };
        bail!("формат tar/arch — для Linux-целей; для {t}: --format {f}");
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

fn build(app: &App, t: Target, profile: Profile, extra: &[String], portable: bool) -> Result<PathBuf> {
    match t {
        Target::LinuxX86_64 | Target::LinuxAarch64 | Target::WindowsX86_64 => linux::build(app, t, profile, extra, portable),
        Target::Android => android::build(app, profile, false, None),
        Target::Ios => ios::build(app, profile),
    }
}

struct RunOpts {
    no_build: bool,
    log: bool,
    pkg: bool,
    no_install: bool,
    no_launch: bool,
    extra: Vec<String>,
    env: Vec<String>,
}

fn run(app: &App, device: &str, profile: Profile, o: &RunOpts, args: &[String]) -> Result<()> {
    let no_build = o.no_build;
    match device {
        "local" | "host" | "here" => {
            let bin = app.bin()?;
            let mut c = linux::cargo(app, "run", profile);
            c.arg("--bin").arg(&bin);
            if o.extra.is_empty() {
                c.args(app.linux_feature_args(Target::host_linux().arch()));
            }
            c.args(&o.extra);
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
            android::install_run(app, &apk, o.log)
        }
        "ios" | "iphone" => {
            let ipa = if no_build { app.work_dir("ios").join(format!("{}-{}.ipa", app.krate, app.version)) } else { ios::build(app, profile)? };
            ios::install(&ipa)
        }
        d if d == "phone" || d.starts_with("ssh:") => {
            let dev = device::Device::parse(d)?;
            let bin = app.bin()?;
            let t = Target::LinuxAarch64;
            if !o.no_install {
                if o.pkg {
                    let p = package::arch(app, t, profile, false, &o.extra)?;
                    device::install_packages(&dev, &[p])?;
                } else {
                    let bin_path = if no_build {
                        app.target_dir.join(if t.is_host() { String::new() } else { t.triple().into() }).join(profile.dir()).join(&bin)
                    } else {
                        linux::build(app, t, profile, &o.extra, false)?
                    };
                    let stage = app.work_dir("stage").join("device");
                    linux::stage(app, &bin_path, &stage, Some(t))?;
                    let tarball = app.work_dir("stage").join(format!("{bin}-device.tar"));
                    let mut c = Command::new("tar");
                    c.arg("-cf").arg(&tarball).arg("-C").arg(stage.join("usr")).arg(".");
                    util::run(c)?;
                    device::install_tree(&dev, &bin, &tarball)?;
                }
            }
            if o.no_launch {
                return Ok(());
            }
            device::launch(&dev, &bin, args, &o.env)
        }
        d => bail!("неизвестное устройство {d}: local, android, ios, phone, ssh:<хост>"),
    }
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

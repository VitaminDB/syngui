//! Веб-цель: WebAssembly в браузере (в том числе Telegram Mini App).
//!
//! `syngui build -t web` → `cargo build --lib --target wasm32-unknown-unknown`, затем
//! `wasm-bindgen --target web --no-typescript`; результат — готовый к выкладке каталог
//! `target/syngui/web/<крейт>/` (index.html, pkg/<lib>.js, pkg/<lib>_bg.wasm, ассеты).
//! `syngui run -d web` раздаёт его встроенным HTTP-сервером.
//!
//! ```toml
//! [package.metadata.syngui.web]
//! features = ["web"]          # возможности cargo для wasm (по умолчанию — крейта по умолчанию)
//! default_features = false
//! assets = "web/assets"       # содержимое — в корень сайта (по умолчанию web/assets, если есть)
//! index = "web/index.html"    # своя страница (по умолчанию web/index.html, если есть; иначе генерируется)
//! title = "Пульт"             # <title> сгенерированной страницы (по умолчанию name)
//! background = "#000000"      # фон страницы до первого кадра
//! telegram = true             # Telegram Mini App: telegram-web-app.js до модуля
//! ```

use crate::project::App;
use crate::target::Profile;
use crate::{linux, util};
use anyhow::{bail, Context, Result};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;

pub const TRIPLE: &str = "wasm32-unknown-unknown";
const TELEGRAM_JS: &str = "https://telegram.org/js/telegram-web-app.js";

/// Каталог готового сайта.
pub fn out_dir(app: &App) -> PathBuf {
    app.work_dir("web")
}

/// Аргументы возможностей cargo из `[package.metadata.syngui.web]`.
pub fn feature_args(app: &App) -> Vec<String> {
    let w = &app.meta.web;
    let mut v = Vec::new();
    if !w.default_features.unwrap_or(true) {
        v.push("--no-default-features".to_string());
    }
    if let Some(f) = w.features.as_ref().filter(|f| !f.is_empty()) {
        v.push("--features".into());
        v.push(f.join(","));
    }
    v
}

/// Версии крейта wasm-bindgen в Cargo.lock рабочего пространства.
pub fn locked_versions(workspace_root: &Path) -> Vec<String> {
    let Ok(lock) = std::fs::read_to_string(workspace_root.join("Cargo.lock")) else {
        return Vec::new();
    };
    let mut v = Vec::new();
    let mut lines = lock.lines();
    while let Some(l) = lines.next() {
        if l.trim() == "name = \"wasm-bindgen\"" {
            if let Some(ver) = lines
                .next()
                .and_then(|n| n.trim().strip_prefix("version = \""))
                .and_then(|n| n.strip_suffix('"'))
            {
                if !v.iter().any(|x| x == ver) {
                    v.push(ver.to_string());
                }
            }
        }
    }
    v
}

/// Версия wasm-bindgen-cli по пути (`wasm-bindgen 0.2.127` → `0.2.127`).
pub fn cli_version_of(bin: &Path) -> Option<String> {
    let o = Command::new(bin).arg("--version").output().ok()?;
    String::from_utf8_lossy(&o.stdout)
        .split_whitespace()
        .nth(1)
        .map(str::to_string)
}

/// Версия wasm-bindgen-cli из PATH.
pub fn cli_version() -> Option<String> {
    cli_version_of(&util::which("wasm-bindgen")?)
}

/// Свой wasm-bindgen-cli нужной версии: ~/.cache/syngui/wasm-bindgen-<версия>/bin/wasm-bindgen.
pub fn cached_cli(version: &str) -> PathBuf {
    let cache = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| util::home().join(".cache"));
    cache
        .join("syngui")
        .join(format!("wasm-bindgen-{version}"))
        .join("bin/wasm-bindgen")
}

/// Расхождение версий wasm-bindgen-cli в PATH и крейта в Cargo.lock (None — совпадают или lock нет).
pub fn mismatch_hint(cli: &str, locked: &[String]) -> Option<String> {
    if locked.is_empty() || locked.iter().any(|l| l == cli) {
        return None;
    }
    let lock = locked.join(", ");
    let want = newest(locked);
    let cached = cached_cli(&want);
    Some(if cached.is_file() {
        format!(
            "в PATH wasm-bindgen-cli {cli}, в Cargo.lock {lock} — сборка возьмёт {}",
            cached.display()
        )
    } else {
        format!(
            "в PATH wasm-bindgen-cli {cli}, в Cargo.lock {lock} — сборка сама поставит {want} в {} \
             (или: cargo install wasm-bindgen-cli --version {want} --locked)",
            cached.display()
        )
    })
}

fn newest(v: &[String]) -> String {
    let mut v = v.to_vec();
    v.sort_by(|a, b| util::version_cmp(a, b));
    v.pop().unwrap_or_default()
}

fn ensure_target() -> Result<()> {
    let installed = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .any(|l| l == TRIPLE)
        })
        .unwrap_or(true);
    if !installed {
        bail!("нет цели {TRIPLE}: rustup target add {TRIPLE}");
    }
    Ok(())
}

/// wasm-bindgen-cli той же версии, что крейт wasm-bindgen в Cargo.lock (иначе он откажется
/// работать): из PATH, если совпадает, иначе свой в ~/.cache/syngui (ставится один раз).
fn wasm_bindgen(app: &App) -> Result<PathBuf> {
    let locked = locked_versions(&app.workspace_root);
    let path = util::which("wasm-bindgen");
    let path_ver = path.as_deref().and_then(cli_version_of);
    if locked.is_empty() || path_ver.as_ref().is_some_and(|v| locked.contains(v)) {
        return path.context(
            "нет wasm-bindgen: cargo install wasm-bindgen-cli --locked (версия — как у крейта wasm-bindgen в Cargo.lock)",
        );
    }
    if locked.len() > 1 {
        util::warn(format!(
            "в Cargo.lock несколько версий wasm-bindgen ({}) — беру новейшую",
            locked.join(", ")
        ));
    }
    let want = newest(&locked);
    let bin = cached_cli(&want);
    if !bin.is_file() {
        util::step(format!(
            "wasm-bindgen-cli {want} (в PATH {}) → {}",
            path_ver.as_deref().unwrap_or("нет"),
            bin.display()
        ));
        let root = bin.parent().and_then(Path::parent).unwrap();
        let mut c = Command::new("cargo");
        c.args([
            "install",
            "wasm-bindgen-cli",
            "--locked",
            "--version",
            &want,
            "--root",
        ])
        .arg(root);
        util::run(c).context("установка wasm-bindgen-cli нужной версии")?;
    }
    Ok(bin)
}

/// Собрать сайт; вернуть каталог `target/syngui/web/<крейт>`.
/// `extra` — свои аргументы cargo (возможности); без них — из метаданных.
pub fn build(app: &App, profile: Profile, extra: &[String]) -> Result<PathBuf> {
    ensure_target()?;
    let Some(lib) = app.lib.clone() else {
        bail!(
            "у крейта {} нет [lib]: для веба нужен crate-type = [\"cdylib\", \"rlib\"] и #[wasm_bindgen(start)]",
            app.krate
        );
    };
    if !app.lib_cdylib {
        bail!(
            "[lib] крейта {} без cdylib: добавьте crate-type = [\"cdylib\", \"rlib\"]",
            app.krate
        );
    }
    util::step(format!("{}: wasm ({})", app.krate, profile.dir()));
    let mut c = linux::cargo(app, "build", profile);
    c.args(["--lib", "--target", TRIPLE]);
    if extra.is_empty() {
        c.args(feature_args(app));
    }
    c.args(extra);
    util::run(c)?;
    let wasm = app
        .target_dir
        .join(TRIPLE)
        .join(profile.dir())
        .join(format!("{lib}.wasm"));
    if !wasm.is_file() {
        bail!("нет {} после сборки", wasm.display());
    }

    let out = out_dir(app);
    let pkg = out.join("pkg");
    let _ = std::fs::remove_dir_all(&pkg);
    util::step(format!("wasm-bindgen → {}", pkg.display()));
    let mut c = Command::new(wasm_bindgen(app)?);
    c.args(["--target", "web", "--no-typescript", "--out-dir"])
        .arg(&pkg)
        .arg(&wasm);
    if profile == Profile::Dev {
        c.arg("--debug");
    }
    util::run(c)?;
    if profile == Profile::Release {
        wasm_opt(&pkg.join(format!("{lib}_bg.wasm")));
    }

    // ассеты — в корень сайта (with_font_url("fonts/X.ttf") ↔ web/assets/fonts/X.ttf)
    let w = &app.meta.web;
    let assets = match &w.assets {
        Some(a) => {
            let p = app.path(a);
            if !p.is_dir() {
                bail!("web.assets: нет каталога {}", p.display());
            }
            Some(p)
        }
        None => Some(app.manifest_dir.join("web/assets")).filter(|p| p.is_dir()),
    };
    if let Some(a) = &assets {
        util::note(format!("ассеты: {}", a.display()));
        util::copy_tree(a, &out)?;
    }

    let index = match &w.index {
        Some(i) => {
            let p = app.path(i);
            Some(
                std::fs::read_to_string(&p)
                    .with_context(|| format!("web.index: {}", p.display()))?,
            )
        }
        None => std::fs::read_to_string(app.manifest_dir.join("web/index.html")).ok(),
    };
    let html = match index {
        Some(h) if w.telegram && !h.contains("telegram-web-app.js") => {
            let tag = format!("<script src=\"{TELEGRAM_JS}\"></script>\n");
            match h.find("</head>") {
                Some(i) => format!("{}{tag}{}", &h[..i], &h[i..]),
                None => format!("{tag}{h}"),
            }
        }
        Some(h) => h,
        None => generate_index(app, &lib),
    };
    util::write_if_changed(&out.join("index.html"), html)?;
    Ok(out)
}

/// Уменьшить .wasm через wasm-opt -Oz (binaryen), если он есть; ошибка — только предупреждение.
fn wasm_opt(wasm: &Path) {
    if util::which("wasm-opt").is_none() {
        util::note("wasm-opt нет — .wasm без доп. сжатия (pacman -S binaryen)");
        return;
    }
    let tmp = wasm.with_extension("opt.wasm");
    let mut c = Command::new("wasm-opt");
    c.args(["-Oz", "--all-features", "-o"]).arg(&tmp).arg(wasm);
    util::step("wasm-opt -Oz");
    match util::run(c).and_then(|_| Ok(std::fs::rename(&tmp, wasm)?)) {
        Ok(()) => {}
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            util::warn(format!("wasm-opt: {e:#}"));
        }
    }
}

fn generate_index(app: &App, lib: &str) -> String {
    let w = &app.meta.web;
    let title = util::xml(&w.title.clone().unwrap_or_else(|| app.display_name()));
    let bg = w.background.clone().unwrap_or_else(|| "#000".into());
    let telegram = if w.telegram {
        format!("    <script src=\"{TELEGRAM_JS}\"></script>\n")
    } else {
        String::new()
    };
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no, interactive-widget=resizes-content">
    <title>{title}</title>
    <style>
        html, body {{ margin: 0; padding: 0; width: 100%; height: 100%; overflow: hidden; background: {bg}; }}
        canvas {{ width: 100%; height: 100%; display: block; touch-action: none; outline: none; }}
    </style>
{telegram}</head>
<body>
    <script type="module">
        import init from './pkg/{lib}.js';
        init();
    </script>
</body>
</html>
"#
    )
}

/// Раздать каталог по HTTP (GET/HEAD, без кэша); блокирует до Ctrl+C.
pub fn serve(dir: &Path, listen: &str) -> Result<()> {
    let (host, port) = match listen.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse::<u16>().context("порт")?),
        None => (listen.to_string(), 8080),
    };
    let mut bound = None;
    for p in port..port.saturating_add(20) {
        if let Ok(l) = TcpListener::bind((host.as_str(), p)) {
            bound = Some((l, p));
            break;
        }
    }
    let Some((listener, port)) = bound else {
        bail!(
            "не удалось занять порт {host}:{port}…{}",
            port.saturating_add(19)
        );
    };
    let shown = if host == "0.0.0.0" {
        "localhost"
    } else {
        &host
    };
    println!("сайт: {}", dir.display());
    println!("\x1b[1mhttp://{shown}:{port}/\x1b[0m  (Ctrl+C — остановить)");
    let root = dir.canonicalize()?;
    for s in listener.incoming().flatten() {
        let root = root.clone();
        std::thread::spawn(move || {
            let _ = handle(s, &root);
        });
    }
    Ok(())
}

fn handle(mut s: TcpStream, root: &Path) -> std::io::Result<()> {
    let mut r = BufReader::new(s.try_clone()?);
    let mut line = String::new();
    r.read_line(&mut line)?;
    // заголовки не нужны — дочитать до пустой строки
    loop {
        let mut h = String::new();
        if r.read_line(&mut h)? == 0 || h == "\r\n" || h == "\n" {
            break;
        }
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    let path = percent_decode(target.split(['?', '#']).next().unwrap_or("/"));
    let reply = |s: &mut TcpStream, code: &str, ctype: &str, body: &[u8], head: bool| {
        let hdr = format!(
            "HTTP/1.1 {code}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
            body.len()
        );
        s.write_all(hdr.as_bytes())?;
        if !head {
            s.write_all(body)?;
        }
        s.flush()
    };
    if method != "GET" && method != "HEAD" {
        return reply(
            &mut s,
            "405 Method Not Allowed",
            "text/plain",
            b"405",
            false,
        );
    }
    let head = method == "HEAD";
    let rel: PathBuf = path
        .split('/')
        .filter(|p| !p.is_empty() && *p != "." && *p != "..")
        .collect();
    let mut file = root.join(rel);
    if file.is_dir() {
        file = file.join("index.html");
    }
    let ok = file
        .canonicalize()
        .map(|f| f.starts_with(root))
        .unwrap_or(false);
    match std::fs::read(&file) {
        Ok(data) if ok => {
            eprintln!("{method} {target} 200");
            reply(&mut s, "200 OK", content_type(&file), &data, head)
        }
        _ => {
            eprintln!("{method} {target} 404");
            reply(&mut s, "404 Not Found", "text/plain", b"404", head)
        }
    }
}

fn content_type(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "wasm" => "application/wasm",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "txt" | "mss" => "text/plain; charset=utf-8",
        "mp3" => "audio/mpeg",
        "ogg" => "audio/ogg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        _ => "application/octet-stream",
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = |c: u8| (c as char).to_digit(16);
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

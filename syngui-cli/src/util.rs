//! Общие мелочи: запуск команд с выводом строки, поиск программ и путей.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Заголовок шага: `==> текст`.
pub fn step(msg: impl AsRef<str>) {
    eprintln!("\x1b[1;34m==>\x1b[0m \x1b[1m{}\x1b[0m", msg.as_ref());
}

pub fn note(msg: impl AsRef<str>) {
    eprintln!("    {}", msg.as_ref());
}

pub fn warn(msg: impl AsRef<str>) {
    eprintln!("\x1b[1;33mвнимание:\x1b[0m {}", msg.as_ref());
}

/// Показать команду (как в shell) — для `--verbose` и сообщений об ошибке.
pub fn show(c: &Command) -> String {
    let mut s = String::new();
    for (k, v) in c.get_envs() {
        if let Some(v) = v {
            let k = k.to_string_lossy();
            if k.contains("PASSWORD") {
                continue;
            }
            s.push_str(&format!("{}={} ", k, quote(&v.to_string_lossy())));
        }
    }
    s.push_str(&quote(&c.get_program().to_string_lossy()));
    for a in c.get_args() {
        s.push(' ');
        s.push_str(&quote(&a.to_string_lossy()));
    }
    s
}

fn quote(s: &str) -> String {
    if !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_./=:,+@%".contains(c)) {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

pub fn verbose() -> bool {
    std::env::var_os("SYNGUI_VERBOSE").is_some()
}

/// Запустить и дождаться; ненулевой код — ошибка с текстом команды.
pub fn run(mut c: Command) -> Result<()> {
    if verbose() {
        note(format!("$ {}", show(&c)));
    }
    let st = c.status().with_context(|| format!("не запускается: {}", c.get_program().to_string_lossy()))?;
    if !st.success() {
        bail!("команда завершилась с ошибкой ({st}):\n  {}", show(&c));
    }
    Ok(())
}

/// Запустить и забрать stdout (stderr — на экран).
pub fn capture(mut c: Command) -> Result<String> {
    if verbose() {
        note(format!("$ {}", show(&c)));
    }
    let out = c
        .stderr(std::process::Stdio::inherit())
        .output()
        .with_context(|| format!("не запускается: {}", c.get_program().to_string_lossy()))?;
    if !out.status.success() {
        bail!("команда завершилась с ошибкой ({}):\n  {}", out.status, show(&c));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Программа в PATH.
pub fn which(name: &str) -> Option<PathBuf> {
    if name.contains('/') {
        let p = PathBuf::from(name);
        return p.is_file().then_some(p);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path).map(|d| d.join(name)).find(|p| p.is_file())
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

/// `~/…` → домашний каталог.
pub fn expand(p: &str) -> PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        home().join(rest)
    } else if p == "~" {
        home()
    } else {
        PathBuf::from(p)
    }
}

/// Последний по версии подкаталог (`build-tools/35.0.0`, `ndk/27.1…`).
pub fn latest_subdir(dir: &Path) -> Option<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir).ok()?.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect();
    v.sort_by(|a, b| version_cmp(&a.file_name().unwrap().to_string_lossy(), &b.file_name().unwrap().to_string_lossy()));
    v.pop()
}

/// Сравнение строк версий по числовым частям (`9.0` < `10.0`).
pub fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let parts = |s: &str| -> Vec<u64> { s.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).map(|p| p.parse().unwrap_or(0)).collect() };
    parts(a).cmp(&parts(b))
}

pub fn write(path: &Path, data: impl AsRef<[u8]>) -> Result<()> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).with_context(|| format!("mkdir {}", d.display()))?;
    }
    std::fs::write(path, data).with_context(|| format!("запись {}", path.display()))
}

/// Записать, только если содержимое изменилось (не трогать mtime — gradle и cargo не пересобирают зря).
pub fn write_if_changed(path: &Path, data: impl AsRef<[u8]>) -> Result<()> {
    if std::fs::read(path).ok().as_deref() == Some(data.as_ref()) {
        return Ok(());
    }
    write(path, data)
}

pub fn copy(from: &Path, to: &Path) -> Result<()> {
    if let Some(d) = to.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::copy(from, to).with_context(|| format!("копирование {} → {}", from.display(), to.display()))?;
    Ok(())
}

/// Скопировать дерево каталогов (поверх существующего).
pub fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    for e in std::fs::read_dir(from).with_context(|| format!("чтение {}", from.display()))? {
        let e = e?;
        let dst = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_tree(&e.path(), &dst)?;
        } else {
            let data = std::fs::read(e.path())?;
            write_if_changed(&dst, data)?;
        }
    }
    Ok(())
}

/// sha256 файла рядом: `<файл>.sha256` в формате sha256sum.
pub fn sha256_file(path: &Path) -> Result<String> {
    let dir = path.parent().unwrap();
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    let mut c = Command::new("sha256sum");
    c.current_dir(dir).arg(&name);
    let line = capture(c)?;
    write(&dir.join(format!("{name}.sha256")), &line)?;
    Ok(line.split_whitespace().next().unwrap_or_default().to_string())
}

/// Экранирование для XML-атрибутов и текста.
pub fn xml(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Строка для bash в одинарных кавычках.
pub fn sh(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

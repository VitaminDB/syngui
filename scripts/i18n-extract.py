#!/usr/bin/env python3
"""Сборщик каталога переводов исходных строк syngui (`t!`, `tn!`, `n_!`).

    i18n-extract.py КАТАЛОГ.lang ИСХОДНИКИ... [--tag en --name English] [--check]

Находит в `.rs` строки `t!("…")`, `tn!(n, "…", "…", "…")` (ключ — первая
форма), `n_!("…")` и обновляет каталог: переводы сохраняются, новые строки
добавляются с пустым переводом (`"…" = ""` — ещё не переведено, syngui
показывает исходник), исчезнувшие из кода — удаляются. Строки идут по файлам,
с комментарием `# путь`. `--check` — ничего не писать, код выхода 1, если
каталог устарел или есть непереведённое.
"""

import argparse
import os
import re
import sys

MACRO = re.compile(r"(?<![A-Za-z0-9_])(t|tn|n_)!\s*\(")


def parse_literal(src, i):
    """Строковый литерал Rust с позиции i (`"…"` или `r#"…"#`): (текст, конец) или None."""
    m = re.match(r'r(#*)"', src[i:])
    if m:
        hashes = m.group(1)
        start = i + m.end()
        end = src.find('"' + hashes, start)
        if end < 0:
            return None
        return src[start:end], end + 1 + len(hashes)
    if i >= len(src) or src[i] != '"':
        return None
    out = []
    j = i + 1
    while j < len(src):
        c = src[j]
        if c == '"':
            return "".join(out), j + 1
        if c == "\\":
            n = src[j + 1]
            if n == "\n":
                # перенос строки в литерале: «\» + пробелы следующей строки съедаются
                j += 2
                while j < len(src) and src[j] in " \t\r\n":
                    j += 1
                continue
            simple = {"n": "\n", "t": "\t", "r": "\r", "0": "\0", '"': '"', "'": "'", "\\": "\\"}
            if n in simple:
                out.append(simple[n])
                j += 2
                continue
            if n == "u":
                close = src.index("}", j)
                out.append(chr(int(src[j + 3 : close], 16)))
                j = close + 1
                continue
            if n == "x":
                out.append(chr(int(src[j + 2 : j + 4], 16)))
                j += 4
                continue
            raise ValueError(f"неизвестный escape \\{n}")
        out.append(c)
        j += 1
    return None


def skip_ws(src, i):
    while i < len(src) and src[i] in " \t\r\n":
        i += 1
    return i


def extract(path):
    """[(msgid, plural)] файла в порядке появления."""
    src = open(path, encoding="utf-8").read()
    # Комментарии мешают: `// t!("…")` в документации — не строка программы.
    found = []
    for m in MACRO.finditer(src):
        line_start = src.rfind("\n", 0, m.start()) + 1
        if src[line_start : m.start()].lstrip().startswith("//"):
            continue
        kind = m.group(1)
        i = skip_ws(src, m.end())
        if kind == "tn":
            # пропустить выражение числа до запятой верхнего уровня
            depth = 0
            while i < len(src):
                c = src[i]
                if c in "([{":
                    depth += 1
                elif c in ")]}":
                    depth -= 1
                elif c == "," and depth == 0:
                    break
                elif c == '"':
                    lit = parse_literal(src, i)
                    i = lit[1] if lit else i + 1
                    continue
                i += 1
            i = skip_ws(src, i + 1)
        lit = parse_literal(src, i)
        if lit is None:
            continue  # t!(переменная) — ключ не литерал
        found.append((lit[0], kind == "tn"))
    return found


def esc(s):
    return s.replace("\\", "\\\\").replace('"', '\\"').replace("\n", "\\n").replace("\t", "\\t")


def unesc(s):
    out, i = [], 0
    while i < len(s):
        if s[i] == "\\" and i + 1 < len(s):
            out.append({"n": "\n", "t": "\t", '"': '"', "\\": "\\"}.get(s[i + 1], s[i + 1]))
            i += 2
        else:
            out.append(s[i])
            i += 1
    return "".join(out)


QUOTED = r'"((?:[^"\\]|\\.)*)"'
ENTRY = re.compile(r"^" + QUOTED + r"(?:\.([a-z]+))?\s*=\s*" + QUOTED + r"\s*(?:#.*)?$")

PLURAL_FORMS = {"ru": ["one", "few", "many"], "uk": ["one", "few", "many"], "pl": ["one", "few", "many"],
                "zh": ["other"], "ja": ["other"], "ko": ["other"]}


def read_catalog(path):
    header, entries = [], {}
    if not os.path.exists(path):
        return header, entries
    for raw in open(path, encoding="utf-8"):
        line = raw.rstrip("\n")
        s = line.strip()
        if s.startswith("@"):
            header.append(line)
            continue
        m = ENTRY.match(s)
        if m:
            entries[(unesc(m.group(1)), m.group(2))] = unesc(m.group(3))
    return header, entries


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("catalog")
    ap.add_argument("sources", nargs="+")
    ap.add_argument("--tag", default=None)
    ap.add_argument("--name", default=None)
    ap.add_argument("--check", action="store_true")
    a = ap.parse_args()

    header, old = read_catalog(a.catalog)
    if not header:
        tag = a.tag or os.path.splitext(os.path.basename(a.catalog))[0]
        header = [f'@tag = "{tag}"', f'@name = "{a.name or tag}"']
    tag = next((h.split('"')[1] for h in header if h.strip().startswith("@tag")), "en")
    forms = PLURAL_FORMS.get(tag.split("-")[0], ["one", "other"])

    files = []
    for root in a.sources:
        if os.path.isfile(root):
            files.append(root)
            continue
        for d, _, names in os.walk(root):
            if "/target" in d:
                continue
            files += [os.path.join(d, n) for n in sorted(names) if n.endswith(".rs")]
    files.sort()

    base = os.path.dirname(os.path.abspath(a.catalog))
    seen = set()
    out = [*header, ""]
    total = missing = 0
    for f in files:
        items = []
        for msgid, plural in extract(f):
            if msgid in seen or not msgid.strip():
                continue
            seen.add(msgid)
            items.append((msgid, plural))
        if not items:
            continue
        out.append(f"# {os.path.relpath(f, os.path.dirname(base))}")
        for msgid, plural in items:
            for form in forms if plural else [None]:
                value = old.get((msgid, form), "")
                total += 1
                if not value:
                    missing += 1
                suffix = f".{form}" if form else ""
                out.append(f'"{esc(msgid)}"{suffix} = "{esc(value)}"')
        out.append("")
    text = "\n".join(out).rstrip() + "\n"
    current = open(a.catalog, encoding="utf-8").read() if os.path.exists(a.catalog) else ""
    print(f"{a.catalog}: строк {total}, без перевода {missing}", file=sys.stderr)
    if a.check:
        sys.exit(1 if text != current or missing else 0)
    if text != current:
        os.makedirs(os.path.dirname(os.path.abspath(a.catalog)), exist_ok=True)
        open(a.catalog, "w", encoding="utf-8").write(text)


if __name__ == "__main__":
    main()

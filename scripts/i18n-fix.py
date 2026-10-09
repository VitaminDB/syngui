#!/usr/bin/env python3
"""Починить сборку после `i18n-wrap.py`: типовые ошибки правятся сами.

    i18n-fix.py [--cwd КАТАЛОГ] [--import КРЕЙТ] -- КОМАНДА_CARGO_BUILD...

`--import` — откуда брать макросы (по умолчанию `syngui`; в крейтах без
него — `synshell_tr`).

Цикл до 8 проходов: `cargo … --message-format=json`, по ошибкам:
- нет макроса `t`/`tn`/`n_` — `use syngui::{…};` в файл (крейты без syngui —
  `use synshell_common::n_;` не ставится: там нужно руками);
- ждали `&str`, а `t!(…)` дал `String` — `&t!(…)`;
- временное значение `&t!(…)` живёт слишком мало (`E0716`, `E0515`,
  `E0597`) или нужна `&'static str` — `n_!("…")` (перевод при показе —
  руками, файл и строка печатаются в отчёте).
Остальные ошибки печатаются как есть.
"""

import json
import re
import subprocess
import sys


def run(cmd, cwd):
    p = subprocess.run(cmd + ["--message-format=json"], cwd=cwd, capture_output=True, text=True)
    msgs = []
    for line in p.stdout.splitlines():
        try:
            m = json.loads(line)
        except json.JSONDecodeError:
            continue
        if m.get("reason") == "compiler-message" and m["message"]["level"] == "error":
            msgs.append(m["message"])
    return msgs


def offset(text, line, col):
    lines = text.split("\n")
    return sum(len(l) + 1 for l in lines[: line - 1]) + col - 1


IMPORT = "syngui"


def add_import(path, names):
    src = open(path, encoding="utf-8").read()
    have = set(re.findall(r"use syngui::\{?([^;]*)\}?;", src))
    want = sorted(names)
    line = f"use {IMPORT}::{{{', '.join(want)}}};\n" if len(want) > 1 else f"use {IMPORT}::{want[0]};\n"
    # после последнего `use` верхнего уровня, иначе после //! комментариев
    uses = list(re.finditer(r"^use [^\n]*;\n", src, re.M))
    if uses:
        pos = uses[-1].end()
    else:
        m = re.match(r"((?://![^\n]*\n)|(?:\s*\n))*", src)
        pos = m.end() if m else 0
    src = src[:pos] + line + src[pos:]
    open(path, "w", encoding="utf-8").write(src)


def main():
    args = sys.argv[1:]
    global IMPORT
    cwd = "."
    while args and args[0] in ("--cwd", "--import"):
        if args[0] == "--cwd":
            cwd = args[1]
        else:
            IMPORT = args[1]
        args = args[2:]
    if args and args[0] == "--":
        args = args[1:]
    manual = []
    for round_ in range(8):
        msgs = run(args, cwd)
        if not msgs:
            print(f"сборка чистая (проход {round_ + 1})")
            break
        imports = {}
        edits = {}  # path → [(offset, old, new)]
        rest = []
        for m in msgs:
            sp = next((s for s in m.get("spans", []) if s.get("is_primary")), None)
            # ошибка внутри макроса — место вызова (самое внешнее раскрытие)
            while sp is not None and sp.get("expansion") and ("i18n/mod.rs" in sp["file_name"] or "synshell-tr/src" in sp["file_name"]):
                sp = sp["expansion"]["span"]
            # подробность («expected `&str`, found `String`») — в метках пролётов
            text = m["message"] + " " + " ".join(s.get("label") or "" for s in m.get("spans", []))
            if sp is None:
                rest.append(text)
                continue
            path = sp["file_name"]
            if not path.startswith("/"):
                path = f"{cwd}/{path}"
            mm = re.match(r"cannot find macro `(t|tn|n_)` in this scope", text)
            if mm:
                imports.setdefault(path, set()).add(mm.group(1))
                continue
            src = open(path, encoding="utf-8").read()
            off = offset(src, sp["line_start"], sp["column_start"])
            end = offset(src, sp["line_end"], sp["column_end"])
            snippet = src[off:end]
            code = (m.get("code") or {}).get("code")
            if code == "E0308" and "expected `&str`, found `String`" in text and snippet.startswith("t!("):
                edits.setdefault(path, []).append((off, "t!(", "&t!("))
                continue
            if code == "E0308" and "expected `&'static str`, found `String`" in text and re.match(r't!\("(?:[^"\\]|\\.)*"\)$', snippet):
                edits.setdefault(path, []).append((off, "t!(", "n_!("))
                manual.append(f"{path}:{sp['line_start']}: n_! — переводить при показе")
                continue
            if code == "E0308" and "incompatible types" in m["message"]:
                # ветки `match`/`if`: где `&str` (литерал, имя, `x.y`) — `.to_string()`
                done = False
                for s2 in m.get("spans", []):
                    while s2.get("expansion") and ("i18n/mod.rs" in s2["file_name"] or "synshell-tr/src" in s2["file_name"]):
                        s2 = s2["expansion"]["span"]
                    p2 = s2["file_name"] if s2["file_name"].startswith("/") else f"{cwd}/{s2['file_name']}"
                    src2 = open(p2, encoding="utf-8").read()
                    o2 = offset(src2, s2["line_start"], s2["column_start"])
                    e2 = offset(src2, s2["line_end"], s2["column_end"])
                    snip = src2[o2:e2]
                    lab = s2.get("label") or ""
                    is_str = "&str" in lab or ("found `&str`" in text and s2.get("is_primary"))
                    if not is_str and s2.get("is_primary") and "expected `&str`" in text:
                        continue
                    if re.fullmatch(r'"(?:[^"\\]|\\.)*"|[A-Za-z_][A-Za-z0-9_.]*', snip) and (is_str or not snip.startswith("t!")):
                        if "\n" not in snip and snip not in ("true", "false"):
                            edits.setdefault(p2, []).append((o2, snip, snip + ".to_string()"))
                            done = True
                if done:
                    continue
            if code in ("E0716", "E0515", "E0597", "E0716") or "temporary value" in text:
                # найти `&t!("…")` в пролёте или рядом
                lo = src.rfind("&t!(", 0, end + 1)
                if lo >= 0 and lo >= off - 200:
                    m2 = re.match(r'&t!\(("(?:[^"\\]|\\.)*")\)', src[lo:])
                    if m2:
                        edits.setdefault(path, []).append((lo, m2.group(0), f"n_!({m2.group(1)})"))
                        manual.append(f"{path}:{src.count(chr(10), 0, lo) + 1}: n_! — переводить при показе")
                        continue
            rest.append(f"{path}:{sp['line_start']}:{sp['column_start']}: {text}")
        for path, names in imports.items():
            add_import(path, names)
        for path, es in edits.items():
            src = open(path, encoding="utf-8").read()
            for off, old, new in sorted(set(es), reverse=True):
                if src[off: off + len(old)] == old:
                    src = src[:off] + new + src[off + len(old):]
            open(path, "w", encoding="utf-8").write(src)
        print(f"проход {round_ + 1}: ошибок {len(msgs)}, импортов {len(imports)}, правок {sum(len(e) for e in edits.values())}, остальное {len(rest)}")
        if not imports and not edits:
            print("\n".join(rest))
            break
    if manual:
        print("\nПереводить при показе (n_!):")
        print("\n".join(sorted(set(manual))))


if __name__ == "__main__":
    main()

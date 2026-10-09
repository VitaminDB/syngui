#!/usr/bin/env python3
"""Обернуть строки интерфейса в `t!` (переход программы на syngui i18n).

    i18n-wrap.py ФАЙЛЫ_ИЛИ_КАТАЛОГИ... [--dry] [--re РЕГУЛЯРКА]

Строковые литералы с буквами языка исходников (по умолчанию — кириллица):
- обычный литерал → `t!("…")`;
- `format!("…{x}…", …)` → `t!("…{x}…", x = x)` (позиционные `{}` получают
  имена по выражению, спецификаторы `{:.1}` переходят в `format!` аргумента);
- `println!`/`eprintln!`/`print!`/`eprint!` → `println!("{}", t!(…))`;
- в `const`/`static` → `n_!("…")` (переводить там, где показывается: `t(x)`).

Не трогает: журнал (`log::*`, `tracing`), `panic!`/`assert!`/`unreachable!`,
`anyhow!`/`bail!`/`.context`/`.expect`, `write!`, атрибуты, шаблоны `match`
(`"…" =>`), сравнения и уже обёрнутое. После — сборка, ошибки типов (нужна
`&'static str`) правятся руками: `n_!` + `t()` при показе.
"""

import argparse
import os
import re
import sys

SKIP_CALLS = {
    "info!", "warn!", "error!", "debug!", "trace!", "log!",
    "panic!", "assert!", "assert_eq!", "assert_ne!", "debug_assert!", "debug_assert_eq!",
    "unreachable!", "todo!", "unimplemented!", "anyhow!", "bail!", "ensure!",
    "write!", "writeln!", "concat!", "include_str!", "include_bytes!", "env!", "matches!",
    "expect", "context", "with_context", "expect_err", "t!", "tn!", "n_!", "tr!", "trn!",
    "format_args!", "compile_error!", "cfg!", "Command::new", "arg", "args", "env", "var",
}
PRINT = {"println!", "eprintln!", "print!", "eprint!"}


class Tok:
    __slots__ = ("kind", "text", "start", "end", "value", "raw")

    def __init__(self, kind, text, start, end, value=None, raw=False):
        self.kind, self.text, self.start, self.end, self.value, self.raw = kind, text, start, end, value, raw

    def __repr__(self):
        return f"{self.kind}:{self.text!r}"


def lex(src):
    toks = []
    i, n = 0, len(src)
    while i < n:
        c = src[i]
        if c.isspace():
            i += 1
            continue
        if src.startswith("//", i):
            j = src.find("\n", i)
            i = n if j < 0 else j
            continue
        if src.startswith("/*", i):
            depth, j = 1, i + 2
            while j < n and depth:
                if src.startswith("/*", j):
                    depth += 1
                    j += 2
                elif src.startswith("*/", j):
                    depth -= 1
                    j += 2
                else:
                    j += 1
            i = j
            continue
        m = re.match(r'(b?r)(#*)"', src[i:])
        if m:
            hashes = m.group(2)
            end = src.find('"' + hashes, i + m.end())
            end = n if end < 0 else end + 1 + len(hashes)
            toks.append(Tok("str", src[i:end], i, end, src[i + m.end(): end - 1 - len(hashes)], raw=True))
            i = end
            continue
        if c == '"' or (c == "b" and src.startswith('b"', i)):
            j = i + (2 if c == "b" else 1)
            while j < n and src[j] != '"':
                j += 2 if src[j] == "\\" else 1
            toks.append(Tok("str", src[i:j + 1], i, j + 1, None, raw=(c == "b")))
            i = j + 1
            continue
        if c == "'":
            m = re.match(r"'(\\(?:u\{[0-9a-fA-F]+\}|x..|.)|[^\\'])'", src[i:])
            if m:
                toks.append(Tok("char", m.group(0), i, i + m.end()))
                i += m.end()
                continue
            m = re.match(r"'[A-Za-z_][A-Za-z0-9_]*", src[i:])
            toks.append(Tok("life", m.group(0) if m else "'", i, i + (m.end() if m else 1)))
            i += m.end() if m else 1
            continue
        if c.isalpha() or c == "_":
            m = re.match(r"[A-Za-z_][A-Za-z0-9_]*!?", src[i:])
            word = m.group(0)
            # `x!=y` — не макрос
            if word.endswith("!") and src[i + len(word): i + len(word) + 1] == "=":
                word = word[:-1]
            toks.append(Tok("ident", word, i, i + len(word)))
            i += len(word)
            continue
        if c.isdigit():
            m = re.match(r"[0-9][0-9a-zA-Z_.]*", src[i:])
            toks.append(Tok("num", m.group(0), i, i + m.end()))
            i += m.end()
            continue
        two = src[i:i + 2]
        if two in ("=>", "==", "!=", "::", "->", "<=", ">=", "&&", "||", "+=", "-="):
            toks.append(Tok("punct", two, i, i + 2))
            i += 2
            continue
        toks.append(Tok("punct", c, i, i + 1))
        i += 1
    return toks


def callee(toks, k):
    """Имя вызова перед `(` с индексом k: `info!`, `Text::new`, `expect`."""
    if k == 0:
        return ""
    p = toks[k - 1]
    if p.kind != "ident":
        return ""
    name = p.text
    # путь `a::b!`/`Text::new` → последняя пара сегментов
    if k >= 3 and toks[k - 2].text == "::" and toks[k - 3].kind == "ident":
        full = toks[k - 3].text + "::" + name
        if full in SKIP_CALLS:
            return full
    return name


def ancestors(toks, idx):
    """Открытые скобки над токеном idx: [(индекс скобки, символ)] от ближней к дальней."""
    out, depth = [], 0
    for k in range(idx - 1, -1, -1):
        t = toks[k].text
        if toks[k].kind != "punct":
            continue
        if t in ")]}":
            depth += 1
        elif t in "([{":
            if depth == 0:
                out.append((k, t))
            else:
                depth -= 1
    return out


def statement_head(toks, idx, src):
    """Начало оператора, в котором стоит токен (текст до 60 знаков)."""
    depth = 0
    for k in range(idx - 1, -1, -1):
        t = toks[k]
        if t.kind != "punct":
            continue
        if t.text in ")]":
            depth += 1
        elif t.text in "([":
            depth -= 1
        elif t.text == "}":
            if depth == 0:
                return src[t.end: t.end + 80].lstrip()
            depth += 1
        elif t.text == "{":
            if depth == 0:
                return src[t.end: t.end + 80].lstrip()
            depth -= 1
        elif t.text == ";" and depth == 0:
            return src[t.end: t.end + 80].lstrip()
    return src[:80]


def matching(toks, k):
    """Индекс закрывающей скобки для открывающей k."""
    depth = 0
    for j in range(k, len(toks)):
        if toks[j].kind != "punct":
            continue
        if toks[j].text in "([{":
            depth += 1
        elif toks[j].text in ")]}":
            depth -= 1
            if depth == 0:
                return j
    return len(toks) - 1


def split_args(toks, open_k, close_k, src):
    """Аргументы вызова верхнего уровня: [(текст, первый токен, последний)]."""
    args, depth, start = [], 0, open_k + 1
    for j in range(open_k + 1, close_k + 1):
        t = toks[j]
        if t.kind == "punct" and t.text in "([{":
            depth += 1
        elif t.kind == "punct" and t.text in ")]}" and j != close_k:
            depth -= 1
        if (j == close_k or (t.kind == "punct" and t.text == "," and depth == 0)):
            if start < j:
                args.append((src[toks[start].start: toks[j - 1].end].strip(), start, j - 1))
            start = j + 1
    return args


PH = re.compile(r"\{([^{}]*)\}")


def arg_name(expr, used):
    e = expr.strip()
    m = re.fullmatch(r"&?\*?([A-Za-z_][A-Za-z0-9_]*)", e)
    if m:
        base = m.group(1)
    else:
        m = re.fullmatch(r"&?[A-Za-z_][A-Za-z0-9_.]*\.([A-Za-z_][A-Za-z0-9_]*)(\(\))?", e)
        base = m.group(1) if m else "v"
        if base in ("len", "count"):
            base = "n"
        if base in ("display", "to_string", "to_string_lossy", "clone", "as_str", "unwrap_or_default"):
            m2 = re.match(r"&?([A-Za-z_][A-Za-z0-9_]*)", e)
            base = m2.group(1) if m2 else "v"
    name, k = base, 2
    while name in used:
        name = f"{base}{k}"
        k += 1
    used.add(name)
    return name


def convert_format(template, args):
    """Шаблон format! + аргументы → (шаблон t!, [(имя, выражение)]) или None."""
    if "{{" in template or "}}" in template:
        return None
    positional = [a for a in args if not re.match(r"^[A-Za-z_][A-Za-z0-9_]*\s*=[^=]", a)]
    named = {}
    for a in args:
        m = re.match(r"^([A-Za-z_][A-Za-z0-9_]*)\s*=\s*(.+)$", a, re.S)
        if m and a not in positional:
            named[m.group(1)] = m.group(2)
    used, out_args, pos = set(), [], 0
    pieces = []
    last = 0
    for m in PH.finditer(template):
        inner = m.group(1)
        name, _, spec = inner.partition(":")
        name = name.strip()
        if name == "" or name.isdigit():
            idx = pos if name == "" else int(name)
            if name == "":
                pos += 1
            if idx >= len(positional):
                return None
            expr = positional[idx]
            nm = arg_name(expr, used)
        elif re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name):
            expr = named.get(name, name)
            nm = name
            if nm in used:
                pieces.append(template[last:m.start()] + "{" + nm + "}")
                last = m.end()
                continue
            used.add(nm)
        else:
            return None
        if spec:
            expr = f'format!("{{:{spec}}}", {expr})'
        out_args.append((nm, expr))
        pieces.append(template[last:m.start()] + "{" + nm + "}")
        last = m.end()
    pieces.append(template[last:])
    return "".join(pieces), out_args


def build_t(literal_src, new_template, args):
    if new_template is None:
        body = literal_src
    else:
        body = '"' + new_template + '"'
    if not args:
        return f"t!({body})"
    return "t!(" + body + ", " + ", ".join(f"{n} = {e}" for n, e in args) + ")"


def process(path, rx, dry):
    src = open(path, encoding="utf-8").read()
    toks = lex(src)
    edits = []  # (start, end, text)
    consts = []
    covered = []  # диапазоны, уже заменённые целиком (format!)
    for i, tk in enumerate(toks):
        if tk.kind != "str" or tk.raw or not rx.search(tk.text):
            continue
        if any(a <= tk.start < b for a, b in covered):
            continue
        prev = toks[i - 1] if i else None
        nxt = toks[i + 1] if i + 1 < len(toks) else None
        if nxt is not None and nxt.text in ("=>", "==", "!=") or prev is not None and prev.text in ("==", "!=", "|"):
            continue
        anc = ancestors(toks, i)
        names = [callee(toks, k) if ch == "(" else ch for k, ch in anc]
        # атрибут #[...]
        if any(ch == "[" and k > 0 and toks[k - 1].text == "#" for k, ch in anc):
            continue
        # шаблон match с `|`: "а" | "б" =>
        if nxt is not None and nxt.text == "|":
            continue
        if any(nm in SKIP_CALLS for nm in names):
            continue
        head = statement_head(toks, i, src)
        is_const = re.match(r"(pub(\([^)]*\))?\s+)?(const|static)\s", head) is not None
        direct = names[0] if names else ""
        if direct in ("format!",) or direct in PRINT:
            k = anc[0][0]
            close = matching(toks, k)
            args = split_args(toks, k, close, src)
            if not args or args[0][1] != i or args[0][2] != i:
                continue
            template = src[tk.start + 1: tk.end - 1]
            conv = convert_format(template, [a[0] for a in args[1:]])
            if conv is None:
                print(f"{path}:{src.count(chr(10), 0, tk.start) + 1}: пропущен сложный {direct}", file=sys.stderr)
                continue
            tpl, targs = conv
            t = build_t(None, tpl, targs)
            if direct == "format!":
                start = toks[k - 1].start
                # `format!` как путь `std::format!` — редкость, не учитываем
                edits.append((start, toks[close].end, t))
                covered.append((start, toks[close].end))
            else:
                edits.append((toks[k].end, toks[close].start, '"{}", ' + t))
                covered.append((toks[k].start, toks[close].end))
            continue
        if is_const:
            edits.append((tk.start, tk.end, f"n_!({tk.text})"))
            consts.append(src.count("\n", 0, tk.start) + 1)
            continue
        edits.append((tk.start, tk.end, f"t!({tk.text})"))
    if not edits:
        return 0, consts
    edits.sort(key=lambda e: e[0])
    out, last = [], 0
    for s, e, txt in edits:
        if s < last:
            continue
        out.append(src[last:s])
        out.append(txt)
        last = e
    out.append(src[last:])
    if not dry:
        open(path, "w", encoding="utf-8").write("".join(out))
    return len(edits), consts


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("paths", nargs="+")
    ap.add_argument("--dry", action="store_true")
    ap.add_argument("--re", default="[А-Яа-яЁё]")
    a = ap.parse_args()
    rx = re.compile(a.re)
    files = []
    for p in a.paths:
        if os.path.isfile(p):
            files.append(p)
        else:
            for d, _, names in os.walk(p):
                files += [os.path.join(d, n) for n in names if n.endswith(".rs")]
    total = 0
    for f in sorted(files):
        n, consts = process(f, rx, a.dry)
        total += n
        if n:
            print(f"{f}: {n}" + (f" (n_! в константах: строки {consts[:12]}{'…' if len(consts) > 12 else ''})" if consts else ""))
    print(f"всего: {total}", file=sys.stderr)


if __name__ == "__main__":
    main()

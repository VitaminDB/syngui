# mss-lsp

LSP-сервер языка стилей MSS (syngui) поверх `mss-analysis`: ошибки и
предупреждения, дополнение, подсказки, переход к определению `var(--x)` и
`@keyframes`, образцы цветов, форматирование.

```sh
cargo build -p mss-lsp --profile fast-release     # target/fast-release/mss-lsp
install -Dm755 target/fast-release/mss-lsp ~/.local/bin/mss-lsp
```

- `mss-lsp` — сервер на stdin/stdout.
- `mss-lsp --check файл.mss…` — диагностики в терминал
  (`файл:строка:столбец: важность: сообщение [код]`), код выхода 1 при ошибках.
  Удобно для проверки после правок без редактора (и для Claude Code).

## Что сервер знает о переменных

Корень — первая папка рабочего пространства (`--check`: ближайший вверх каталог
с файлом контекста или `.git`). Переменные `--x` и имена `@keyframes` собираются
из всех `.mss` под корнем (кроме `target`, `node_modules` и скрытых каталогов);
открытый файл берётся из редактора. Сверх этого — файл контекста.

### Файл контекста

Ищется в корне и выше по дереву: `.mss-context.toml`, `mss-context.toml`,
`.mss-context.json`, `mss-context.json` (первый найденный). Все поля
необязательны, незнакомые поля — ошибка (сервер покажет сообщение).

```toml
# Классы, которые ставит программа (дополнение селекторов).
classes = ["panel", "dock", "applet"]
# @keyframes из других таблиц стилей.
keyframes = ["pulse"]

# Переменные темы: имя с «--», значение как в MSS, описание для подсказки.
[[variables]]
name = "--bg"
value = "#1e1e2e"
doc = "Фон окон"

[[variables]]
name = "--radius"
value = "8px"
```

То же в JSON:

```json
{
  "classes": ["panel", "dock"],
  "keyframes": ["pulse"],
  "variables": [
    { "name": "--bg", "value": "#1e1e2e", "doc": "Фон окон" }
  ]
}
```

Сохранение файла контекста в редакторе перечитывает его; сохранение `.mss` —
обновляет переменные этого файла.

## Kate

Настройки → LSP-клиент → «Пользовательские настройки сервера»:

```json
{
  "servers": {
    "mss": {
      "command": ["mss-lsp"],
      "rootIndicationFileNames": [".mss-context.toml", "mss-context.json", ".git"],
      "highlightingModeRegex": "^MSS$"
    }
  }
}
```

Kate выбирает сервер по режиму подсветки. Своего режима MSS у Kate нет:
либо добавьте его (копия `css.xml` в `~/.local/share/org.kde.syntax-highlighting/syntax/`
с `name="MSS"` и `extensions="*.mss"`), либо назначьте `.mss` режим CSS
(Настройки → Типы файлов) и замените регулярное выражение на `"^CSS$"` —
тогда сервер будет подключаться и к `.css`.

## Helix

`~/.config/helix/languages.toml`:

```toml
[language-server.mss-lsp]
command = "mss-lsp"

[[language]]
name = "mss"
scope = "source.mss"
file-types = ["mss"]
roots = [".mss-context.toml", "mss-context.json", ".git"]
grammar = "css"
block-comment-tokens = { start = "/*", end = "*/" }
indent = { tab-width = 4, unit = "    " }
language-servers = ["mss-lsp"]
```

## VS Code

Подойдёт любое расширение «универсальный LSP-клиент», либо своё на пять строк.
Папка `~/.vscode/extensions/mss-lsp/`:

`package.json`
```json
{
  "name": "mss-lsp", "version": "0.0.1", "publisher": "local",
  "engines": { "vscode": "^1.80.0" },
  "main": "extension.js",
  "activationEvents": ["onLanguage:mss"],
  "contributes": {
    "languages": [{ "id": "mss", "extensions": [".mss"], "aliases": ["MSS"] }]
  },
  "dependencies": { "vscode-languageclient": "^9.0.0" }
}
```

`extension.js`
```js
const { LanguageClient } = require('vscode-languageclient/node');
let client;
exports.activate = () => {
  client = new LanguageClient('mss', 'MSS', { command: 'mss-lsp' },
    { documentSelector: [{ language: 'mss' }] });
  client.start();
};
exports.deactivate = () => client && client.stop();
```

Затем `npm install` в этой папке и перезапуск VS Code. Подсветку можно взять от
CSS: `"files.associations": { "*.mss": "css" }` — но тогда `documentSelector`
поменяйте на `{ pattern: '**/*.mss' }`.

## Коды диагностик

`syntax`, `unknown-property`, `invalid-value`, `value` (цвет `#…`, неизвестная
функция, `var()` внутри `rgba()`, `var()` с запасным значением внутри составного
значения), `unknown-variable`, `unknown-pseudo`, `multiple-pseudo`,
`unknown-keyframes`, `duplicate`.

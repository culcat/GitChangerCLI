# GitChanger CLI ⚡

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![GitHub](https://img.shields.io/badge/github-culcat%2FGitChangerCLI-blue)](https://github.com/culcat/GitChangerCLI)

Быстрая, нативная консольная утилита (CLI) на **Rust** для управления несколькими Git-аккаунтами и SSH-ключами.

Основана на концепции расширения [GitSwitcher](https://github.com/culcat/GitSwitcher) для VS Code, перенесённая в терминал с максимальной производительностью, автоматическим конфигурированием `~/.ssh/config`, поддержкой `ssh-agent`, интерактивным TUI-мастером и мгновенной сменой профилей на уровне репозитория или глобально.

---

## 🚀 Возможности

- **⚡ Мгновенное переключение профилей**: Меняйте `user.name`, `user.email` и `user.signingkey` одной командой для конкретного репозитория (`--local`) или глобально (`--global`).
- **🔄 Умная перезапись Git Remote URL**:
  - При переключении на SSH: `https://github.com/owner/repo.git` ➔ `git@host-alias:owner/repo.git`.
  - При переключении на HTTPS: `git@host-alias:owner/repo.git` ➔ `https://github.com/owner/repo.git`.
- **🔑 Полная интеграция с SSH Config (`~/.ssh/config`)**:
  - Автоматическое добавление и обновление блоков `Host` с параметрами `HostName`, `User git`, `IdentityFile` и `IdentitiesOnly yes`.
  - Автоматический бэкап `~/.ssh/config.bak` перед каждой записью.
  - Проверка прав доступа приватных ключей (рекомендуемые права `0600`).
- **🛡 Интеграция с ssh-agent**:
  - Автоматический запуск агента при необходимости (`ssh-agent -s`).
  - Авто-добавление ключа в память агента при переключении (`ssh-add`).
  - Тестирование соединения с GitHub / GitLab / Bitbucket (`gitchanger test`).
- **🎨 Интерактивный TUI мастер**:
  - Пошаговое добавление профилей (`gitchanger add`).
  - Поиск и выбор профиля стрелочками / нечетким поиском (fuzzy search).
  - Авто-сканирование приватных ключей в папке `~/.ssh/`.
- **🔍 Сканирование системы (`gitchanger scan`)**:
  - Обнаружение всех приватных ключей и хостов в системе для быстрого импорта.
- **🤖 Менеджмент токенов GitHub MCP (`mcp` / `token`)**:
  - Привязка GitHub Personal Access Token (PAT) к каждому Git-профилю.
  - Автоматическая синхронизация токена в конфигурациях MCP-клиентов (Antigravity, Claude Desktop, Cursor, VS Code) при смене профиля (`gitchanger switch`).
  - Проверка валидности токенов, аккаунтов и OAuth-скоупов через официальный GitHub API (`gitchanger mcp test`).
  - Безопасное маскирование токенов в терминале (`ghp_****...****`).
- **💾 Резервное копирование (`export` / `import`)**:
  - Экспорт и импорт профилей в формате JSON или TOML для переноса между машинами.
- **🔌 Git Subcommand**:
  - Можно использовать напрямую как `git changer`!

---

## 📦 Установка

### Из исходников (требуется Rust & Cargo):

```bash
git clone https://github.com/culcat/GitChangerCLI.git
cd GitChangerCLI
cargo install --path .
```

Бинарный файл `gitchanger` будет установлен в `~/.cargo/bin/gitchanger`. Убедитесь, что `~/.cargo/bin` добавлен в вашу переменную `$PATH`:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

### Использование как подкоманды `git`:

Создайте символическую ссылку на `git-changer` в любой директории из `$PATH` (например, `~/.local/bin` или `~/.cargo/bin`):

```bash
ln -s $(which gitchanger) ~/.cargo/bin/git-changer
```

После этого утилита будет доступна как:
```bash
git changer status
git changer switch
```

---

## 🛠 Быстрый старт и использование

### 1. Интерактивный запуск

Просто запустите команду без параметров:
```bash
gitchanger
```
Если вы находитесь в Git-репозитории, отобразится текущий статус и откроется интерактивное меню выбора профиля для переключения.

---

### 2. Просмотр статуса (`status` / `current` / `whoami`)

Показывает текущее состояние репозитория, активный профиль, URL удалённых репозиториев (remotes) и ключи:

```bash
gitchanger status
```

Флаг `-v, --verbose` отображает расширенную диагностику (статус `ssh-agent`, загруженные ключи, пути к конфигурационным файлам):
```bash
gitchanger status -v
```

---

### 3. Список всех профилей (`list` / `ls`)

Красивая таблица всех настроенных профилей:

```bash
gitchanger list
```

Для просмотра полных путей к ключам и хост-алиасам используйте:
```bash
gitchanger list -v
```

---

### 4. Добавление профиля (`add`)

#### Интерактивный режим (рекомендуется):
```bash
gitchanger add
```
Мастер предложит:
1. Ввести ID профиля (например: `work`, `personal`).
2. Ввести имя автора Git (`user.name`).
3. Ввести email автора Git (`user.email`).
4. Выбрать протокол (`SSH` или `HTTPS`).
5. Для SSH: выбрать найденный SSH-ключ из `~/.ssh/` или указать свой путь, задать алиас хоста и домен (например, `github.com`).
6. Опционально указать ключ подписи коммитов (`user.signingkey`).
7. Опционально указать Personal Access Token (PAT) для GitHub MCP (`--token`).
8. Установить профиль по умолчанию.

#### Неинтерактивный режим (для скриптов):
```bash
# Добавление HTTPS-профиля
gitchanger add --id personal --name "culcat" --email "culcat@example.com" --transport https --token ghp_xxx --default --non-interactive

# Добавление SSH-профиля
gitchanger add --id work --name "Work User" --email "work@corp.com" --transport ssh --key ~/.ssh/id_ed25519_work --alias gh-work --host github.com --token ghp_yyy --non-interactive
```

---

### 5. Переключение профиля (`switch` / `use`)

#### Интерактивный выбор:
```bash
gitchanger switch
```

#### По ID или алиасу:
```bash
gitchanger switch work
```

#### Дополнительные флаги:
- `-g, --global`: применить профиль глобально в `~/.gitconfig` (по умолчанию применяется локально к текущему репозиторию).
- `--no-remote-update`: не изменять URL удалённых репозиториев (origin).
- `--no-ssh-agent`: не добавлять ключ в ssh-agent.
- `--no-mcp`: не обновлять токен в файлах конфигурации GitHub MCP.
- `-t, --test`: протестировать SSH-соединение сразу после переключения.

---

### 6. Редактирование профиля (`edit`)

```bash
gitchanger edit
# или
gitchanger edit work
```

---

### 7. Удаление профиля (`delete` / `rm`)

```bash
gitchanger delete work
```
Удаляет профиль из конфигурации и автоматически очищает соответствующий блок `Host` в `~/.ssh/config` (можно сохранить с помощью `--keep-ssh`).

---

### 8. Тестирование SSH-соединения (`test`)

Проверяет сетевое и ключевое подключение к GitHub / GitLab / Bitbucket через SSH:

```bash
gitchanger test work
```

---

### 9. Сканирование системы (`scan`)

Ищет имеющиеся ключи в `~/.ssh/` и записи в `~/.ssh/config`:

```bash
gitchanger scan
```

---

### 10. Управление токенами GitHub MCP (`mcp` / `token`)

GitChanger позволяет управлять токенами Personal Access Token (PAT) для GitHub MCP-серверов (`@modelcontextprotocol/server-github`) в **Antigravity / Gemini CLI**, **Claude Desktop**, **Cursor** и **VS Code**:

```bash
# Просмотр статуса токенов в профилях и найденных конфигурациях MCP
gitchanger mcp status

# Привязка токена к профилю (интерактивный ввод с маскированием)
gitchanger mcp set work

# Установка токена напрямую флагом
gitchanger mcp set work --token ghp_yourTokenHere

# Синхронизация токена профиля в конфигурационные файлы MCP-клиентов
gitchanger mcp sync work

# Просмотр сохранённого токена (маскированный или полный для экспорта в скрипты)
gitchanger mcp get work
gitchanger mcp get work --unmask

# Тестирование токена через GitHub API (проверка логина, лимитов и скоупов, включая 'repo')
gitchanger mcp test work

# Удаление токена из профиля (и опционально очистка из MCP-файлов)
gitchanger mcp remove work --clear-mcp
```

При переключении активного профиля (`gitchanger switch <id>`), если у целевого профиля настроен токен GitHub MCP, он **автоматически синхронизируется** во все обнаруженные конфигурационные файлы MCP-клиентов (если не указан флаг `--no-mcp`).

---

### 11. Экспорт и импорт (`export` / `import`)

```bash
# Экспорт в файл
gitchanger export backup.json
# или вывод в stdout
gitchanger export

# Импорт из файла
gitchanger import backup.json
# Перезаписать существующие профили
gitchanger import backup.json --force
```

---

## ⚙️ Конфигурация

Конфигурация хранится в стандартной XDG-директории:
`~/.config/gitchanger/config.toml`

Пример `config.toml`:

```toml
[[profiles]]
id = "personal"
name = "culcat"
email = "krainukovaleksandr1@gmail.com"
transport = "https"
ssh_host_name = "github.com"
github_token = "ghp_PersonalPAT..."
use_ssh_agent = true
is_default = true

[[profiles]]
id = "work"
name = "Aleksandr Work"
email = "aleksandr@company.com"
transport = "ssh"
ssh_host_alias = "gh-work"
ssh_host_name = "github.com"
ssh_key_path = "/home/aleksandr/.ssh/id_ed25519_work"
github_token = "ghp_WorkPAT..."
use_ssh_agent = true
is_default = false
```

---

## 🔒 Безопасность

- GitChanger **не хранит** пароли или приватные ключи SSH. Хранятся исключительно ссылки на локальные пути к ключам.
- Токены GitHub MCP в терминале **всегда маскируются** (`ghp_QFoL...BkGQ`) во всех командах (`status`, `list`, `scan`, `mcp status`), предотвращая случайную утечку на стримах или скриншотах. Полный токен можно вывести только явным флагом `--unmask`.
- При изменении `~/.ssh/config` и файлов конфигурации MCP всегда создаётся автоматический бэкап (`.bak`).
- Права доступа к `~/.ssh/config` выставляются в строгий режим `0600`.
- Запись конфигурации осуществляется атомарно через временные файлы для предотвращения повреждения данных.

---

## 📄 Лицензия

Распространяется под лицензией [MIT](LICENSE).
Автор: [culcat](https://github.com/culcat)

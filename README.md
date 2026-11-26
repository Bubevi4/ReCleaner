# Recleaner

**RU:** Windows-очиститель и игровой оптимизатор в одном `.exe` (Rust + Slint). Запуск одним кликом, с манифестом `requireAdministrator` и DPI PerMonitorV2.

**EN:** Windows cleaner and Win11-oriented gaming optimizer as a single click-to-run `.exe` (Rust + Slint), with an embedded `requireAdministrator` manifest and PerMonitorV2 DPI awareness.

<p align="center">
  <img src="docs/icon.png" alt="Recleaner icon" width="128" height="128" />
</p>

![Recleaner UI placeholder](docs/screenshot-placeholder.svg)

> Скриншот интерфейса будет добавлен позже / UI screenshot TBD.

## Скачать / Download

**[Latest Release](https://github.com/Bubevi4/ReCleaner/releases/latest)** — один `.exe`, без установщика и DLL.

| Файл / File | Архитектура / Arch |
|---|---|
| `Recleaner-windows-x64.exe` | Windows 10/11 AMD64 / Intel 64-bit |
| `Recleaner-windows-x86.exe` | Windows 32-bit (x86) |
| `Recleaner-windows-arm64.exe` | Windows 11 ARM64 |

RU: скачайте нужный файл и запустите (Windows запросит права администратора).

EN: download the matching binary and run it (Administrator UAC prompt is expected).

## Возможности / Features

| | RU | EN |
|---|---|---|
| Очистка | Правила TOML: temp, кэши браузеров/приложений/игр/GPU, корзина | Rule-driven cleanup: temps, browser/app/game/GPU caches, Recycle Bin |
| Оптимизация | Safe / Moderate / Risky твики, точка восстановления, бэкап и откат | Advanced gaming tweaks with restore point, per-tweak backup & revert |
| Автозагрузка | HKCU/HKLM Run + папки Startup + StartupApproved | Startup manager for Run keys, folders, StartupApproved |
| Языки | RU / EN через Slint `@tr()` | RU / EN via Slint `@tr()` |
| Сборка | Один статически слинкованный `recleaner.exe` | Single statically linked `recleaner.exe` |

## Сборка из исходников / Build from source

**Требования / Requirements:** Rust stable (MSVC toolchain on Windows), Windows SDK. UI uses **Slint 1.x** (resolved to 1.19 in this workspace).

```bash
# Ensure cargo is on PATH (%USERPROFILE%\.cargo\bin)
cargo build --release -p app
# Other architectures:
# cargo build --release -p app --target x86_64-pc-windows-msvc
# cargo build --release -p app --target i686-pc-windows-msvc
# cargo build --release -p app --target aarch64-pc-windows-msvc
```

Бинарник: `target/<triple>/release/recleaner.exe` (Windows subsystem — без консольного окна в release).

Иконка exe: `app/resources/icon.ico` (встраивается в PE для Проводника / панели задач).
## Запуск / Run

```bash
cargo run --release -p app
# или
.\target\release\recleaner.exe
```

Приложение запрашивает права администратора (манифест). Конфиг и логи:

`%LOCALAPPDATA%\Recleaner\` → `config.toml`, `logs\`, `backups\`.

## Безопасность / Safety notes

- Перед удалением: **Analyze**, затем **Clean**.
- Не следуем junction/symlink (reparse points); системные корни (`C:\`, `C:\Windows`, …) защищены.
- Заблокированные файлы и процессы из `close_processes` пропускаются.
- **Risky** твики (VBS/HVCI, DiagTrack, SysMain, Nagle и т.п.) требуют подтверждения и могут снизить безопасность — используйте осознанно.
- Перед первым Apply оптимизатора создаётся точка восстановления (если System Restore доступен) и JSON-бэкап твиков.

## Как добавить правило очистки / Adding a cleaning rule

См. [CONTRIBUTING.md](CONTRIBUTING.md). Кратко: добавьте `[[rule]]` в `crates/recleaner-core/rules/*.toml` и пересоберите.

## Структура / Layout

```
app/                      # UI (Slint) + binary + manifest
crates/recleaner-win/     # WinAPI wrappers
crates/recleaner-core/    # Cleanup engine + embedded rules
crates/recleaner-optimizer/ # Tweaks + startup manager
```

## Лицензия / License

[MIT](LICENSE)

## Contributing

[CONTRIBUTING.md](CONTRIBUTING.md) — стиль кода, правила, твики, CI.

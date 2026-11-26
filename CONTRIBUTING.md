# Contributing to Recleaner

Спасибо за интерес к проекту / Thanks for contributing.

## Development setup

1. Install [Rust](https://rustup.rs) stable with the **MSVC** toolchain on Windows.
2. Clone and build:

```bash
cargo build -p app
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

CI (`.github/workflows/ci.yml`) runs on `windows-latest` and uploads `recleaner.exe`.

## Project layout

| Path | Role |
|------|------|
| `app/` | Binary, Slint UI, Windows manifest / icon |
| `crates/recleaner-win/` | WinAPI wrappers (paths, admin, registry, services, restore) |
| `crates/recleaner-core/` | Cleanup engine + embedded `rules/*.toml` |
| `crates/recleaner-optimizer/` | Gaming tweaks, backups, startup manager |

## Adding a cleanup rule

1. Edit the matching file under `crates/recleaner-core/rules/` (`windows.toml`, `browsers.toml`, `apps.toml`, `games.toml`, `gpu.toml`, `dev.toml`, `privacy.toml`).
2. Append a `[[rule]]` block:

```toml
[[rule]]
id = "my-unique-id"
category = "apps"          # windows|browsers|apps|games|gpu|dev|privacy
name = { en = "My cache", ru = "Мой кэш" }
paths = ["%LOCALAPPDATA%\\MyApp\\Cache"]
include = ["**/*"]
exclude = []
min_age_hours = 24
requires_admin = false
default_enabled = true
risk = "safe"              # safe|moderate|risky
close_processes = ["MyApp"]
action = "delete_files"    # delete_files|recycle_bin|stop_service_then_delete|command
```

3. Rules are embedded at compile time (`include_str!`) — rebuild after edits.
4. Prefer env vars (`%TEMP%`, `%LOCALAPPDATA%`). Never target drive roots or `C:\Windows` itself.
5. Mark hard-to-restore paths as `risky` and usually `default_enabled = false`.
6. Extend unit tests in `crates/recleaner-core/src/safety.rs` when changing safety logic.

## Adding an optimizer tweak

Implement a `Tweak` in `crates/recleaner-optimizer/src/tweaks.rs` (`detect` / `apply` returning JSON backup / `revert`) and register it in `all_tweaks()`.

Risky security-impacting tweaks must surface a confirm dialog in the UI (already wired for `risk = "risky"`).

## Code style

- `cargo fmt`
- `cargo clippy --workspace --all-targets -- -D warnings`
- Document public APIs with `///`; modules with `//!`
- Keep WinAPI unsafe blocks minimal and commented

## License

By contributing you agree your changes are licensed under the MIT License.

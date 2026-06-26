# Filament

A polished, cross-platform **desktop app for browsing and editing your Claude Code
configuration** — agents, skills, slash commands, MCP servers, and settings — in one
place, with the correct metadata, iconography, and scope/precedence made visible.

Written in **pure Rust** ([Iced](https://iced.rs)), no web stack.

A polished, native-feeling desktop app: a translucent **"glass" UI** (with real
backdrop blur where the OS supports it — macOS vibrancy, KDE/Wayland), [Phosphor]
icons throughout, and bundled **Inter** + **JetBrains Mono** fonts.

![Filament viewing an agent](docs/screenshot-viewer.png)

[Phosphor]: https://phosphoricons.com

## Why

Claude Code config lives in scattered Markdown + YAML and JSON files across
`~/.claude/`, project `.claude/` directories, and plugins, with non-obvious
precedence rules. There's no good way to *see* it at a glance, let alone edit it
safely. Filament turns the whole config into a legible, editable dashboard.

## Features

- **Everything in one view** — agents, skills, commands, MCP servers, and
  settings, grouped in a searchable sidebar with type icons, color swatches, and
  scope chips.
- **Rich inspector** — model/effort/permission/memory badges, color-coded tool
  chips (builtin / `mcp__…` / `Agent(…)` / skill, allow vs deny), MCP transport &
  env, settings permissions and hooks, and the system prompt rendered as live
  Markdown.
- **Scope & precedence** — see which definition *wins* when names collide across
  managed / project / user / plugin scopes; shadowed entries are marked.
- **Fuzzy search & filters** — instant filtering across names, descriptions, and
  kinds.
- **Editing** — a typed form for agents (dropdowns, toggles, validation) and a
  raw source editor for every kind. Saves are **lossless**: only the fields you
  change are rewritten, so comments, key order, and unknown fields survive
  verbatim. Writes are atomic.
- **Creation wizard** — scaffold a new agent, skill, or command into the scope of
  your choice from a template.
- **Integrated terminal** — an embedded terminal panel (Alacritty engine via
  `iced_term`) so you can run `claude` for the selected agent (the **Run**
  button) or any command, without leaving the app. *(Ghostty itself can't be
  embedded in an Iced/wgpu app yet — its renderer isn't released — so the
  terminal is Alacritty-backed.)*
- **Live refresh** — external edits to your config files show up automatically,
  thanks to a debounced filesystem watcher.
- **Invalid files don't break it** — a malformed file is listed with an error
  badge and its parse error, never crashing the scan.

| Editor | Settings & hooks |
| --- | --- |
| ![Agent editor](docs/screenshot-editor.png) | ![Settings](docs/screenshot-settings.png) |

![Integrated terminal](docs/screenshot-terminal.png)

## Install / build

**Prebuilt downloads:** CI builds a macOS app bundle and a Linux tarball in
separate per-OS pipelines on every push to `main` (available as workflow
artifacts) and attaches them to the GitHub Release for any `v*` tag. The macOS
`.app` is unsigned, so the first launch needs a right-click → **Open**.

**From source** — requires Rust 1.94+ (pinned via `rust-toolchain.toml`).

```sh
cargo build --release -p filament
./target/release/filament
```

**Linux** needs the usual GUI/runtime libraries:

```sh
sudo apt-get install -y libxkbcommon-x11-0 libwayland-client0 libfontconfig1
```

(For building, the `-dev` variants: `libxkbcommon-dev libxkbcommon-x11-dev
libwayland-dev libfontconfig1-dev`.)

## Usage

```text
filament [PATH | --workspace <dir>] [--home <dir>] [--no-user]
         [--select <name>] [--search <query>]
```

- `--workspace <dir>` (or a bare path): the project to scan. Filament walks up to
  the git root collecting `.claude/` directories, reads `.mcp.json`, and merges in
  your user-level `~/.claude/`.
- `--home <dir>`: override the home directory (keeps dev/tests off your real
  `~/.claude`).
- `--no-user`: project scope only.
- `--select` / `--search`: deep-link to an item or prefill the search box.

Try it against the bundled fixtures:

```sh
cargo run -p filament -- \
  --workspace crates/filament-core/tests/fixtures/workspace_a \
  --home crates/filament-core/tests/fixtures/home
```

## Architecture

A Cargo workspace with two crates:

- **`filament-core`** — UI-free engine: the domain model, a byte-span frontmatter
  splitter, per-file parsers (errors captured as diagnostics, never panics),
  discovery + scope/precedence resolution, validation, and lossless edit / atomic
  write primitives. Fully unit-tested.
- **`filament`** — the Iced desktop app: sidebar, inspector, editor, wizard,
  theming, fuzzy search, and the file-watch subscription.

The split keeps all parsing/editing logic fast to compile and testable headlessly.

## Development

```sh
cargo test --workspace                       # unit + integration tests
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo deny check bans advisories sources     # supply-chain checks
```

CI (`.github/workflows/ci.yml`) runs fmt, clippy, tests, and a release build on
macOS, Windows, and Linux, plus `cargo-deny`.

## License

MIT — see [LICENSE](LICENSE).

Bundled fonts retain their own licenses: **Inter** and **JetBrains Mono** under
the SIL Open Font License 1.1, and **Phosphor** icons under MIT.

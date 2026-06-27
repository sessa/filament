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
- **Sessions** — a full [crow](https://github.com/radiusmethod/crow) port: pair a
  git **worktree** with a Claude Code instance per task across Board / Review /
  Ticket boards, with cross-backend GitHub/GitLab/Jira support, an automation
  suite, a manager terminal, multi-tab terminals, and a CLI control socket.
  (See [Sessions](#sessions--full-crow-parity) below.)
- **Integrated terminal** — an embedded terminal panel (Alacritty engine via
  `iced_term`) so you can run `claude` for the selected agent (the **Run**
  button) or any command, without leaving the app. *(Ghostty itself can't be
  embedded in an Iced/wgpu app yet — its renderer isn't released — so the
  terminal is Alacritty-backed.)*
- **Appearance & settings** — a dedicated **Settings** section to tune the look
  and feel: light/dark theme, an accent color (Claude coral by default), UI
  **density** (a global zoom from Compact to Spacious), the terminal font size and
  shell, and session defaults. Preferences persist in your OS data directory. The
  palette and typography are tuned to sit comfortably alongside Claude Code —
  warm, paper-and-ink neutrals rather than cold blue-grays.

  ![Settings](docs/screenshot-app-settings.png)
- **Live refresh** — external edits to your config files show up automatically,
  thanks to a debounced filesystem watcher.
- **Invalid files don't break it** — a malformed file is listed with an error
  badge and its parse error, never crashing the scan.

| Editor | Settings & hooks |
| --- | --- |
| ![Agent editor](docs/screenshot-editor.png) | ![Settings](docs/screenshot-settings.png) |

![Integrated terminal](docs/screenshot-terminal.png)

## Sessions — full crow parity

The **Sessions** section (toggle it in the header) ports the whole workflow of
[radiusmethod/crow](https://github.com/radiusmethod/crow) into Filament: instead
of juggling branches in one checkout, you spin up an isolated **git worktree** per
task, run Claude Code in it, and manage the work across three boards — a sessions
pipeline, a PR **review** board, and a project **ticket** board — with optional
automation. crow is a native macOS/Swift app built on Ghostty + tmux; Filament
brings the same model to a cross-platform Rust/Iced app (the embedded terminal is
[`iced_term`](https://crates.io/crates/iced_term) — Alacritty-backed — since
Ghostty can't be embedded in wgpu yet, and terminals run in-process rather than
via tmux).

![Sessions board](docs/screenshot-sessions.png)

**Boards (switch with the segmented control):**

- **Board** — sessions grouped **Working → In Review → Done**, plus **Paused** and
  **Archived** side groups. State is derived from the linked PR (open ⇒ In Review,
  merged ⇒ Done) and issue (closed ⇒ Done), with **positive-evidence
  auto-complete** so a session attached to an already-closed issue isn't marked
  done until real work exists. An inline **filter** narrows the list; a
  **checkbox** on each row enables **multi-select** with a bulk **delete** bar.
- **Review** — a PR triage board: sessions in review plus open PRs that don't yet
  back a session, each with **Start review** (creates a worktree on the PR's
  existing branch).
- **Tickets** — open issues grouped into project-board columns (**Backlog →
  Ready → In Progress → In Review → Done**) with a status filter; "Start working"
  turns a ticket into a session.

**Per-session detail:**

- **Run Claude / Shell** in the worktree, **Copy branch**, **Open PR**, and a
  **status** row (mark in review, pause, archive, set active).
- **Rename** inline, attach reference **links**, and a two-step delete
  confirmation that distinguishes *remove session* (keep the worktree) from
  *delete worktree*.
- **PR card** with draft / review decision, **merge readiness**
  (mergeable / conflicting / merged) and the CI check roll-up (passing / pending /
  failing).

**Cross-backend (task ≠ code):** each session records a **code backend** (GitHub
via `gh`, or GitLab via `glab`) for PRs/CI and a **task backend** (GitHub issues,
GitLab issues, or **Jira** via `acli`) for tickets — configured in Settings →
Backends, globally or per workspace, with a `branchPrefix`, self-hosted GitLab
`host` (`GITLAB_HOST`), Jira site/project, repo **exclude** lists (with `*`
wildcards), and a background **poll interval** (default 60s).

**Automation suite (Settings → Automation, off by default except auto-complete):**
auto-create a session from a labelled issue (`crow:auto`), suggest opening a PR
when a session has work but none, auto-start review when a PR becomes reviewable,
respond to change-requests / CI failures by typing a follow-up into the session
terminal, and auto-merge (squash) approved+green PRs carrying `crow:merge`.

**Manager terminal & tabs:** a persistent **Manager** Claude session (launched
`--permission-mode auto`, optionally `--rc`) for orchestration, and **multiple
terminal tabs** per session — open, switch, rename, and close them; tabs survive
navigation.

**Other:** **orphan recovery** (adopt untracked worktrees), **safe deletion**
(protected base/default branches keep their worktree; removal never `--force`s),
a first-run **setup wizard**, and a repository switcher remembered between
launches.

Provider features use the [`gh`](https://cli.github.com) / `glab` / `acli` CLIs
and are entirely optional: when they're missing or unauthenticated, sessions,
worktrees, and terminals still work — only issue/PR data is unavailable, surfaced
as a quiet hint. Worktree management uses your installed `git` (no libgit2).
Session metadata and configuration live in your OS data directory, not in the repo.

### CLI & automation socket

Like crow, a running Filament listens on a Unix socket so it can be driven from
the command line — and, via the bundled **`filament-workspace`** Claude skill
(scaffolded by `filament setup`), by Claude Code itself:

```text
filament setup [--dev-root <dir>]      # initialize config + scaffold the skill
filament list-sessions                 # JSON of all sessions
filament get-session   --session <id>
filament rename-session --session <id> <name>
filament set-status    --session <id> <active|paused|inReview|completed|archived>
filament set-ticket    --session <id> [--url <u>] [--title <t>] [--number <n>]
filament add-link      --session <id> --label <l> --url <u> [--type <kind>]
filament list-links    --session <id>
filament new-terminal  --session <id> --cwd <path> [--name <t>] [--command <c>]
filament list-terminals / close-terminal / rename-terminal …
filament send          --session <id> [--terminal <id>] <text>
filament select-session --session <id>
filament delete-session --session <id> [--keep-worktree]
```

Store-backed commands work against the on-disk session store; terminal and
focus commands signal the running app. (The socket is Unix-only.)

## Install / build

**Prebuilt downloads:** pushing a `vX.Y.Z` tag runs two separate per-OS release
pipelines that publish a macOS `Filament.app` and a Linux tarball to the matching
GitHub Release. (CI also builds release binaries on every push across all three
OSes as a check.)

When the release pipeline's Apple Developer ID secrets are configured the macOS
`.app` is signed with a hardened runtime, **notarized**, and stapled, so it opens
with a normal double-click straight from the download.

If you're running an **unsigned** build (built from source, or a release made
before notarization was set up), a browser-downloaded copy is quarantined and
Apple Silicon shows *"Filament.app is damaged and can't be opened."* The app is
fine — just remove the quarantine attribute once, then open it normally:

```sh
xattr -dr com.apple.quarantine /Applications/Filament.app
```

(The right-click → **Open** trick only clears the milder "unidentified developer"
dialog, not the "damaged" one, so use the command above.)

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
         [--select <name>] [--search <query>] [--sessions] [--settings]
```

- `--workspace <dir>` (or a bare path): the project to scan. Filament walks up to
  the git root collecting `.claude/` directories, reads `.mcp.json`, and merges in
  your user-level `~/.claude/`.
- `--home <dir>`: override the home directory (keeps dev/tests off your real
  `~/.claude`).
- `--no-user`: project scope only.
- `--select` / `--search`: deep-link to an item or prefill the search box.
- `--sessions`: open straight into the Sessions section.

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
  write primitives. Also the session engine — a `git` worktree wrapper, the
  `session` model + JSON store, `config` (cross-backend / automation settings), a
  provider-agnostic `provider` facade over `github` (`gh`), `gitlab` (`glab`) and
  `jira` (`acli`) — all gracefully degrading — the `automation` decision engine,
  and the `ipc` protocol for the CLI control socket. Fully unit-tested.
- **`filament`** — the Iced desktop app: sidebar, inspector, editor, wizard,
  theming (warm Claude-tuned palette + a small type/spacing scale), persisted
  preferences and a Settings screen, fuzzy search, the file-watch subscription,
  the embedded terminal, and the Sessions board.

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

# Running Filament & making the terminal / Claude work

A short, practical runbook: how to launch it, the **defaults**, and the one
gotcha that stops the embedded terminal from finding `claude`.

## 1. Launch

**Prebuilt:** download `Filament.app` (macOS) or the Linux tarball from the
GitHub Release and run it.

**From source** (Rust 1.94+):

```sh
cargo run --release -p filament            # opens the app
cargo run --release -p filament -- --sessions   # straight to the Sessions board
```

Linux also needs the GUI runtime libs once:

```sh
sudo apt-get install -y libxkbcommon-x11-0 libwayland-client0 libfontconfig1
```

## 2. Make the terminal & "Run Claude" work

The embedded terminal spawns real processes (`claude`, your shell, `git`). The
**only** common failure is *"Failed to spawn command 'claude': No such file or
directory."* That happens when `claude` isn't on the app's `PATH` — which is the
norm for a **Finder/dock-launched macOS app**, because GUI apps inherit a bare
`launchd` PATH (`/usr/bin:/bin:/usr/sbin:/sbin`), *not* your shell's PATH where
Homebrew/npm put `claude`.

Filament now fixes this automatically: terminals are launched with an **augmented
`PATH`** = the app's PATH + your **login shell's PATH** (so nvm/asdf/Homebrew are
honored) + well-known bins (`/opt/homebrew/bin`, `/usr/local/bin`,
`~/.local/bin`, `~/.npm-global/bin`, `~/.cargo/bin`, …).

If you're on an older build, or `claude` is somewhere unusual, use any one of:

1. **Symlink it into a standard dir** (simplest):
   ```sh
   sudo ln -sf "$(which claude)" /usr/local/bin/claude
   ```
2. **Launch the app from a terminal** so it inherits your full shell env:
   ```sh
   /Applications/Filament.app/Contents/MacOS/filament      # macOS
   ```
3. **Verify Claude is installed and on your shell PATH:**
   ```sh
   which claude && claude --version
   ```

Quick check that the terminal works at all: open a session → **Shell** → type
`echo hi`. Then **Run Claude** (or the **Manager** button).

## 3. Defaults

| Setting | Default | Where |
| --- | --- | --- |
| Code backend (PRs/CI) | **GitHub** (`gh`) | Settings → Backends |
| Task backend (issues) | **GitHub Issues** | Settings → Backends |
| Branch prefix | *(none)* — e.g. set `feature/` | Settings → Backends |
| Poll interval | **60s** (`0` = off) | Settings → Backends |
| Auto-create label | `crow:auto` | Settings → Automation |
| Auto-merge label | `crow:merge` | Settings → Automation |
| Auto-complete | **on** | Settings → Automation |
| All other automation | **off** | Settings → Automation |
| Manager permission mode | `--permission-mode auto` | Settings → Automation |
| Terminal shell | `$SHELL` (else `/bin/bash`) | Settings → Terminal |
| Terminal font size | 13 pt | Settings → Terminal |
| Theme | Dark (cycles Dark→Light→Ayu) | header toggle |

Config lives in your OS data dir (`config.json`, `sessions.json`); nothing is
written to your repos. First launch shows a one-screen **setup wizard**.

## 4. Optional provider CLIs

All optional — without them, worktrees and terminals still work; only issue/PR
data is unavailable (shown as a quiet hint):

```sh
gh auth login        # GitHub  (code + issues)
glab auth login      # GitLab  (set the host in Settings → Backends for self-hosted)
acli jira auth login # Jira    (set site + project key in Settings → Backends)
```

## 5. Driving it from the CLI / Claude

A running app listens on a Unix socket; the `filament` subcommands (and the
scaffolded `filament-workspace` skill) drive it:

```sh
filament setup                       # initialize config + scaffold the Claude skill
filament list-sessions
filament new-terminal --session <id> --cwd <path> --name work
filament send --session <id> "claude --version"   # type into that terminal
filament set-status --session <id> paused
```

Run `filament setup` once so Claude Code gets the `filament-workspace` skill and
can open worktree sessions for you from an issue URL or a plain-English task.

# 🍽️ uplate — upgrade + boilerplate

Starter repos are **easy to clone, but painful to keep alive**.

`uplate` turns any git repo into a living boilerplate. Clone it detached (no upstream), track which commit you started from, and apply upstream changes later with conservative 3-way merges.

## Why

Tools like `gitpick` clone boilerplates cleanly, but that's it — you're on your own when the template updates. `uplate` adds an upgrade path: it simulates a 3-way merge in a temp directory, so you keep your changes and get the template's changes.

- [x] Clone any git repo as a detached boilerplate
- [x] Upgrade with conservative 3-way merges
- [x] AI agents can handle conflicts — `uplate upgrade --prompt` prints a ready-to-paste resolution prompt
- [x] Works with GitHub, GitLab, any git host
- [x] Dry-run upgrades before applying
- [x] Supports monorepos with subpath selection

## Install

```sh
brew install blankeos/tap/uplate # Homebrew (macOS/Linux)
npm install -g uplate            # or npm
bun install -g uplate            # or bun
cargo binstall uplate            # or cargo-binstall (prebuilt binary, faster)
cargo install uplate             # or cargo (build from source)
curl -sSL https://raw.githubusercontent.com/uplate/uplate/main/install.sh | sh # or linux/macos (via curl)
```

> Requires git. That's it. No install needed? `npx uplate` works too.

## Quick start

```sh
# Create a project from a boilerplate
uplate create blankeos/solid-launch my-app

# Check if updates are available
cd my-app
uplate status

# Preview the upgrade
uplate upgrade --dry-run

# Apply it
uplate upgrade
```

## Commands

| Command                                       | Description                                        |
| --------------------------------------------- | -------------------------------------------------- |
| `uplate create <source> [destination]`        | Clone a boilerplate as a detached git project      |
| `uplate adopt <source> [--base <commit/tag>]` | Link an existing project to its boilerplate source |
| `uplate status`                               | Check if a template update is available            |
| `uplate upgrade [--dry-run] [--prompt]`       | Apply template changes via 3-way merge             |

## Source formats

```sh
# GitHub shorthand (owner/repo)
uplate create blankeos/solid-launch

# GitHub with subpath (for monorepos)
uplate create blankeos/solid-launch/apps/web

# Full GitHub URL with branch
uplate create https://github.com/blankeos/solid-launch/tree/dev/apps/web

# Any git host
uplate create https://gitlab.com/group/project

# GitLab with subpath
uplate create https://gitlab.com/group/project/-/tree/main/templates/vite
```

## Upgrade workflow

When you run `uplate upgrade`, it:

1. Clones the old and new template versions into a temp directory
2. Creates a 3-way merge: `yours` (your project) ← `theirs` (new template) over `base` (old template)
3. If the merge is clean, applies the resulting patch to your project
4. If conflicts arise, your working tree is untouched — use `--prompt` for an AI-agent-ready prompt

```sh
uplate upgrade               # Apply clean upgrades
uplate upgrade --dry-run     # Preview changes first
uplate upgrade --prompt      # Print a conflict prompt for coding agents
```

## How it works

uplate stores metadata in `.uplate.jsonc` at your project root, tracking the template source, base commit, and last-upgraded commit. All operations use git under the hood — no proprietary formats, no lock-in.

## Adopting existing projects

Already created a project from a template without uplate? Adopt it retroactively:

```sh
cd my-existing-project
uplate adopt blankeos/solid-launch --base 2f1e3a4
```

## License

MIT.

uplate is a wordplay on "upgrade" + "boilerplate"

This cli makes "boilerplates as git repositories" more seamless. Starter repos are **easy to clone,
but painful to keep alive**.

With uplate, you get:

- [x] Easier cloning
- [x] Easier updates

## Without uplate

1. You clone and detatch boilerplates with `npx gitpick <owner/repo/path> <your-app-name>`

- [x] Git must be detatched (it's like a fresh project)

2. 😓 No way to update

---

## With uplate

1. `uplate <owner/repo/path> <your-app-name>` - clone boilerplate
2. `uplate upgrade` - upgrade boilerplate

🙌 Super simple, right?

---

## Usage

The CLI should feel extremely small on the surface. Most users should only need to learn two commands:

```bash
uplate <owner/repo/path> <your-app-name>
uplate upgrade
```

Example:

```bash
uplate blankeos/solid-launch/apps/web my-app
cd my-app
uplate upgrade
```

### Create from boilerplate

```bash
uplate <source> <destination>
uplate create <source> <destination>
```

`uplate` and `uplate create` are the same command. The shorthand should be the nicest path, while `create` exists for people who prefer explicit subcommands.

Supported source shapes should include repo roots and optional subpaths:

```bash
uplate blankeos/solid-launch my-app
uplate blankeos/solid-launch/apps/web my-app
uplate github:blankeos/solid-launch/apps/web my-app
uplate https://gitlab.com/soulasid.litd/react-vite-tanstack-mui-boilerplate my-app
```

Default assumption for `owner/repo[/path]` is GitHub. Full git remote URLs should support non-GitHub hosts like GitLab.

Locked v1 source parser rules:

- `owner/repo` → GitHub repo root
- `owner/repo/path/to/template` → GitHub repo subdirectory
- `github:owner/repo` → explicit GitHub repo root
- `github:owner/repo/path/to/template` → explicit GitHub repo subdirectory
- `https://github.com/owner/repo` → GitHub repo root
- `https://github.com/owner/repo/tree/ref/path/to/template` → GitHub repo subdirectory at ref
- `https://gitlab.com/group/project` → GitLab/full remote repo root
- `https://gitlab.com/group/project/-/tree/ref/path/to/template` → GitLab/full remote subdirectory at ref

For shorthand GitHub sources, path starts after the first two segments:

```txt
blankeos/solid-launch/apps/web
owner = blankeos
repo = solid-launch
path = apps/web
```

For full remote URLs, store the canonical git remote URL plus optional path/ref separately.

If args are missing, `uplate` / `uplate create` should use a small cliclack-style wizard:

```bash
uplate
uplate create
```

Prompts:

```txt
? Template source repo/path
? Destination directory (.)
```

Destination defaults to `.` so users can initialize the current directory, similar to common create-app CLIs.

What it does:

- Downloads/clones only the requested boilerplate path.
- Detaches git history so the result feels like a fresh app.
- Writes root-level `.uplate.jsonc` with the source repo/path/ref and base commit/tag.
- Does **not** store a full base snapshot by default.

Potential expanded form:

```bash
uplate create <owner/repo/path> <your-app-name>
```

But shorthand should probably be preferred because it feels nicer:

```bash
uplate blankeos/solid-launch/apps/web my-app
```

### Adopt an existing gitpicked project

For projects that were already created with `gitpick` before uplate existed, there should be a way to attach uplate metadata afterward.

Possible command:

```bash
uplate init <owner/repo/path>
```

or maybe more explicit:

```bash
uplate adopt <owner/repo/path>
```

Example:

```bash
cd my-existing-app
uplate adopt blankeos/solid-launch/apps/web --base abc123
```

What it does:

- Writes `.uplate.jsonc` into the current project.
- Records the source repo/path.
- Records the best-known base commit/tag.
- Verifies that the current project looks related to that boilerplate snapshot.
- Does not modify app files.

Best case:

```bash
uplate adopt blankeos/solid-launch/apps/web --base v1.4.0
```

If the user knows the commit or tag they originally gitpicked from, adoption is straightforward.

If they do not know the original commit, uplate could support an inference mode later:

```bash
uplate adopt blankeos/solid-launch/apps/web --infer
```

Inference idea:

- Look at recent commits/tags from the boilerplate source.
- Compare snapshots against the current project.
- Pick the closest matching candidate.
- Ask for confirmation before writing `.uplate.jsonc`.

Example output:

```txt
Potential base matches:
  92%  v1.4.0 / abc123
  85%  v1.3.0 / zzz999
  61%  main@def456

Use v1.4.0 / abc123 as the base? yes/no
```

If confidence is low, uplate should not pretend to know. It should ask the user to provide a commit/tag manually.

`--infer` is deferred. Do not include it in the first implementation.

Important: adoption without a known or confidently inferred base is less safe, because uplate needs the old boilerplate snapshot for future 3-way upgrades.

This command is a good candidate for a cliclack-style wizard when args are missing, because users may not remember the exact source/base metadata.

Possible interactive flow:

```bash
uplate adopt
```

Prompts:

```txt
? Boilerplate source repo/path
? Do you know the original commit/tag?
  - Yes, I have a commit
  - Yes, I have a tag
  - No, try to infer it
? Base commit/tag
? Write .uplate.jsonc? yes/no
```

The non-interactive version should still exist for scripts and power users:

```bash
uplate adopt blankeos/solid-launch/apps/web --base abc123
```

General rule: use cliclack wizards only for commands where missing args represent real human uncertainty. Commands like `upgrade`, `status`, and `upgrade --dry-run` probably do not need wizard flows.

### Upgrade from latest boilerplate

```bash
uplate upgrade
```

What it does:

- Reads `.uplate.jsonc`.
- Ensures the working tree is clean before doing anything destructive. If there are staged or unstaged changes, stop.
- Fetches the old boilerplate version from the saved base commit.
- Fetches the latest boilerplate version from the configured source ref.
- Performs a 3-way upgrade merge in a temp directory before touching the user's project:
  - base = old boilerplate snapshot
  - ours = current user project
  - theirs = latest boilerplate snapshot
- Applies clean changes automatically.
- If conflicts are detected, stops and explains them without writing conflict markers into the app.
- Updates `.uplate.jsonc` only after a successful upgrade.

No package-manager-specific behavior in v1. `package.json`, `Cargo.toml`, lockfiles, etc. should just be treated like normal files in the merge.

### Preview upgrade

```bash
uplate upgrade --dry-run
```

Should show what would happen without changing files:

```txt
Upgrade available: abc123 -> def456

Auto-mergeable:
  src/middleware.ts
  .env.example

Needs review:
  src/auth.ts

Deleted upstream:
  src/old-auth.ts
```

### Generate AI prompt

```bash
uplate upgrade --prompt
```

Should print or write a prompt that includes:

- the project's `.uplate.jsonc`
- the old boilerplate commit
- the latest boilerplate commit
- the upstream diff
- conflicted files
- user's current file contents where relevant
- clear instructions for the coding agent

This is the escape hatch for difficult upgrades.

### Status

```bash
uplate status
```

Should answer:

- What boilerplate is this project tracking?
- What commit was last applied?
- Is a newer upstream commit available?
- Is the local working tree clean?
- Is the saved base commit still available upstream?

Example output:

```txt
Source: blankeos/solid-launch/apps/web
Current template commit: abc123
Latest template commit: def456
Working tree: clean
Base commit: available

Run `uplate upgrade --dry-run` to preview changes.
```

### `.uplate.jsonc` schema

Root-level `.uplate.jsonc` is the only default project metadata file.

Suggested v1 schema:

```jsonc
{
  "schemaVersion": 1,
  "source": {
    "type": "github", // github | git
    "input": "blankeos/solid-launch/apps/web",
    "remote": "https://github.com/blankeos/solid-launch.git",
    "owner": "blankeos",
    "repo": "solid-launch",
    "path": "apps/web",
    "ref": "main",
  },
  "base": {
    "commit": "abc123",
    "ref": "main",
    "tag": null,
  },
  "current": {
    "commit": "abc123",
    "upgradedAt": null,
  },
  "createdAt": "2026-06-27T00:00:00Z",
}
```

Notes:

- `source.input` preserves what the user typed.
- `source.remote` is the canonical git remote used for fetching.
- `source.path` is optional and can be `null`/empty for repo-root templates.
- `source.ref` is the ref to follow on upgrade. If created from `main`, keep following `main`. If created from another branch, keep following that branch.
- `base.commit` is the old template commit needed for 3-way merge.
- `base.tag` is optional fallback metadata if the base came from a tag.
- `current.commit` is the last successfully applied template commit.
- Update `current.commit` only after a successful clean upgrade.
- No full base snapshot is stored by default.

### Repair / missing base commit

If the saved base commit no longer exists upstream, uplate should not attempt a normal automatic upgrade.

Possible reasons this can happen:

- the boilerplate repo was force-pushed
- the commit was amended away
- the branch history was rebased
- the source branch was deleted/recreated
- the repo was made private or inaccessible
- the requested subdirectory moved or stopped existing

Before giving up, uplate can try fallback refs if they were saved in `.uplate.jsonc`, especially tags. For example, if the project was created from a stable tag like `v1.4.0`, uplate can try to resolve that tag and verify it points to the expected old snapshot.

Useful metadata to save:

```jsonc
{
  "source": "blankeos/solid-launch/apps/web",
  "baseCommit": "abc123",
  "baseRef": "main",
  "baseTag": "v1.4.0",
}
```

Tag fallback should only be trusted if it resolves to the same commit or an equivalent verified snapshot. If a tag moved, uplate should warn instead of silently trusting it.

Example:

```txt
Cannot safely upgrade.

The saved base commit abc123 no longer exists upstream:
  blankeos/solid-launch/apps/web

This can happen if the boilerplate history was amended, rebased, force-pushed,
deleted/recreated, made inaccessible, or otherwise changed.

uplate needs that old boilerplate version to perform a safe 3-way merge.
```

Possible escape hatches later:

```bash
uplate upgrade --prompt
uplate repair --base <path-to-old-template>
```

Default stance: missing base commit is rare enough to be okay, but should produce a clear warning/error instead of doing something risky.

### Optional portability later

By default, uplate should **not** store the full base snapshot in the user's project because it can be large and annoying for tools like Biome/formatters.

So default project metadata should be just:

```txt
.uplate.jsonc
```

Could maybe support opt-in later:

```bash
uplate pin
uplate unpin
```

Where `uplate pin` creates a `.uplate/` folder and stores a compressed base snapshot for teams/repos that want maximum durability. Not v1-critical.

## Under the hood

1. `uplate <owner/repo/path> <your-app-name>`
   - uses gitpick - clones the repo, detatches git like a fresh project.
   - we save a minor marker of the hash of when it was last cloned into `.uplate.jsonc`
2. `uplate upgrade`
   - actually very hard to do, but it's very optimistic:
     - 1. Never execute if there are unstaged/staged changes. diff must be empty.
     - 2. It reads the `.uplate.jsonc`
     - 3. Checks the diff between that PR and the very latest version of your thing. Runs a diff algo.
     - 4. If no "dangerous" changes, it'll merge seamlessly. If merge conflicts are detected, it will just give a prompt session on what to keep and not to keep.
   - Do not add special package-manager behavior in v1. Files like package.json, Cargo.toml, and lockfiles are merged like any other file.
   - 4. In the worst case that you have no idea how to fix it at all, let AI do it!
     - Just tell your coding agent `uplate upgrade --prompt` it'll essentially print prompt of what to do + a diff so the ai agent can help you fix the conflicts without thinking about it at all. No skills needed.
   - After any update, must always update `.uplate.jsonc`

### 3-way merge model

The upgrade should not be a blind copy of the latest boilerplate over the user app.

Instead, the model is:

```txt
        D  latest boilerplate
       /
A ----
       \
        U  user's current project
```

Where:

- `A` = boilerplate snapshot at the last applied commit
- `D` = latest boilerplate snapshot
- `U` = user's current project

uplate should merge:

```txt
merge(base=A, ours=U, theirs=D)
```

This lets uplate distinguish between:

- changes made by the boilerplate author
- changes made by the app developer
- overlapping changes that need human/AI review

If the user and boilerplate changed different parts of a file, the upgrade can apply cleanly. If both changed the same lines or same semantic area differently, uplate should stop and explain the conflict.

### Implementation direction

Build the CLI in Rust.

Reasons:

- all my CLIs are already Rust
- good fit for filesystem-heavy work
- good fit for diffing, hashing, temp dirs, archives, and git orchestration
- fast startup for commands like `uplate status`
- standalone binary distribution

Initial implementation can still use the real `git` binary for hard git/merge behavior instead of implementing everything from scratch.

Potential install paths later:

```bash
cargo install uplate
brew install uplate
npx uplate ... # possible thin npm wrapper around the Rust binary
```

### Suggested command surface

Need to keep judging this carefully because usage is the main product decision.

Likely v1:

```bash
uplate <source> <destination>
uplate create <source> <destination>
uplate # interactive create wizard
uplate create # interactive create wizard
uplate adopt <source> --base <commit-or-tag>
uplate adopt # interactive adopt wizard
uplate upgrade
uplate upgrade --dry-run
uplate upgrade --prompt
uplate status
```

Maybe later:

```bash
uplate adopt <owner/repo/path> --infer
uplate repair --base <path>
uplate pin
uplate unpin
```

## Inspirations

- https://github.com/kellyselden/boilerplate-update

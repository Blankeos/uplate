uplate is a wordplay on "upgrade" + "boilerplate"

This cli makes "boilerplates as git repositories" more seamless.

- [x] Easy cloning
- [x] Easy updates

## Without uplate

1. You clone and detatch boilerplates with `npx gitpick <owner/repo/path> <your-app-name>`

- [x] Git must be detatched (it's like a fresh project)

2. 😓 No way to update

---

## With uplate

1. `uplate <owner/repo/path> <your-app-name>` - clone boilerplate
2. `uplate upgrade` - upgrade boilerplate

🙌 Super simple, right?

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
   - Also has a few optimistic rules on what's safe to merge:
     - package.json, Cargo.toml, etc. versions.
   - 4. In the worst case that you have no idea how to fix it at all, let AI do it!
     - Just tell your coding agent `uplate upgrade --prompt` it'll essentially print prompt of what to do + a diff so the ai agent can help you fix the conflicts without thinking about it at all. No skills needed.
   - After any update, must always update `.uplate.jsonc`

## Inspirations

- https://github.com/kellyselden/boilerplate-update

default:
    just --list

# Run the CLI in development.
dev *args:
    cargo run -- {{ args }}

# Build release artifacts with cargo-dist.
dist-build *args:
    dist build {{ args }}

# Keep npm package docs aligned with the root README.
sync_readme:
    cp README.md npm/README.md

# Release: bump versions, regenerate changelog, commit, tag, and push.
# Usage: just tag [patch|minor|major]
tag bump="":
    sh scripts/tag_and_release.sh {{ bump }}

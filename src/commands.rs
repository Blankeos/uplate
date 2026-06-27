use crate::{
    cli::{Cli, Command, UpgradeArgs},
    config::{BaseConfig, CurrentConfig, UplateConfig, CONFIG_FILE},
    git, snapshot,
    source::{self, ParsedSource},
};
use anyhow::{bail, Context, Result};
use chrono::Utc;
use cliclack::{input, intro, outro};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

pub fn run(cli: Cli) -> Result<()> {
    git::ensure_git_available().context("uplate requires git for v1, but git was not found")?;

    match cli.command {
        Some(Command::Create(args)) => create(args.source, args.destination),
        Some(Command::Adopt(args)) => adopt(args.source, args.base),
        Some(Command::Status(_)) => status(),
        Some(Command::Upgrade(args)) => upgrade(args),
        None => create(cli.source, cli.destination),
    }
}

fn create(source_arg: Option<String>, destination_arg: Option<PathBuf>) -> Result<()> {
    let (source_input, destination) = prompt_create_args(source_arg, destination_arg)?;
    let parsed = source::parse_source(&source_input)?;
    let ref_name = resolve_follow_ref(&parsed)?;
    let snapshot = snapshot::materialize_source(&parsed, &ref_name)?;

    ensure_create_destination_is_safe(&destination)?;
    snapshot::copy_template_contents(&snapshot.template_dir, &destination)?;

    if !git::is_git_repo(&destination) {
        git::init_repo(&destination)?;
    }

    let config = UplateConfig {
        schema_version: 1,
        source: parsed.into_config(ref_name.clone()),
        base: BaseConfig {
            commit: snapshot.commit.clone(),
            ref_name,
            tag: None,
        },
        current: CurrentConfig {
            commit: snapshot.commit.clone(),
            upgraded_at: None,
        },
        created_at: Utc::now(),
    };
    config.write_to(&destination)?;

    println!("Created {} from {}", destination.display(), source_input);
    println!("Base template commit: {}", short_commit(&snapshot.commit));
    println!("Review the generated files, then commit them with git.");
    Ok(())
}

fn adopt(source_arg: Option<String>, base_arg: Option<String>) -> Result<()> {
    let (source_input, base_input) = prompt_adopt_args(source_arg, base_arg)?;
    let project_root = std::env::current_dir()?;
    if project_root.join(CONFIG_FILE).exists() {
        bail!(
            "{} already exists. This project is already adopted by uplate.",
            CONFIG_FILE
        );
    }
    git::ensure_clean_worktree(&project_root)?;

    let parsed = source::parse_source(&source_input)?;
    let ref_name = resolve_follow_ref(&parsed)?;
    let base_snapshot = snapshot::materialize_source(&parsed, &base_input)
        .with_context(|| missing_base_message(&base_input, &parsed.remote))?;
    snapshot::verify_adoption_fit(&project_root, &base_snapshot.template_dir)?;
    let tag = resolve_tag_if_exact(&base_snapshot.repo_dir, &base_input, &base_snapshot.commit);

    let config = UplateConfig {
        schema_version: 1,
        source: parsed.into_config(ref_name.clone()),
        base: BaseConfig {
            commit: base_snapshot.commit.clone(),
            ref_name,
            tag,
        },
        current: CurrentConfig {
            commit: base_snapshot.commit.clone(),
            upgraded_at: None,
        },
        created_at: Utc::now(),
    };
    config.write_to(&project_root)?;

    println!("Adopted project with source {}", source_input);
    println!(
        "Base template commit: {}",
        short_commit(&base_snapshot.commit)
    );
    Ok(())
}

fn status() -> Result<()> {
    let project_root = std::env::current_dir()?;
    let config = UplateConfig::read_from(&project_root)?;
    let latest = git::latest_remote_commit(&config.source.remote, &config.source.ref_name)?;

    let tree_clean = git::is_worktree_clean(&project_root)?;
    let base_available = git::commit_exists_in_remote(&config.source.remote, &config.base.commit)?;

    println!("Source: {}", config.source.input);
    println!("Remote: {}", config.source.remote);
    if let Some(path) = &config.source.path {
        println!("Path: {path}");
    }
    println!("Following ref: {}", config.source.ref_name);
    println!("Current template: {}", short_commit(&config.current.commit));
    println!("Latest template:  {}", short_commit(&latest));
    println!(
        "Working tree: {}",
        if tree_clean { "clean" } else { "dirty" }
    );
    println!(
        "Base commit: {}",
        if base_available {
            "available"
        } else {
            "missing"
        }
    );

    if latest == config.current.commit {
        println!("Status: up to date");
    } else {
        println!("Status: update available");
        if tree_clean {
            println!("Run `uplate upgrade --dry-run` to preview changes.");
        } else {
            println!("Working tree is dirty, commit or stash before upgrade.");
        }
    }
    Ok(())
}

fn upgrade(args: UpgradeArgs) -> Result<()> {
    let project_root = std::env::current_dir()?;
    git::ensure_clean_worktree(&project_root)?;
    let mut config = UplateConfig::read_from(&project_root)?;
    let previous_commit = config.current.commit.clone();
    let latest = git::latest_remote_commit(&config.source.remote, &config.source.ref_name)?;

    if latest == config.current.commit {
        println!("Already up to date at {}", short_commit(&latest));
        return Ok(());
    }

    let simulation = match simulate_upgrade(&project_root, &config, &latest) {
        Ok(simulation) => simulation,
        Err(error) if args.prompt => {
            print_agent_prompt(&config, &latest, &error.to_string());
            return Err(error);
        }
        Err(error) => return Err(error),
    };

    if simulation.patch.trim().is_empty() {
        println!("No file changes needed, but updating uplate metadata.");
    } else if args.dry_run {
        println!(
            "Upgrade preview: {} -> {}",
            short_commit(&config.current.commit),
            short_commit(&latest)
        );
        if simulation.diff_stat.trim().is_empty() {
            println!("No diff stat available.");
        } else {
            println!("{}", simulation.diff_stat);
        }
        return Ok(());
    }

    if args.dry_run {
        return Ok(());
    }

    if !simulation.patch.trim().is_empty() {
        git::ensure_clean_worktree(&project_root)
            .context("Working tree changed during simulation. Retry the upgrade.")?;
        git::run_with_input(
            Some(&project_root),
            ["apply", "--binary", "--whitespace=nowarn", "-"],
            &simulation.patch,
        )
        .context("failed to apply clean simulated upgrade patch")?;
    }

    config.current.commit = latest.clone();
    config.base.commit = latest.clone();
    config.current.upgraded_at = Some(Utc::now());
    config.write_to(&project_root)?;

    println!(
        "Upgraded template {} -> {}",
        short_commit(&previous_commit),
        short_commit(&latest)
    );
    println!("Review the changes, then commit them with git.");
    Ok(())
}

struct UpgradeSimulation {
    patch: String,
    diff_stat: String,
}

fn simulate_upgrade(
    project_root: &Path,
    config: &UplateConfig,
    latest: &str,
) -> Result<UpgradeSimulation> {
    let base_snapshot = snapshot::materialize_config_source(
        &config.source.remote,
        config.source.path.as_deref(),
        &config.current.commit,
    )
    .with_context(|| missing_base_message(&config.current.commit, &config.source.remote))?;

    let latest_snapshot = snapshot::materialize_config_source(
        &config.source.remote,
        config.source.path.as_deref(),
        latest,
    )?;

    let temp = TempDir::new()?;
    let merge_repo = temp.path().join("merge");
    fs::create_dir(&merge_repo)?;
    git::init_repo(&merge_repo)?;

    snapshot::copy_template_contents(&base_snapshot.template_dir, &merge_repo)?;
    git::add_all(&merge_repo)?;
    git::commit_all(&merge_repo, "uplate base")?;
    let base_commit = git::rev_parse(&merge_repo, "HEAD")?;

    git::run(Some(&merge_repo), ["checkout", "--quiet", "-b", "ours"])?;
    snapshot::replace_with_git_tracked_project(project_root, &merge_repo)?;
    git::add_all(&merge_repo)?;
    git::commit_all(&merge_repo, "uplate ours")?;

    git::run(
        Some(&merge_repo),
        ["checkout", "--quiet", "-b", "theirs", &base_commit],
    )?;
    snapshot::replace_with_template(&latest_snapshot.template_dir, &merge_repo)?;
    git::add_all(&merge_repo)?;
    git::commit_all(&merge_repo, "uplate theirs")?;

    git::run(Some(&merge_repo), ["checkout", "--quiet", "ours"])?;
    let (merge_ok, merge_out) = git::run_allow_failure(
        Some(&merge_repo),
        ["merge", "--no-commit", "--no-ff", "theirs"],
    )?;
    if !merge_ok {
        bail!(
            "Cannot safely upgrade.\n\nThe upgrade was simulated in a temporary merge and conflicts were detected. Your working tree was left untouched.\n\n{}\n\nRun `uplate upgrade --prompt` for a coding-agent prompt, or resolve manually by comparing your project against the latest boilerplate.",
            merge_out.stderr
        );
    }

    let diff_stat = git::run_raw(Some(&merge_repo), ["diff", "--stat", "HEAD"])?.stdout;
    let patch = git::run_raw(Some(&merge_repo), ["diff", "--binary", "HEAD"])?.stdout;
    Ok(UpgradeSimulation { patch, diff_stat })
}

fn print_agent_prompt(config: &UplateConfig, latest: &str, error: &str) {
    println!(
        "\n--- uplate upgrade prompt ---\n\
I'm upgrading a project that tracks a detached boilerplate with uplate.\n\n\
Source input: {}\n\
Remote: {}\n\
Subpath: {}\n\
Previous template commit: {}\n\
Latest template commit: {}\n\n\
uplate tried to simulate a conservative 3-way merge in a temporary directory before touching the project, but it could not apply the upgrade cleanly. The user's working tree was left untouched.\n\n\
Error:\n{}\n\n\
Please help manually apply the upstream boilerplate changes from the previous template commit to the latest template commit, preserving user modifications. Do not write conflict markers unless explicitly needed, and do not commit changes. Update .uplate.jsonc current.commit to the latest template commit only if the upgrade is fully resolved.\n\
--- end prompt ---",
        config.source.input,
        config.source.remote,
        config.source.path.as_deref().unwrap_or("."),
        config.current.commit,
        latest,
        error
    );
}

fn prompt_create_args(
    source_arg: Option<String>,
    destination_arg: Option<PathBuf>,
) -> Result<(String, PathBuf)> {
    if source_arg.is_some() && destination_arg.is_some() {
        return Ok((source_arg.unwrap(), destination_arg.unwrap()));
    }

    intro("Create from boilerplate")?;
    let source = match source_arg {
        Some(value) => value,
        None => input("Template source repo/path")
            .placeholder("owner/repo/path or https://gitlab.com/group/project")
            .interact()?,
    };
    let destination = match destination_arg {
        Some(value) => value,
        None => {
            let value: String = input("Destination directory")
                .placeholder(".")
                .default_input(".")
                .interact()?;
            PathBuf::from(value)
        }
    };
    outro("Ready to create project ✓")?;
    Ok((source, destination))
}

fn prompt_adopt_args(
    source_arg: Option<String>,
    base_arg: Option<String>,
) -> Result<(String, String)> {
    if source_arg.is_some() && base_arg.is_some() {
        return Ok((source_arg.unwrap(), base_arg.unwrap()));
    }

    intro("Adopt existing project")?;
    let source = match source_arg {
        Some(value) => value,
        None => input("Template source repo/path")
            .placeholder("owner/repo/path or https://gitlab.com/group/project")
            .interact()?,
    };
    let base = match base_arg {
        Some(value) => value,
        None => input("Original boilerplate base commit or tag")
            .placeholder("abc123 or v1.4.0")
            .interact()?,
    };
    outro("Ready to adopt project ✓")?;
    Ok((source, base))
}

fn resolve_follow_ref(parsed: &ParsedSource) -> Result<String> {
    match &parsed.ref_name {
        Some(value) => Ok(value.clone()),
        None => git::default_branch(&parsed.remote),
    }
}

fn ensure_create_destination_is_safe(destination: &Path) -> Result<()> {
    if destination.exists() {
        if !destination.is_dir() {
            bail!(
                "destination exists and is not a directory: {}",
                destination.display()
            );
        }
        let mut entries = fs::read_dir(destination)
            .with_context(|| format!("failed to read {}", destination.display()))?;
        if entries.next().transpose()?.is_some() {
            bail!(
                "destination directory is not empty: {}. Choose an empty directory.",
                destination.display()
            );
        }
    } else {
        fs::create_dir_all(destination)
            .with_context(|| format!("failed to create {}", destination.display()))?;
    }
    Ok(())
}

fn resolve_tag_if_exact(repo: &Path, input: &str, commit: &str) -> Option<String> {
    // Only attempt tag resolution when input looks like a tag name, not a commit hash.
    // Git commit hashes are 7-40 hex chars; tags typically have broader naming.
    if input.len() >= 7 && input.len() <= 40 && input.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let tag_ref = format!("refs/tags/{input}^{{commit}}");
    let tag_commit = git::rev_parse_verify(repo, &tag_ref)
        .or_else(|_| git::rev_parse_verify(repo, &format!("refs/tags/{input}")))
        .ok()?;
    if tag_commit == commit {
        Some(input.to_string())
    } else {
        None
    }
}

fn missing_base_message(reference: &str, remote: &str) -> String {
    format!(
        "Cannot safely upgrade.\n\nThe saved base commit {reference} no longer exists upstream ({remote}).\n\nThis can happen if the boilerplate history was amended, rebased, force-pushed, deleted/recreated, made inaccessible, or otherwise changed.\n\nuplate needs that old boilerplate version to perform a safe 3-way merge."
    )
}

fn short_commit(commit: &str) -> String {
    commit.chars().take(12).collect()
}

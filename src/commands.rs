use crate::{
    cli::{Cli, Command, UpgradeArgs},
    config::{BaseConfig, CurrentConfig, UplateConfig, CONFIG_FILE},
    git, snapshot,
    source::{self, ParsedSource},
};
use anyhow::{bail, Context, Result};
use chrono::Utc;
use cliclack::{input, intro, log, note, outro, spinner};
use console::style;
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

    let progress = spinner();
    progress.start("Creating project...");

    let result = (|| -> Result<String> {
        progress.set_message("Fetching template...");
        let snapshot = snapshot::materialize_source(&parsed, &ref_name)?;

        ensure_create_destination_is_safe(&destination)?;
        progress.set_message("Copying files...");
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

        Ok(snapshot.commit)
    })();

    match result {
        Ok(commit) => {
            progress.stop("Project created");
            print_create_success(&destination, &source_input, &commit)?;
            Ok(())
        }
        Err(err) => {
            progress.error("Creating project failed");
            Err(err)
        }
    }
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

    println!(
        "{} {}",
        style("Source:").dim(),
        style(&config.source.input).cyan()
    );
    println!(
        "{} {}",
        style("Remote:").dim(),
        style(&config.source.remote).dim()
    );
    if let Some(path) = &config.source.path {
        println!("{} {}", style("Path:").dim(), style(path).cyan());
    }
    println!(
        "{} {}",
        style("Following ref:").dim(),
        style(&config.source.ref_name).cyan()
    );
    println!(
        "{} {}",
        style("Current template:").dim(),
        style(short_commit(&config.current.commit)).cyan()
    );
    println!(
        "{} {}",
        style("Latest template:").dim(),
        style(short_commit(&latest)).cyan()
    );
    println!(
        "{} {}",
        style("Working tree:").dim(),
        if tree_clean {
            style("clean").green()
        } else {
            style("dirty").yellow()
        }
    );
    println!(
        "{} {}",
        style("Base commit:").dim(),
        if base_available {
            style("available").green()
        } else {
            style("missing").red()
        }
    );

    if latest == config.current.commit {
        println!("{} {}", style("Status:").dim(), style("up to date").green());
    } else {
        println!(
            "{} {}",
            style("Status:").dim(),
            style("update available").yellow()
        );
        if tree_clean {
            println!(
                "{}",
                style("Run `uplate upgrade --dry-run` to preview changes.").dim()
            );
        } else {
            println!(
                "{}",
                style("Working tree is dirty, commit or stash before upgrade.").yellow()
            );
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
    let base_tree = git::write_tree(&merge_repo)?;

    snapshot::replace_with_git_tracked_project(project_root, &merge_repo)?;
    git::add_all(&merge_repo)?;
    let ours_tree = git::write_tree(&merge_repo)?;

    snapshot::replace_with_template(&latest_snapshot.template_dir, &merge_repo)?;
    git::add_all(&merge_repo)?;
    let theirs_tree = git::write_tree(&merge_repo)?;

    let (merge_ok, merge_out) =
        git::merge_trees(&merge_repo, &base_tree, &ours_tree, &theirs_tree)?;
    if !merge_ok {
        bail!(
            "Cannot safely upgrade.\n\nThe upgrade was simulated in a temporary merge and conflicts were detected. Your working tree was left untouched.\n\n{}\n\nRun `uplate upgrade --prompt` for a coding-agent prompt, or resolve manually by comparing your project against the latest boilerplate.",
            merge_conflict_output(&merge_out)
        );
    }
    let merged_tree = merge_out.stdout.trim();

    let diff_stat = git::run_raw(
        Some(&merge_repo),
        ["diff", "--stat", &ours_tree, merged_tree],
    )?
    .stdout;
    let patch = git::run_raw(
        Some(&merge_repo),
        ["diff", "--binary", &ours_tree, merged_tree],
    )?
    .stdout;
    Ok(UpgradeSimulation { patch, diff_stat })
}

fn merge_conflict_output(output: &git::GitOutput) -> String {
    let details = format!("{}{}", output.stdout, output.stderr);
    if details.trim().is_empty() {
        "git merge-tree reported conflicts without additional details.".to_string()
    } else {
        details
    }
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
    if let Some(source) = source_arg.as_deref() {
        source::validate_source_shape(source)?;
    }

    if source_arg.is_some() && destination_arg.is_some() {
        return Ok((source_arg.unwrap(), destination_arg.unwrap()));
    }

    intro("Create from boilerplate")?;
    let source = match source_arg {
        Some(value) => {
            log::info(format!("Template source: {}", style(&value).cyan()))?;
            value
        }
        None => input("Template source repo/path")
            .placeholder("owner/repo/path or https://gitlab.com/group/project")
            .validate(|value: &String| {
                source::validate_source_shape(value).map_err(|err| err.to_string())
            })
            .interact()?,
    };
    let destination = match destination_arg {
        Some(value) => value,
        None => {
            let value: String = input("Destination directory")
                .placeholder(". (here)")
                .default_input(".")
                .interact()?;
            PathBuf::from(value)
        }
    };
    Ok((source, destination))
}

fn prompt_adopt_args(
    source_arg: Option<String>,
    base_arg: Option<String>,
) -> Result<(String, String)> {
    if let Some(source) = source_arg.as_deref() {
        source::validate_source_shape(source)?;
    }

    if source_arg.is_some() && base_arg.is_some() {
        return Ok((source_arg.unwrap(), base_arg.unwrap()));
    }

    intro("Adopt existing project")?;
    let source = match source_arg {
        Some(value) => {
            log::info(format!("Template source: {}", style(&value).cyan()))?;
            value
        }
        None => input("Template source repo/path")
            .placeholder("owner/repo/path or https://gitlab.com/group/project")
            .validate(|value: &String| {
                source::validate_source_shape(value).map_err(|err| err.to_string())
            })
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

fn print_create_success(destination: &Path, source_input: &str, commit: &str) -> Result<()> {
    let headline = format!(
        "Created {} from {}",
        style(destination.display()).cyan().bold(),
        style(source_input).green(),
    );
    log::step(headline).context("failed to render create summary")?;

    let details = format!(
        "{}\n{}",
        style(format!("Base template commit: {}", short_commit(commit))).dim(),
        style("Review the generated files, then commit them with git.").dim(),
    );
    note("", details).context("failed to render create details")?;
    Ok(())
}

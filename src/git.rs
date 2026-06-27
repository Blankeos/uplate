use anyhow::{bail, Context, Result};
use std::{
    ffi::OsStr,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

#[derive(Debug, Clone)]
pub struct GitOutput {
    pub stdout: String,
    pub stderr: String,
}

pub fn run_raw<I, S>(cwd: Option<&Path>, args: I) -> Result<GitOutput>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new("git");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command.args(args);
    let rendered = render_command(&command);
    let output = command
        .output()
        .with_context(|| format!("failed to run {rendered}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        bail!("{rendered} failed\n{stderr}");
    }
    Ok(GitOutput { stdout, stderr })
}

pub fn add_all(repo: &Path) -> Result<()> {
    run(Some(repo), ["add", "-A"])?;
    Ok(())
}

pub fn run_with_input<I, S>(cwd: Option<&Path>, args: I, input: &str) -> Result<GitOutput>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new("git");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let rendered = render_command(&command);
    let mut child = command
        .spawn()
        .with_context(|| format!("failed to run {rendered}"))?;
    child
        .stdin
        .as_mut()
        .context("failed to open git stdin")?
        .write_all(input.as_bytes())?;
    let output = child
        .wait_with_output()
        .with_context(|| format!("failed to wait for {rendered}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        bail!("{rendered} failed\n{stderr}");
    }
    Ok(GitOutput { stdout, stderr })
}

pub fn ensure_git_available() -> Result<()> {
    run(None, ["--version"])?;
    Ok(())
}

pub fn run<I, S>(cwd: Option<&Path>, args: I) -> Result<GitOutput>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new("git");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command.args(args);
    let rendered = render_command(&command);
    let output = command
        .output()
        .with_context(|| format!("failed to run {rendered}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        bail!("{rendered} failed\n{stderr}");
    }
    Ok(GitOutput { stdout, stderr })
}

pub fn run_allow_failure<I, S>(cwd: Option<&Path>, args: I) -> Result<(bool, GitOutput)>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new("git");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command.args(args);
    let output = command.output().context("failed to run git")?;
    Ok((
        output.status.success(),
        GitOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        },
    ))
}

pub fn clone_repo(remote: &str, destination: &Path) -> Result<()> {
    run(
        None,
        [
            OsStr::new("clone"),
            OsStr::new("--quiet"),
            OsStr::new(remote),
            destination.as_os_str(),
        ],
    )?;
    Ok(())
}

pub fn checkout(repo: &Path, reference: &str) -> Result<()> {
    let (ok, _) = run_allow_failure(Some(repo), ["checkout", "--quiet", reference])?;
    if ok {
        return Ok(());
    }
    // Reference not found locally — fetch it then try again
    run(
        Some(repo),
        ["fetch", "--quiet", "--depth", "1", "origin", reference],
    )
    .with_context(|| format!("failed to fetch ref {reference}"))?;
    run(Some(repo), ["checkout", "--quiet", reference])
        .with_context(|| format!("failed to check out {reference} after fetch"))?;
    Ok(())
}

pub fn rev_parse(repo: &Path, reference: &str) -> Result<String> {
    Ok(run(Some(repo), ["rev-parse", reference])?
        .stdout
        .trim()
        .to_string())
}

pub fn rev_parse_verify(repo: &Path, reference: &str) -> Result<String> {
    Ok(run(Some(repo), ["rev-parse", "--verify", reference])?
        .stdout
        .trim()
        .to_string())
}

pub fn write_tree(repo: &Path) -> Result<String> {
    Ok(run(Some(repo), ["write-tree"])?.stdout.trim().to_string())
}

pub fn merge_trees(repo: &Path, base: &str, ours: &str, theirs: &str) -> Result<(bool, GitOutput)> {
    run_allow_failure(
        Some(repo),
        [
            "merge-tree",
            "--write-tree",
            "--merge-base",
            base,
            ours,
            theirs,
        ],
    )
}

pub fn default_branch(remote: &str) -> Result<String> {
    let out = run(None, ["ls-remote", "--symref", remote, "HEAD"])?;
    for line in out.stdout.lines() {
        if let Some(rest) = line.strip_prefix("ref: refs/heads/") {
            if let Some((branch, _)) = rest.split_once('\t') {
                return Ok(branch.to_string());
            }
        }
    }
    Ok("HEAD".to_string())
}

pub fn latest_remote_commit(remote: &str, ref_name: &str) -> Result<String> {
    let out = run(None, ["ls-remote", remote, ref_name])?;
    let Some(line) = out.stdout.lines().next() else {
        bail!("could not find ref {ref_name} in {remote}");
    };
    let Some((commit, _)) = line.split_once('\t') else {
        bail!("unexpected ls-remote output for {ref_name}: {line}");
    };
    Ok(commit.to_string())
}

pub fn is_git_repo(path: &Path) -> bool {
    run_allow_failure(Some(path), ["rev-parse", "--is-inside-work-tree"])
        .map(|(ok, out)| ok && out.stdout.trim() == "true")
        .unwrap_or(false)
}

pub fn init_repo(path: &Path) -> Result<()> {
    run(Some(path), ["init", "--quiet"])?;
    Ok(())
}

pub fn is_worktree_clean(path: &Path) -> Result<bool> {
    if !is_git_repo(path) {
        return Ok(false);
    }
    Ok(run(Some(path), ["status", "--porcelain"])?
        .stdout
        .is_empty())
}

pub fn commit_exists_in_remote(remote: &str, commit: &str) -> Result<bool> {
    let temp = tempfile::tempdir().context("failed to create temporary git probe")?;
    run(Some(temp.path()), ["init", "--quiet"])?;
    let (ok, _) = run_allow_failure(
        Some(temp.path()),
        ["fetch", "--quiet", "--depth", "1", remote, commit],
    )?;
    Ok(ok)
}

pub fn ensure_clean_worktree(path: &Path) -> Result<()> {
    if !is_git_repo(path) {
        bail!(
            "{} is not a git repository. uplate requires git for v1.",
            path.display()
        );
    }
    let out = run(Some(path), ["status", "--porcelain"])?;
    if !out.stdout.is_empty() {
        bail!(
            "working tree is not clean. Commit, stash, or discard your changes before running uplate."
        );
    }
    Ok(())
}

fn render_command(command: &Command) -> String {
    let mut parts = vec![command.get_program().to_string_lossy().to_string()];
    parts.extend(
        command
            .get_args()
            .map(|arg| arg.to_string_lossy().to_string()),
    );
    parts.join(" ")
}

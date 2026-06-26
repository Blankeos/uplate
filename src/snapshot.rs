use crate::{git, source::ParsedSource};
use anyhow::{bail, Context, Result};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;
use walkdir::WalkDir;

#[derive(Debug)]
pub struct MaterializedSnapshot {
    _temp_dir: TempDir,
    pub repo_dir: PathBuf,
    pub template_dir: PathBuf,
    pub commit: String,
}

fn ensure_no_submodules(source: &Path) -> Result<()> {
    if source.join(".gitmodules").exists() {
        bail!(
            "git submodules are not supported by uplate v1: {} contains .gitmodules",
            source.display()
        );
    }
    Ok(())
}

fn validate_relative_template_path(path: &str) -> Result<()> {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        bail!("template path must be relative: {path}");
    }
    for component in candidate.components() {
        match component {
            std::path::Component::Normal(_) => {}
            _ => bail!("invalid template path: {path}"),
        }
    }
    Ok(())
}

pub fn materialize_source(source: &ParsedSource, reference: &str) -> Result<MaterializedSnapshot> {
    materialize_remote(&source.remote, source.path.as_deref(), reference)
}

pub fn materialize_config_source(
    remote: &str,
    path: Option<&str>,
    reference: &str,
) -> Result<MaterializedSnapshot> {
    materialize_remote(remote, path, reference)
}

fn materialize_remote(
    remote: &str,
    path: Option<&str>,
    reference: &str,
) -> Result<MaterializedSnapshot> {
    let temp_dir = tempfile::tempdir().context("failed to create temporary directory")?;
    let repo_dir = temp_dir.path().join("repo");
    git::clone_repo(remote, &repo_dir).with_context(|| format!("failed to clone {remote}"))?;
    git::checkout(&repo_dir, reference)
        .with_context(|| format!("failed to check out {reference}"))?;
    let commit = git::rev_parse(&repo_dir, "HEAD")?;
    let template_dir = path
        .filter(|subpath| !subpath.trim().is_empty())
        .map(|subpath| {
            validate_relative_template_path(subpath)?;
            Ok::<PathBuf, anyhow::Error>(repo_dir.join(subpath))
        })
        .transpose()?
        .unwrap_or_else(|| repo_dir.clone());
    if !template_dir.exists() {
        bail!(
            "template path {} does not exist at {commit}",
            path.unwrap_or(".")
        );
    }
    if !template_dir.is_dir() {
        bail!("template path {} is not a directory", path.unwrap_or("."));
    }
    Ok(MaterializedSnapshot {
        _temp_dir: temp_dir,
        repo_dir,
        template_dir,
        commit,
    })
}

pub fn copy_template_contents(source: &Path, destination: &Path) -> Result<()> {
    copy_dir_contents_with_filter(source, destination, should_skip_template)
}

pub fn replace_with_template(source: &Path, destination: &Path) -> Result<()> {
    clear_dir_except_git(destination)?;
    copy_template_contents(source, destination)
}

pub fn replace_with_git_tracked_project(project_root: &Path, destination: &Path) -> Result<()> {
    clear_dir_except_git(destination)?;
    copy_dir_contents_with_filter(project_root, destination, should_skip_project)
}

fn copy_dir_contents_with_filter(
    source: &Path,
    destination: &Path,
    should_skip: fn(&Path) -> bool,
) -> Result<()> {
    ensure_no_submodules(source)?;
    fs::create_dir_all(destination)
        .with_context(|| format!("failed to create {}", destination.display()))?;

    for entry in WalkDir::new(source).min_depth(1) {
        let entry = entry?;
        let path = entry.path();
        let rel = path.strip_prefix(source)?;
        if should_skip(rel) {
            continue;
        }
        let dest_path = destination.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&dest_path)
                .with_context(|| format!("failed to create {}", dest_path.display()))?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(path, &dest_path).with_context(|| {
                format!(
                    "failed to copy {} to {}",
                    path.display(),
                    dest_path.display()
                )
            })?;
            let permissions = fs::metadata(path)?.permissions();
            fs::set_permissions(&dest_path, permissions)?;
        } else if entry.file_type().is_symlink() {
            copy_symlink(path, &dest_path)?;
        }
    }
    Ok(())
}

fn clear_dir_except_git(path: &Path) -> Result<()> {
    if !path.exists() {
        fs::create_dir_all(path)?;
        return Ok(());
    }
    for entry in fs::read_dir(path).with_context(|| format!("failed to read {}", path.display()))? {
        let entry = entry?;
        let file_name = entry.file_name();
        if file_name == ".git" {
            continue;
        }
        let entry_path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() && !file_type.is_symlink() {
            fs::remove_dir_all(&entry_path)
                .with_context(|| format!("failed to remove {}", entry_path.display()))?;
        } else {
            fs::remove_file(&entry_path)
                .with_context(|| format!("failed to remove {}", entry_path.display()))?;
        }
    }
    Ok(())
}

fn should_skip_template(relative: &Path) -> bool {
    relative.components().any(|component| {
        let value = component.as_os_str().to_string_lossy();
        value == ".git"
    })
}

fn should_skip_project(relative: &Path) -> bool {
    relative.components().any(|component| {
        let value = component.as_os_str().to_string_lossy();
        value == ".git" || value == ".uplate"
    })
}

#[cfg(unix)]
fn copy_symlink(source: &Path, destination: &Path) -> Result<()> {
    use std::os::unix::fs as unix_fs;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let target = fs::read_link(source)?;
    if target.is_absolute() {
        bail!(
            "refusing to copy absolute symlink {} -> {}",
            source.display(),
            target.display()
        );
    }
    unix_fs::symlink(target, destination)
        .with_context(|| format!("failed to copy symlink {}", source.display()))
}

#[cfg(windows)]
fn copy_symlink(source: &Path, destination: &Path) -> Result<()> {
    use std::os::windows::fs as windows_fs;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let target = fs::read_link(source)?;
    if target.is_absolute() {
        bail!(
            "refusing to copy absolute symlink {} -> {}",
            source.display(),
            target.display()
        );
    }
    if source.is_dir() {
        windows_fs::symlink_dir(target, destination)?;
    } else {
        windows_fs::symlink_file(target, destination)?;
    }
    Ok(())
}
/// Returns the set of filenames and dirnames at the top level of `dir`.
pub fn top_level_names(dir: &Path) -> Result<HashSet<String>> {
    let mut names = HashSet::new();
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().to_string();
        if file_name != ".git" && file_name != ".uplate" && file_name != ".uplate.jsonc" {
            names.insert(file_name);
        }
    }
    Ok(names)
}

/// Verify that the user project's top-level files overlap with the boilerplate template.
/// Returns an error if there is no overlap, suggesting the project may be unrelated.
pub fn verify_adoption_fit(project_root: &Path, template_dir: &Path) -> Result<()> {
    let project_names = top_level_names(project_root)?;
    let template_names = top_level_names(template_dir)?;
    let intersection: HashSet<_> = project_names.intersection(&template_names).collect();
    if intersection.is_empty() {
        bail!(
            "The project at {} does not appear to be related to the boilerplate source.\n\
             No common top-level files or directories were found.\n\
             If this is a match, use `--base` with the exact commit or tag.",
            project_root.display()
        );
    }
    Ok(())
}

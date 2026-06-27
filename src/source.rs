use crate::config::{SourceConfig, SourceType};
use anyhow::{bail, Context, Result};
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSource {
    pub input: String,
    pub kind: SourceType,
    pub remote: String,
    pub owner: Option<String>,
    pub repo: Option<String>,
    pub path: Option<String>,
    pub ref_name: Option<String>,
}

pub fn validate_source_shape(input: &str) -> Result<()> {
    parse_source(input).map(|_| ())
}

fn reject_raw_path_traversal(input: &str) -> Result<()> {
    if input == "."
        || input == ".."
        || input.contains("/../")
        || input.contains("/./")
        || input.ends_with("/..")
        || input.ends_with("/.")
        || input.starts_with("../")
        || input.starts_with("./")
    {
        bail!("source path must not contain . or .. segments");
    }
    Ok(())
}

impl ParsedSource {
    pub fn into_config(self, ref_name: String) -> SourceConfig {
        SourceConfig {
            kind: self.kind,
            input: self.input,
            remote: self.remote,
            owner: self.owner,
            repo: self.repo,
            path: self.path,
            ref_name,
        }
    }
}

pub fn parse_source(input: &str) -> Result<ParsedSource> {
    let input = input.trim();
    if input.is_empty() {
        bail!("source cannot be empty");
    }
    reject_raw_path_traversal(input)?;

    if let Some(rest) = input.strip_prefix("github:") {
        return parse_github_shorthand(input, rest);
    }

    if input.starts_with("http://") || input.starts_with("https://") {
        return parse_url_source(input);
    }

    if input.contains("://") {
        bail!("unsupported source URL scheme: {input}");
    }

    parse_github_shorthand(input, input)
}

fn parse_github_shorthand(original: &str, value: &str) -> Result<ParsedSource> {
    let parts = split_path(value);
    if parts.len() < 2 {
        bail!(
            "{original} is not a valid template source. Expected owner/repo, owner/repo/path, github:owner/repo/path, or a git URL."
        );
    }
    let owner = parts[0].to_string();
    let repo = trim_git_suffix(parts[1]).to_string();
    validate_segment("owner", &owner)?;
    validate_segment("repo", &repo)?;
    let path = join_optional_path(&parts[2..])?;

    Ok(ParsedSource {
        input: original.to_string(),
        kind: SourceType::Github,
        remote: format!("https://github.com/{owner}/{repo}.git"),
        owner: Some(owner),
        repo: Some(repo),
        path,
        ref_name: None,
    })
}

fn parse_url_source(input: &str) -> Result<ParsedSource> {
    let url = Url::parse(input).with_context(|| format!("invalid source URL: {input}"))?;
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    match host.as_str() {
        "github.com" => parse_github_url(input, &url),
        _ => parse_generic_git_url(input, &url),
    }
}

fn parse_github_url(input: &str, url: &Url) -> Result<ParsedSource> {
    let parts = split_path(url.path().trim_start_matches('/'));
    if parts.len() < 2 {
        bail!("GitHub URL must include owner and repo: {input}");
    }

    let owner = parts[0].to_string();
    let repo = trim_git_suffix(parts[1]).to_string();
    validate_segment("owner", &owner)?;
    validate_segment("repo", &repo)?;

    let mut ref_name = None;
    let mut path = None;

    if parts.len() > 2 {
        if parts[2] != "tree" {
            bail!("GitHub URL subpaths must use /tree/<ref>/<path>: {input}");
        }
        if parts.len() < 4 {
            bail!("GitHub tree URL must include a ref: {input}");
        }
        ref_name = Some(parts[3].to_string());
        path = join_optional_path(&parts[4..])?;
    }

    Ok(ParsedSource {
        input: input.to_string(),
        kind: SourceType::Github,
        remote: format!("https://github.com/{owner}/{repo}.git"),
        owner: Some(owner),
        repo: Some(repo),
        path,
        ref_name,
    })
}

fn parse_generic_git_url(input: &str, url: &Url) -> Result<ParsedSource> {
    let parts = split_path(url.path().trim_start_matches('/'));
    if parts.is_empty() {
        bail!("git URL must include a repository path: {input}");
    }

    if let Some(tree_index) = parts.iter().position(|part| *part == "-") {
        if parts.get(tree_index + 1) != Some(&"tree") {
            bail!("unsupported GitLab-style URL shape: {input}");
        }
        if tree_index == 0 {
            bail!("git URL must include a repository path before /-/tree: {input}");
        }
        let ref_name = parts
            .get(tree_index + 2)
            .map(|value| value.to_string())
            .context("GitLab-style tree URL must include a ref")?;
        let repo_path = parts[..tree_index].join("/");
        let subpath = join_optional_path(&parts[(tree_index + 3)..])?;
        return Ok(ParsedSource {
            input: input.to_string(),
            kind: SourceType::Git,
            remote: canonical_url_remote(url, &repo_path),
            owner: None,
            repo: Some(trim_git_suffix(parts[tree_index - 1]).to_string()),
            path: subpath,
            ref_name: Some(ref_name),
        });
    }

    Ok(ParsedSource {
        input: input.to_string(),
        kind: SourceType::Git,
        remote: canonical_url_remote(url, &parts.join("/")),
        owner: None,
        repo: Some(trim_git_suffix(parts.last().expect("non-empty parts")).to_string()),
        path: None,
        ref_name: None,
    })
}

fn canonical_url_remote(url: &Url, repo_path: &str) -> String {
    let mut remote = url.clone();
    remote.set_path(&format!("/{}", trim_git_suffix(repo_path)));
    remote.set_query(None);
    remote.set_fragment(None);
    // Most git hosts accept the URL without .git suffix, but
    // append it for consistency unless the original already had it.
    let remote_str = remote.to_string();
    if remote_str.ends_with(".git") {
        remote_str
    } else {
        format!("{remote_str}.git")
    }
}

fn split_path(value: &str) -> Vec<&str> {
    value
        .split('/')
        .filter(|part| !part.trim().is_empty())
        .collect()
}

fn join_optional_path(parts: &[&str]) -> Result<Option<String>> {
    if parts.is_empty() {
        Ok(None)
    } else {
        for part in parts {
            if part.is_empty() || *part == "." || *part == ".." || part.contains('\\') {
                bail!("invalid template path segment: {part}");
            }
        }
        Ok(Some(parts.join("/")))
    }
}

fn trim_git_suffix(value: &str) -> &str {
    value.strip_suffix(".git").unwrap_or(value)
}

fn validate_segment(name: &str, value: &str) -> Result<()> {
    if value.is_empty() || value == "." || value == ".." || value.contains('\\') {
        bail!("invalid {name} segment: {value}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_github_shorthand_root() {
        let parsed = parse_source("blankeos/solid-launch").unwrap();
        assert_eq!(
            parsed.remote,
            "https://github.com/blankeos/solid-launch.git"
        );
        assert_eq!(parsed.path, None);
    }

    #[test]
    fn parses_github_shorthand_subpath() {
        let parsed = parse_source("blankeos/solid-launch/apps/web").unwrap();
        assert_eq!(parsed.owner.as_deref(), Some("blankeos"));
        assert_eq!(parsed.repo.as_deref(), Some("solid-launch"));
        assert_eq!(parsed.path.as_deref(), Some("apps/web"));
    }

    #[test]
    fn parses_github_prefix() {
        let parsed = parse_source("github:blankeos/solid-launch/apps/web").unwrap();
        assert_eq!(
            parsed.remote,
            "https://github.com/blankeos/solid-launch.git"
        );
        assert_eq!(parsed.path.as_deref(), Some("apps/web"));
    }

    #[test]
    fn parses_github_tree_url() {
        let parsed =
            parse_source("https://github.com/blankeos/solid-launch/tree/dev/apps/web").unwrap();
        assert_eq!(parsed.ref_name.as_deref(), Some("dev"));
        assert_eq!(parsed.path.as_deref(), Some("apps/web"));
    }

    #[test]
    fn parses_gitlab_root_url() {
        let parsed =
            parse_source("https://gitlab.com/soulasid.litd/react-vite-tanstack-mui-boilerplate")
                .unwrap();
        assert_eq!(
            parsed.remote,
            "https://gitlab.com/soulasid.litd/react-vite-tanstack-mui-boilerplate.git"
        );
        assert_eq!(parsed.path, None);
    }

    #[test]
    fn parses_gitlab_tree_url() {
        let parsed =
            parse_source("https://gitlab.com/group/project/-/tree/main/templates/app").unwrap();
        assert_eq!(parsed.remote, "https://gitlab.com/group/project.git");
        assert_eq!(parsed.ref_name.as_deref(), Some("main"));
        assert_eq!(parsed.path.as_deref(), Some("templates/app"));
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(parse_source("owner/repo/../secret").is_err());
        assert!(parse_source("https://github.com/owner/repo/tree/main/../secret").is_err());
        assert!(parse_source("https://gitlab.com/group/project/-/tree/main/../secret").is_err());
    }

    #[test]
    fn rejects_single_word_source() {
        let error = parse_source("adot").unwrap_err().to_string();
        assert!(error.contains("adot is not a valid template source"));
    }
}

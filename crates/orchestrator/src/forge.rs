//! Forge: the GitHub-facing seam. A `Forge` trait (so logic is testable with a
//! fake) plus the idempotent `open_or_reuse`. The real client (octocrab) lives
//! in `GithubForge`; network access is confined to this module (ADR-0008).

use async_trait::async_trait;
use octocrab::Octocrab;

use crate::incident::{Incident, issue_body_with_marker};

/// A GitHub repository, `owner/name`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoId {
    pub owner: String,
    pub name: String,
}

impl RepoId {
    /// Parse `owner/name` from a git remote URL (ssh or https GitHub forms).
    pub fn parse_remote(url: &str) -> Option<RepoId> {
        let s = url.trim();
        let rest = s
            .strip_prefix("git@github.com:")
            .or_else(|| s.strip_prefix("https://github.com/"))
            .or_else(|| s.strip_prefix("http://github.com/"))?;
        let rest = rest.strip_suffix(".git").unwrap_or(rest);
        let mut parts = rest.split('/');
        let owner = parts.next()?.to_string();
        let name = parts.next()?.to_string();
        if owner.is_empty() || name.is_empty() {
            return None;
        }
        Some(RepoId { owner, name })
    }
}

/// A reference to a created/found issue.
#[derive(Debug, Clone)]
pub struct IssueRef {
    pub number: u64,
    pub url: String,
}

/// A forge operation failure (network, auth, API).
#[derive(Debug)]
pub struct ForgeError(pub String);

impl std::fmt::Display for ForgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "forge error: {}", self.0)
    }
}

impl std::error::Error for ForgeError {}

/// The GitHub-facing operations the loop needs. Async; implemented by
/// `GithubForge` (real) and a fake in tests.
#[async_trait]
pub trait Forge {
    /// Find an OPEN issue whose body contains `marker` (the fingerprint).
    async fn find_open_issue_by_marker(
        &self,
        repo: &RepoId,
        marker: &str,
    ) -> Result<Option<IssueRef>, ForgeError>;

    /// Create an issue and return its reference.
    async fn create_issue(
        &self,
        repo: &RepoId,
        title: &str,
        body: &str,
        labels: &[String],
    ) -> Result<IssueRef, ForgeError>;
}

/// Idempotently open an issue for `inc`: reuse an open issue carrying the same
/// fingerprint marker, otherwise create one. Returns `(issue, created)`.
pub async fn open_or_reuse<F: Forge + ?Sized>(
    forge: &F,
    repo: &RepoId,
    inc: &Incident,
    labels: &[String],
) -> Result<(IssueRef, bool), ForgeError> {
    if let Some(existing) = forge
        .find_open_issue_by_marker(repo, &inc.fingerprint)
        .await?
    {
        return Ok((existing, false));
    }
    let body = issue_body_with_marker(&inc.body, &inc.fingerprint);
    let issue = forge.create_issue(repo, &inc.title, &body, labels).await?;
    Ok((issue, true))
}

/// The real forge: GitHub via octocrab. Network access lives only here.
pub struct GithubForge {
    client: Octocrab,
}

impl GithubForge {
    /// Build from a token in `GITHUB_TOKEN` or `GH_TOKEN` (never from `gh`).
    pub fn from_env() -> Result<Self, ForgeError> {
        let token = std::env::var("GITHUB_TOKEN")
            .or_else(|_| std::env::var("GH_TOKEN"))
            .map_err(|_| {
                ForgeError(
                    "set GITHUB_TOKEN or GH_TOKEN (locally: `export GITHUB_TOKEN=$(gh auth token)`)"
                        .to_string(),
                )
            })?;
        let client = Octocrab::builder()
            .personal_token(token)
            .build()
            .map_err(|e| ForgeError(format!("octocrab build: {e}")))?;
        Ok(Self { client })
    }
}

#[async_trait]
impl Forge for GithubForge {
    async fn find_open_issue_by_marker(
        &self,
        repo: &RepoId,
        marker: &str,
    ) -> Result<Option<IssueRef>, ForgeError> {
        // NOTE: GitHub search is eventually consistent (indexing lag), so this
        // dedups across separate runs, not concurrent double-submits.
        let query = format!(
            "repo:{}/{} is:issue is:open {}",
            repo.owner, repo.name, marker
        );
        let page = self
            .client
            .search()
            .issues_and_pull_requests(&query)
            .send()
            .await
            .map_err(|e| ForgeError(format!("search issues: {e}")))?;
        Ok(page.items.into_iter().next().map(|i| IssueRef {
            number: i.number,
            url: i.html_url.to_string(),
        }))
    }

    async fn create_issue(
        &self,
        repo: &RepoId,
        title: &str,
        body: &str,
        labels: &[String],
    ) -> Result<IssueRef, ForgeError> {
        let issue = self
            .client
            .issues(&repo.owner, &repo.name)
            .create(title)
            .body::<String>(Some(body.to_string()))
            .labels(labels.to_vec())
            .send()
            .await
            .map_err(|e| ForgeError(format!("create issue: {e}")))?;
        Ok(IssueRef {
            number: issue.number,
            url: issue.html_url.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// In-memory forge: stores (body, ref) and matches a marker by substring.
    struct FakeForge {
        issues: Mutex<Vec<(String, IssueRef)>>,
        next: Mutex<u64>,
    }

    impl FakeForge {
        fn new() -> Self {
            Self {
                issues: Mutex::new(Vec::new()),
                next: Mutex::new(1),
            }
        }
    }

    #[async_trait]
    impl Forge for FakeForge {
        async fn find_open_issue_by_marker(
            &self,
            _repo: &RepoId,
            marker: &str,
        ) -> Result<Option<IssueRef>, ForgeError> {
            let issues = self.issues.lock().unwrap();
            Ok(issues
                .iter()
                .find(|(body, _)| body.contains(marker))
                .map(|(_, r)| r.clone()))
        }

        async fn create_issue(
            &self,
            _repo: &RepoId,
            _title: &str,
            body: &str,
            _labels: &[String],
        ) -> Result<IssueRef, ForgeError> {
            let mut next = self.next.lock().unwrap();
            let number = *next;
            *next += 1;
            let issue = IssueRef {
                number,
                url: format!("https://example.test/issues/{number}"),
            };
            self.issues
                .lock()
                .unwrap()
                .push((body.to_string(), issue.clone()));
            Ok(issue)
        }
    }

    fn repo() -> RepoId {
        RepoId {
            owner: "o".into(),
            name: "n".into(),
        }
    }

    #[test]
    fn parse_remote_ssh_and_https() {
        let a = RepoId::parse_remote("git@github.com:constantin-jais/Agent-O-Matic.git").unwrap();
        assert_eq!(
            a,
            RepoId {
                owner: "constantin-jais".into(),
                name: "Agent-O-Matic".into()
            }
        );
        let b = RepoId::parse_remote("https://github.com/constantin-jais/Agent-O-Matic").unwrap();
        assert_eq!(b.name, "Agent-O-Matic");
        assert!(RepoId::parse_remote("ftp://nope").is_none());
    }

    #[tokio::test]
    async fn open_or_reuse_creates_then_reuses() {
        let forge = FakeForge::new();
        let inc = Incident::new("gate-red", "high", "T", "B", "k", 1);

        let (first, created1) = open_or_reuse(&forge, &repo(), &inc, &[]).await.unwrap();
        assert!(created1, "first call creates");

        let (second, created2) = open_or_reuse(&forge, &repo(), &inc, &[]).await.unwrap();
        assert!(!created2, "second call reuses");
        assert_eq!(first.number, second.number);
        assert_eq!(forge.issues.lock().unwrap().len(), 1, "no duplicate issue");
    }

    #[tokio::test]
    async fn distinct_fingerprints_create_distinct_issues() {
        let forge = FakeForge::new();
        let a = Incident::new("gate-red", "high", "A", "B", "k1", 1);
        let b = Incident::new("gate-red", "high", "A", "B", "k2", 1);
        let (_, c1) = open_or_reuse(&forge, &repo(), &a, &[]).await.unwrap();
        let (_, c2) = open_or_reuse(&forge, &repo(), &b, &[]).await.unwrap();
        assert!(c1 && c2);
        assert_eq!(forge.issues.lock().unwrap().len(), 2);
    }
}

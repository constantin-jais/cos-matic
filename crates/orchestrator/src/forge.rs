//! Forge: the GitHub-facing seam. A `Forge` trait (so logic is testable with a
//! fake) plus the idempotent `open_or_reuse`. The real client (octocrab) lives
//! in `GithubForge`; **all** network access — issues, PRs, check status, merge —
//! is confined to this module (ADR: orchestrator-consolidated-on-octocrab,
//! ADR: github-via-octocrab).

use async_trait::async_trait;
use octocrab::Octocrab;
use octocrab::params::repos::Commitish;

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

/// A reference to a created/found pull request.
#[derive(Debug, Clone)]
pub struct PrRef {
    pub number: u64,
    pub url: String,
}

/// How to merge a PR. Only rebase today; extensible (squash/merge) without
/// touching call sites.
#[derive(Debug, Clone, Copy)]
pub enum MergeStrategy {
    Rebase,
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

/// Map a check run's `conclusion` to the gate's bucket. A run with no conclusion
/// has not completed (queued / in-progress) -> `pending`; `success`/`neutral`
/// pass; `skipped` is a final, non-blocking state; anything else (`failure`,
/// `cancelled`, `timed_out`, …) fails. This is the Rust home of the mapping the
/// gate's old `gh ... --jq` filter used to do.
fn check_bucket(conclusion: Option<&str>) -> &'static str {
    match conclusion {
        None => "pending",
        Some("success") | Some("neutral") => "pass",
        Some("skipped") => "skipping",
        Some(_) => "fail",
    }
}

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

    /// List the branch's check runs, pre-classified into the buckets the gate's
    /// `classify` understands (`pass`/`fail`/`pending`/`skipping`).
    async fn list_check_runs(
        &self,
        repo: &RepoId,
        git_ref: &str,
    ) -> Result<Vec<(String, String)>, ForgeError>;

    /// Find an OPEN pull request whose head is `branch`.
    async fn find_open_pr(&self, repo: &RepoId, branch: &str) -> Result<Option<PrRef>, ForgeError>;

    /// Open a PR from `branch` into the repo's default branch.
    async fn create_pr(
        &self,
        repo: &RepoId,
        branch: &str,
        title: &str,
        body: &str,
    ) -> Result<PrRef, ForgeError>;

    /// Merge a PR by number with `strategy`; returns a reference string.
    async fn merge_pr(
        &self,
        repo: &RepoId,
        pr_number: u64,
        strategy: MergeStrategy,
    ) -> Result<String, ForgeError>;
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
///
/// Two clients hold the **2-token model** the live debug forced: reading check
/// status needs a token with Checks read access, which the write token (a
/// fine-grained PAT that pushes/opens/merges) may lack, while that PAT can do
/// what a CI runner's `github.token` cannot (open a PR). `client` (write token)
/// serves PR list/create/merge; `checks_client` (`BOLT_COSMATIC_CHECKS_TOKEN`, falling
/// back to `client`) serves `list_check_runs` only.
pub struct GithubForge {
    client: Octocrab,
    checks_client: Octocrab,
}

impl GithubForge {
    /// Build from a token in `GITHUB_TOKEN` or `GH_TOKEN` (never from `gh`), with
    /// an optional dedicated checks-read token in `BOLT_COSMATIC_CHECKS_TOKEN`.
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
        let checks_client = match std::env::var("BOLT_COSMATIC_CHECKS_TOKEN") {
            Ok(t) if !t.is_empty() => Octocrab::builder()
                .personal_token(t)
                .build()
                .map_err(|e| ForgeError(format!("octocrab build (checks): {e}")))?,
            _ => client.clone(),
        };
        Ok(Self {
            client,
            checks_client,
        })
    }
}

#[async_trait]
impl Forge for GithubForge {
    async fn find_open_issue_by_marker(
        &self,
        repo: &RepoId,
        marker: &str,
    ) -> Result<Option<IssueRef>, ForgeError> {
        // Dedup by scanning open issues for the fingerprint marker. The issues
        // LIST reflects writes sooner than search, but GitHub reads are still
        // eventually consistent: two calls within ~1-2s can each miss the
        // other's just-created issue and duplicate (observed live). Real
        // incidents re-fire far apart, so this is acceptable; a local
        // fingerprint->number cache could harden same-machine rapid calls
        // later. Scans the first 100 open issues.
        let page = self
            .client
            .issues(&repo.owner, &repo.name)
            .list()
            .state(octocrab::params::State::Open)
            .per_page(100u8)
            .send()
            .await
            .map_err(|e| ForgeError(format!("list issues: {e}")))?;
        Ok(page
            .items
            .into_iter()
            .find(|i| i.body.as_deref().is_some_and(|b| b.contains(marker)))
            .map(|i| IssueRef {
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

    async fn list_check_runs(
        &self,
        repo: &RepoId,
        git_ref: &str,
    ) -> Result<Vec<(String, String)>, ForgeError> {
        // The REST check-runs endpoint, read with the checks token. The live
        // debug proved a CI runner's github.token cannot resolve the
        // `statusCheckRollup` GraphQL `gh pr checks` uses; this typed REST call
        // reads fine with `checks:read` and handles a slash-bearing ref.
        let runs = self
            .checks_client
            .checks(&repo.owner, &repo.name)
            .list_check_runs_for_git_ref(Commitish(git_ref.to_string()))
            .send()
            .await
            .map_err(|e| ForgeError(format!("list check-runs: {e}")))?;
        Ok(runs
            .check_runs
            .into_iter()
            .map(|c| (c.name, check_bucket(c.conclusion.as_deref()).to_string()))
            .collect())
    }

    async fn find_open_pr(&self, repo: &RepoId, branch: &str) -> Result<Option<PrRef>, ForgeError> {
        // Filter open PRs by head ref in Rust rather than via the API's
        // `head=user:ref` param, whose format is a known footgun. The fix branch
        // is recent, so it is in the first page.
        let page = self
            .client
            .pulls(&repo.owner, &repo.name)
            .list()
            .state(octocrab::params::State::Open)
            .per_page(100u8)
            .send()
            .await
            .map_err(|e| ForgeError(format!("list PRs: {e}")))?;
        Ok(page
            .items
            .into_iter()
            .find(|p| p.head.as_ref().map(|h| h.ref_field.as_str()) == Some(branch))
            .map(|p| PrRef {
                number: p.number.unwrap_or_default(),
                url: p.html_url.map(|u| u.to_string()).unwrap_or_default(),
            }))
    }

    async fn create_pr(
        &self,
        repo: &RepoId,
        branch: &str,
        title: &str,
        body: &str,
    ) -> Result<PrRef, ForgeError> {
        // `gh pr create` auto-detected the base; the typed API needs it explicit.
        let base = self
            .client
            .repos(&repo.owner, &repo.name)
            .get()
            .await
            .map_err(|e| ForgeError(format!("get repo: {e}")))?
            .default_branch
            .unwrap_or_else(|| "main".to_string());
        let pr = self
            .client
            .pulls(&repo.owner, &repo.name)
            .create(title, branch, base)
            .body(body)
            .send()
            .await
            .map_err(|e| ForgeError(format!("create PR: {e}")))?;
        Ok(PrRef {
            number: pr.number.unwrap_or_default(),
            url: pr.html_url.map(|u| u.to_string()).unwrap_or_default(),
        })
    }

    async fn merge_pr(
        &self,
        repo: &RepoId,
        pr_number: u64,
        strategy: MergeStrategy,
    ) -> Result<String, ForgeError> {
        let method = match strategy {
            MergeStrategy::Rebase => octocrab::params::pulls::MergeMethod::Rebase,
        };
        let merged = self
            .client
            .pulls(&repo.owner, &repo.name)
            .merge(pr_number)
            .method(method)
            .send()
            .await
            .map_err(|e| ForgeError(format!("merge PR #{pr_number}: {e}")))?;
        if !merged.merged {
            return Err(ForgeError(format!("PR #{pr_number} was not merged")));
        }
        Ok(format!("merged #{pr_number}"))
    }
}

/// In-memory `Forge` for offline tests across modules (used by `forge` and
/// `automerge` tests). Stores issues and PRs and returns canned check-runs.
#[cfg(test)]
pub(crate) struct FakeForge {
    issues: std::sync::Mutex<Vec<(String, IssueRef)>>,
    next: std::sync::Mutex<u64>,
    prs: std::sync::Mutex<Vec<(String, PrRef)>>,
    checks: std::sync::Mutex<Vec<(String, String)>>,
}

#[cfg(test)]
impl FakeForge {
    pub(crate) fn new() -> Self {
        Self {
            issues: std::sync::Mutex::new(Vec::new()),
            next: std::sync::Mutex::new(1),
            prs: std::sync::Mutex::new(Vec::new()),
            checks: std::sync::Mutex::new(Vec::new()),
        }
    }

    /// Seed an open PR for `branch`.
    pub(crate) fn with_pr(self, branch: &str, number: u64) -> Self {
        self.prs.lock().unwrap().push((
            branch.to_string(),
            PrRef {
                number,
                url: format!("https://example.test/pull/{number}"),
            },
        ));
        self
    }

    /// Seed the `(name, bucket)` check-runs `list_check_runs` returns.
    pub(crate) fn with_checks(self, checks: &[(&str, &str)]) -> Self {
        *self.checks.lock().unwrap() = checks
            .iter()
            .map(|(n, b)| (n.to_string(), b.to_string()))
            .collect();
        self
    }
}

#[cfg(test)]
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

    async fn list_check_runs(
        &self,
        _repo: &RepoId,
        _git_ref: &str,
    ) -> Result<Vec<(String, String)>, ForgeError> {
        Ok(self.checks.lock().unwrap().clone())
    }

    async fn find_open_pr(
        &self,
        _repo: &RepoId,
        branch: &str,
    ) -> Result<Option<PrRef>, ForgeError> {
        Ok(self
            .prs
            .lock()
            .unwrap()
            .iter()
            .find(|(b, _)| b == branch)
            .map(|(_, p)| p.clone()))
    }

    async fn create_pr(
        &self,
        _repo: &RepoId,
        branch: &str,
        _title: &str,
        _body: &str,
    ) -> Result<PrRef, ForgeError> {
        let mut next = self.next.lock().unwrap();
        let number = *next;
        *next += 1;
        let pr = PrRef {
            number,
            url: format!("https://example.test/pull/{number}"),
        };
        self.prs
            .lock()
            .unwrap()
            .push((branch.to_string(), pr.clone()));
        Ok(pr)
    }

    async fn merge_pr(
        &self,
        _repo: &RepoId,
        pr_number: u64,
        _strategy: MergeStrategy,
    ) -> Result<String, ForgeError> {
        Ok(format!("merged #{pr_number}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> RepoId {
        RepoId {
            owner: "o".into(),
            name: "n".into(),
        }
    }

    #[test]
    fn parse_remote_ssh_and_https() {
        let a = RepoId::parse_remote("git@github.com:constantin-jais/bolt-cos-matic.git").unwrap();
        assert_eq!(
            a,
            RepoId {
                owner: "constantin-jais".into(),
                name: "bolt-cos-matic".into()
            }
        );
        let b = RepoId::parse_remote("https://github.com/constantin-jais/bolt-cos-matic").unwrap();
        assert_eq!(b.name, "bolt-cos-matic");
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

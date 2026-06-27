# A3 — Incident → Issue — Implementation Plan

> Inline TDD execution on worktree `harness-a3` (`feat/a3-incident-issue`, stacked on `feat/a2-ci-gate`).

**Goal:** `aom incident open` turns a structured incident into an **idempotent** GitHub issue (no duplicate for the same fingerprint), journaling it zero-PII. First half of the autonomous loop; dispatch (A4) is the next increment.

**Architecture:** `orchestrator::incident` (model + blake3 fingerprint + `~/.aom/incidents.jsonl` journal, mirroring aom's `audit.rs`). `orchestrator::forge` (an `async-trait` `Forge`: find-by-marker + create; `GithubForge` over octocrab, `FakeForge` for tests). Idempotency = a visible `aom-fingerprint: <hex>` footer searched before creating. CLI runs the async path via a `tokio` runtime `block_on`.

**Tech Stack:** octocrab + tokio (rt-multi-thread, macros) + async-trait (new — justified in ADR-0008), blake3/serde/toml (existing), `std::process` (git remote).

## Global Constraints

- Edition 2024, rust 1.95, MIT; zero clippy warnings under `-D warnings`; fmt clean (run `cargo fmt` before checks — repo rustfmt differs from the edit hook).
- Network is confined to `forge.rs` (octocrab). Everything else stays offline/deterministic.
- Zero token in code/logs: `GithubForge` reads `GITHUB_TOKEN`/`GH_TOKEN` from env only (no `gh`).
- Zero-PII journal (fingerprint, kind, severity, ts — no usernames/abs paths).
- Builds on A1 (`orchestrator`) and A2 (CI gate).

## Public interfaces

```rust
// orchestrator::incident
pub struct Incident { pub fingerprint: String, pub kind: String, pub severity: String,
                      pub title: String, pub body: String, pub ts_unix: u64 }
pub fn fingerprint(kind: &str, key: &str) -> String;          // blake3 hex of "kind\nkey"
impl Incident { pub fn new(kind,severity,title,body,key,ts_unix) -> Self; }
pub const MARKER_PREFIX: &str = "aom-fingerprint:";
pub fn issue_body_with_marker(body: &str, fp: &str) -> String; // body + "\n\n<sub>aom-fingerprint: `<fp>`</sub>"
pub fn append_journal(inc: &Incident, dir: &Path) -> std::io::Result<()>; // ~/.aom/incidents.jsonl

// orchestrator::forge
pub struct RepoId { pub owner: String, pub name: String }
impl RepoId { pub fn parse_remote(url: &str) -> Option<RepoId>; } // git@github.com:o/n.git | https://github.com/o/n(.git)
pub struct IssueRef { pub number: u64, pub url: String }
#[async_trait] pub trait Forge {
    async fn find_open_issue_by_marker(&self, repo:&RepoId, marker:&str) -> Result<Option<IssueRef>, ForgeError>;
    async fn create_issue(&self, repo:&RepoId, title:&str, body:&str, labels:&[String]) -> Result<IssueRef, ForgeError>;
}
pub struct GithubForge { /* octocrab client */ }
impl GithubForge { pub fn from_env() -> Result<Self, ForgeError>; }
// idempotent open: find by fingerprint marker → reuse, else create
pub async fn open_or_reuse<F: Forge>(forge:&F, repo:&RepoId, inc:&Incident, labels:&[String])
    -> Result<(IssueRef, bool /*created*/), ForgeError>;
```

## Tasks

### T1 — deps + ADR-0008

- `cargo add` (in `crates/orchestrator`) `octocrab`, `tokio --features rt-multi-thread,macros`, `async-trait`; promote to `[workspace.dependencies]` and reference with `.workspace = true`. (cli gains `tokio` for the runtime.)
- ADR-0008: why we accept octocrab+tokio (user chose the library path over `gh`); network confined to `forge.rs`; token from env only.
- `cargo build --workspace` green; commit.

### T2 — incident model + journal (TDD)

- Tests: `fingerprint` stable & differs by kind/key; `issue_body_with_marker` contains the fp; `append_journal` writes one JSON line, zero-PII (no abs path / username fields).
- Implement; `cargo test -p orchestrator` green; commit.

### T3 — Forge trait + FakeForge + idempotency (TDD)

- `FakeForge` (Mutex<Vec<stored issues>>), matches marker substring in body.
- Tests: `RepoId::parse_remote` for ssh+https forms; `open_or_reuse` on empty Fake → creates (created=true); second call same incident → reuses (created=false), only one issue stored.
- Implement; green; commit.

### T4 — GithubForge (octocrab) [verify octocrab API via context7]

- `from_env` (token), `find_open_issue_by_marker` (search `repo:o/n is:issue is:open in:body <fp>`), `create_issue` (issues handler). Note: GitHub search has indexing latency → idempotency is across separate runs, documented.
- Compiles + clippy clean (no live call in tests); commit.

### T5 — CLI + live test + PR

- `aom incident open --kind --title --body [--severity m] [--repo o/n] [--label ...]`: build Incident (ts via SystemTime), journal, `RepoId` from `--repo` or `git remote get-url origin`, `GithubForge::from_env`, `open_or_reuse`, print `created/reused #N url`. Async via `tokio::runtime` block_on in `crates/cli`.
- **Live test (after user OK — creates a real issue):** `export GITHUB_TOKEN=$(gh auth token)`; run twice → first creates, second reuses (same #).
- `cargo fmt && clippy -D warnings && test --workspace` green; PR base `feat/a2-ci-gate`.

## Verification

- Logic proven offline via `FakeForge` (idempotency).
- One live run proves `GithubForge` end-to-end (issue created once, reused on re-run).
- Workspace stays green; `aom gate run` still passes.

## Deferred to A4

`aom dispatch` (worktree + claude -p loop + PR), `get_issue`/`create_pr`/`comment_issue` Forge methods, gate→incident auto-wiring.

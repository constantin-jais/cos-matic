# ADR-0036 — Orchestration authorization model

## Status

Accepted (2026-07-02).

## Context

Handoff documents from the Wrench analysis phase carry authorization evidence via
`biscuit_authorization_references`. These references form a delegation chain:
root human (via manual issue or scheduled run) → agent (Bolt-Cos-Matic loop) →
this orchestration request.

The orchestrator must not accept and act on a handoff unless the authorization
chain is valid, unbroken, and authorized for the scope of this particular
operation (e.g., the human authorized the agent to solve this class of problem,
and the agent did not alter the token).

Today, handoffs are accepted on submission alone, with no token validation.

## Decision

Define a **one-shot, depth-1 Biscuit delegation-chain verification step** that
runs before handoff acceptance.

The verification will:

1. Extract `biscuit_authorization_references` from the handoff.
2. Load the public key(s) that signed the delegation root.
3. Validate the chain (root authority → delegation → this request).
4. Enforce token expiration and any additional time-based checks.
5. Return a typed principal (user ID, tenant, role, scope) or a structured
   authorization error.

Because the minting process and public key distribution live outside this crate
(in rumble-lm), the verification will be implemented behind a testable trait:

```rust
pub trait BiscuitAuthorizer: Send + Sync {
    fn verify_delegation_chain(
        &self,
        token: &str,
        public_keys: &[PublicKey],
    ) -> Result<AuthorizedPrincipal, AuthorizationError>;
}
```

This allows:

- **Production**: a real implementation using `biscuit-auth` crate and external
  key material.
- **Testing**: a stub implementation that returns known-valid principals or
  predictable errors without requiring live minting.

Verification will be gated behind a **rollout flag** (environment variable or
config field, default: `warn-only` to avoid breaking existing handoff flows).

The flag semantics:

- `disabled` — skip verification, log a warning (allows brownfield operation).
- `warn-only` — run verification, log all errors as warnings, accept handoff
  regardless (permits gradual rollout).
- `enforce` — run verification, reject handoff on any error (hard gate).

## Consequences

- Handoff validation is now layered: syntactic (schema), semantic
  (spec_context), and authorization (delegation chain).
- The service boundary now asserts that the requester is authorized before
  orchestrating.
- All three verification levels are independently testable.
- No production outage from missing keys or unminted tokens: default is
  `warn-only`, and the flag is live-configurable.
- Future tightening to `enforce` is non-breaking: all currently-minted handoffs
  will already carry valid references (because they flow from the same Wrench
  that will integrate this gate).

## Implementation notes

`crates/orchestrator/src/handoff/authorization.rs` defines the trait, error
types, and a stub implementation suitable for testing.

The actual integration with `biscuit-auth` crate and public key loading is
deferred until the rumble-lm integration point. This ADR documents the seam and
the contract; the next milestone will plug the real verifier.

## Non-goals

- Does not implement revocation, key rotation, or online validation with an
  external service (depth-1 only).
- Does not handle multi-signature or threshold schemes.
- Does not parse or validate the internal structure of delegated tokens (Biscuit
  library handles that).

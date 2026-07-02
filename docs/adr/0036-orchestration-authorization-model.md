# ADR-0036 — Orchestration authorization model

## Status

Proposed (2026-07-02). Governs handoff validation and orchestrator command authorization.

## Context

Biscuit is the ecosystem's decided delegation standard (contract `DelegatedAuthorization-Biscuit v0.1 Draft`). The token model is explicit: facts and checks, locally verifiable, attenuable but never expanding rights, closed-world authorization.

However, handoff validation and orchestrator commands currently do not verify any delegation token. They rely instead on environment kill-switches (binary control levers without identity), which provide a defense-in-depth layer but not an authorization model.

The gap is critical: a kill-switch can stop all agents, but it cannot grant or deny specific operations to specific requesters. An agent that submits a handoff carries no provenance. The orchestrator does not know whether the request is from an authorized human root, a legitimately delegated agent, or a compromise of either.

## Decision

**Handoff validation must verify the Biscuit delegation chain** before accepting any orchestration request:

1. The handoff submission carries `biscuit_authorization_references` — a delegation chain proving:
   - a human root issued a capability to an agent;
   - the agent (or a further-delegated agent) is making this request;
   - the token has not expired and is not revoked.

2. Validation rejects any handoff without a valid delegation chain. An exception (operator override via a sealed out-of-band channel) is documented and audited; it is not the default path.

3. **Initial scope (P0):** depth-1 delegation only. A human root delegates to an agent; that agent acts. Further attenuation (agent → sub-agent, attenuated tokens for specific repositories) is deferred.

4. **Token lifetime:** one-shot tokens for now. Usage-count limits (allowing a token to be used _N_ times before expiration) require server-side state (a revocation or usage registry) and are deferred until that substrate exists.

5. **Kill-switches remain as defense-in-depth,** not as the authorization model. The existing environment switches (`BOLT_COSMATIC_DISPATCH_DISABLED`, `BOLT_COSMATIC_AUTOMERGE_DISABLED`, `BOLT_COSMATIC_DEPLOY_DISABLED`, `BOLT_COSMATIC_LOOP_DISABLED`) can still halt each operation class; they do not replace per-request authorization.

## Consequences

- **Implementation is a gate before any runtime expansion.** The orchestrator cannot execute a plan, merge a PR, or deploy without first validating the delegation chain. The check is enforced in code, not as a human audit step.
- **The contract moves from Draft to Accepted** once cos-matic demonstrates verification against fixtures and rumble-lm mints tokens against those same fixtures, proving interoperability.
- **Tests must cover valid, expired, revoked, and malformed tokens.** Fixture-driven tests establish a golden token set (issued by a test root, delegated to a test agent, with known expiration times and attenuation rules).
- **Audit trails record the delegation chain,** not just the outcome. A successful merge is linked to the token that authorized it; a rejected handoff logs which delegation check failed.
- **Future expansion (multi-level delegation, usage counts, scoped repositories) is possible because the foundation validates and stores the token structure.** Adding a new attenuation rule does not require changes to the validation core; it adds a new check to the authorizer's policy.

## Implementation notes

The validation gate lives in `crates/orchestrator/src/authorization/mod.rs` and is called before `dry_run_plan_file()` and before live execution. A successful call returns the validated principal (user, tenant, authenticated agent identity); a failed call raises a typed error that is audited and returned to the caller.

Biscuit public keys are loaded from an environment variable or a keyring at startup. Key rotation is supported by accepting both the current and previous public key until old tokens expire.

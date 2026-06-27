# Security Baseline

- Validate and sanitize every external input at the trust boundary; never trust
  client-supplied data.
- Never log secrets, credentials, tokens, or personally identifiable information.
  Redact before logging.
- Keep secrets out of source control and out of error messages; load them from
  the environment or a secrets manager at runtime.
- Prefer least privilege: the narrowest scope, the shortest-lived token, the
  smallest blast radius.
- Treat dependencies as attack surface: pin versions, review licenses, and audit
  for known vulnerabilities.

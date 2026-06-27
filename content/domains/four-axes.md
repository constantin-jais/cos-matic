# Decision Axes

When a choice is non-obvious, decide on these four axes, in priority order. When
they conflict, the earlier axis wins.

1. **Security** — attack surface, threat model, no secrets or PII in logs,
   validate untrusted input at the boundary.
2. **Quality** — readability over cleverness, strict types, no panics on input,
   no dead code, lints with zero warnings.
3. **Performance** — measure hot paths, avoid needless allocation, prefer async
   for I/O, watch for N+1.
4. **Completeness** — done means code plus tests plus docs, within the stated
   scope.

These are the only valid criteria. Effort, calendar estimates, and "MVP vs
nice-to-have" are not decision axes here — reason in terms of technical
complexity. Flag over-engineering (a solution larger than its scope) as its own
defect, distinct from incompleteness.

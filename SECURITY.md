# Security policy

`expat-rs` is pre-release software (v0.0.x). Do not rely on it in production yet.

## Reporting a vulnerability

Please use GitHub's private vulnerability reporting for this repository
("Security" tab → "Report a vulnerability"). Do not open a public issue for
security problems.

Include a minimal reproducer (input bytes and parse options) if possible. A
reproducer that makes the port panic, hang, use unbounded memory, or disagree
with libexpat 2.9.0 on an observable result is in scope.

## Scope

- `crates/expat` (the safe port) is in scope.
- `crates/expat-oracle` vendors libexpat C for *testing only*. Vulnerabilities in
  libexpat itself should be reported upstream: https://github.com/libexpat/libexpat/security

# Security policy

## Reporting a vulnerability

Use [GitHub private vulnerability reporting](https://github.com/ahilles107/gravity/security/advisories/new)
to report a suspected vulnerability privately to the repository maintainers.
Do not open a public issue containing exploit details or credentials.

Include the affected version, reproduction steps, expected impact, and a minimal
example where possible. Remove unrelated personal data and secrets. If you have
exposed a credential, revoke it rather than relying on deletion of the report.

## Supported versions

Security fixes target the latest release. Older versions may require an upgrade;
there is no separate long-term support branch or guaranteed response time.

## Security model

Gravity launches Claude Code processes with the operating-system permissions of
the account running the daemon. Bot directories and permission settings are
cooperative boundaries, not an operating-system sandbox. Run only trusted code
and review tool permissions before allowing agents access to sensitive resources.

Keep the daemon on a trusted network and use device-scoped tokens for remote
clients. Never share the owner token. See the [architecture](docs/architecture.md)
and [public-build configuration](docs/public-builds.md) for authentication,
telemetry, and release-signing behavior.

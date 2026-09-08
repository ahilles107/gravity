# Public builds and optional services

The Rust and JavaScript dependencies come from public registries. Building and
testing require no service credentials. Running real bots requires a Claude Code
installation and authentication; tests use a deterministic runtime double.

## Telemetry

Telemetry is configured at build time. It is disabled when the PostHog project
token or host is missing or blank. In configured builds, usage analytics and
error reporting start automatically when the app launches.

To configure a distribution, copy `apps/desktop/.env.example` to
`apps/desktop/.env.local` and set `VITE_POSTHOG_PROJECT_TOKEN` and
`VITE_POSTHOG_HOST` before building.
Leave the token empty to make telemetry unavailable. `VITE_*` values are public
frontend configuration: never put a personal API key in them.

When enabled, Gravity sends typed usage events, application/daemon versions,
connection type, and error reports. Error messages and optional daemon log tails
are redacted before sending, but redaction cannot guarantee removal of every
kind of sensitive text. Session recording, page views, DOM autocapture and
feature flags are disabled.

Source-map upload is separate, optional build tooling. Export non-empty
`POSTHOG_PERSONAL_API_KEY` and `POSTHOG_PROJECT_ID` in the build environment to
enable it; `POSTHOG_HOST` defaults to the EU endpoint. `POSTHOG_RELEASE_VERSION`
labels the upload. Empty credentials skip the plugin. Keep the personal key in
CI secrets, never in a `VITE_*` variable or tracked file. Uploading maps shares
source with the configured PostHog project.

## CI and releases

CI uses GitHub-hosted Ubuntu and Apple Silicon macOS runners. The examples in
`ops/runners/` are optional and use a public base image. They have no dependency
on another repository or an existing host fleet.

Source builds have no updater endpoint or signing key and do not produce signed
updater artifacts. Release workflows build an ad-hoc signed DMG and daemon
tarball without maintainer secrets. A `v*` tag publishes GitHub release assets;
manual runs only upload workflow artifacts.

Optional repository configuration:

| Setting | Kind | Purpose |
| --- | --- | --- |
| `UPDATER_PUBLIC_KEY` | Variable | Your updater verification key; public, not a credential |
| `TAURI_SIGNING_PRIVATE_KEY` | Secret | Enables signed updater archives and `latest.json`; requires the matching public key |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Secret | Password for an encrypted updater private key, if used |
| `RELEASE_BASE_URL` | Variable | Optional HTTPS download mirror base; default is GitHub release assets |
| `R2_BUCKET` | Variable | Enables the optional R2 mirror with `RELEASE_BASE_URL` |
| `CLOUDFLARE_API_TOKEN`, `CLOUDFLARE_ACCOUNT_ID` | Secrets | Credentials for that optional R2 upload |
| `POSTHOG_PROJECT_TOKEN`, `POSTHOG_HOST` | Variables | Optional telemetry destination; enables capture in configured builds |
| `POSTHOG_PROJECT_ID` | Variable | Optional source-map destination |
| `POSTHOG_PERSONAL_API_KEY` | Secret | Optional source-map upload credential |
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | Secrets | Optional Developer ID signing and notarization |

### Updater signing

Each independently distributed app needs its own updater signing pair. Store
the public verification key in `UPDATER_PUBLIC_KEY` and the private key in
`TAURI_SIGNING_PRIVATE_KEY`. Preserve that pair across releases so installed
clients continue to accept updates.

A local signed build can supply `plugins.updater` and
`bundle.createUpdaterArtifacts` using Tauri's `--config` option. Forks
distributing a separate app should also choose their own app identifiers and
launchd labels.

### Optional download mirror

GitHub release assets are the default download host. To use an R2 mirror,
configure `RELEASE_BASE_URL` with an HTTPS base URL, `R2_BUCKET` with the bucket
name, and the two Cloudflare secrets listed above. The workflow uploads versioned
artifacts before publishing `desktop/gravity/latest.json`. Leave the mirror
variables unset to use GitHub releases without Cloudflare credentials.

### Marketing site

The marketing site is an optional deployment. Configure its download manifest
and hosting as described in `apps/marketing/README.md`. Review canonical URLs,
imagery, and branding before distributing a fork.

## Secret scanning before publication

Keep local environment files, MCP credentials, signing keys, and daemon state
out of Git. `.gitignore` covers common locations; it cannot remove files already
tracked or secrets from existing history.

Install Gitleaks 8.30.0 and run from the repository root:

```sh
gitleaks git --redact --log-opts="--all"
```

The `secrets` CI workflow scans the checked-out history. `.gitleaks.toml` extends
the standard detectors with a Linear-key rule and narrowly excludes one known
synthetic UI fixture. Scan every branch/tag you intend to publish; review
non-code assets and GitHub issues, Actions logs, and release attachments too.
If a real credential is found, revoke it before considering any history cleanup.
History rewriting requires a coordinated backup and explicit approval.

Gravity's source is licensed under the [MIT License](../LICENSE). Before
publishing a distribution, preserve required third-party notices and review
redistribution rights for any added images or other assets.

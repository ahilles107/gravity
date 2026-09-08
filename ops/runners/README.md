# Optional self-hosted CI runners

CI uses GitHub-hosted Ubuntu and macOS runners by default. These examples are
only for maintainers who explicitly choose to operate their own runners.
Do not route untrusted public pull requests onto machines with credentials or
access to private infrastructure.

## Linux

The Dockerfile extends the public `myoung34/github-runner:ubuntu-noble` image
with Rust build dependencies; it does not require another project's image.
Workflow setup actions install Node, pnpm, and Rust.

```sh
cd ops/runners/linux
cp .env.example .env
# Set your repository URL and a dedicated runner credential in .env.
docker compose build
docker compose up -d --scale runner=2
```

Keep `.env` local. Use a dedicated credential scoped to your repository; never
reuse credentials from unrelated projects. The container mounts the host's
Docker socket, so jobs must be trusted. Each replica has separate persistent
workspaces and Rust caches. Set memory and CPU limits for your own host.

See the [runner image documentation](https://github.com/myoung34/docker-github-actions-runner)
for available credentials, environment variables, and image tags.

## macOS (Apple Silicon)

Run `macos/install-runner.sh` on the runner Mac with `REPO_URL`, `RUNNER_NAME`,
and a short-lived `TOKEN` from your repository's Actions runner settings.
The default installation directory is `~/actions-runner-gravity`; override
`DIR` when needed. The installer registers a launchd user agent.

Change a trusted workflow's `runs-on` explicitly to the labels you registered:
`[self-hosted, Linux, X64]` or `[self-hosted, macOS, ARM64]`.

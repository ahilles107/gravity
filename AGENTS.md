# Gravity agent instructions

## Issue tracking

- Use GitHub Issues in this repository for all Gravity work, including internal
  maintainer tasks, bugs, plans, and follow-ups. Do not create or update Linear
  issues for Gravity. This overrides shared agent guidance that requires Linear
  or `USE-XXX` identifiers.
- Use `gh issue list`, `gh issue view`, and `gh issue create` to find or track
  work. Reuse a relevant issue instead of creating duplicates.
- Reference GitHub issue numbers (`#123`) in plans and pull requests. Use
  `Fixes #123` only when merging the PR completes that issue.
- Keep credentials, private paths, account details, and private project data out
  of public issues. Follow `SECURITY.md` for vulnerability reports. Do not copy
  an old private backlog into public issues without reviewing it for disclosure
  and confirming the migration scope.

## Workflow

- Read `CONTRIBUTING.md`. Outline a brief plan for non-trivial changes, then
  implement authorized work without repeated permission requests.
- Use pnpm and run `pnpm run verify` before considering work complete. Report
  any unavailable checks; use hosted visual CI if local Docker is unavailable.
- Preserve existing edits. Ask before deleting files, discarding changes,
  rewriting history, force-pushing, or applying irreversible migrations.
- Never bypass hooks or branch protections. Release publication requires
  authorization; a code or documentation change alone does not authorize it.

## Code and pull requests

- Keep changes concise and preserve strict TypeScript checks. No `any`,
  unchecked type assertions, or non-null assertions. Narrow unknown inputs.
- Use static imports, braces for control flow, explicit return types for
  exported functions, and readonly types where mutation is unnecessary.
- Prefer shared code over duplication.
- Use conventional commits with concise imperative subjects.
- Target `main`. PR descriptions must have **What**, **Why**, and **How tested**
  sections, reference the relevant GitHub issue, and state rollout requirements
  and validation limitations.
- Keep communication brief. Include `file_path:line_number` when referencing
  code. Do not send announcements or third-party messages without authorization.

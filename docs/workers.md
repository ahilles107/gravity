# Temporary workers

Status: implemented in the daemon and covered by
`crates/gravityd/tests/workers.rs` and `crates/gravityd/tests/worker_repo.rs`.
The desktop app marks workers in the bot list and sets a project's shared
repository; it does not show the queue yet.

A bot can split a job into independent pieces and hand each to a
temporary worker: a book bot spawns a worker per chapter, a migration bot
one per module, a research bot one per source. Each worker exists for one
task. It starts, does that task, reports, and is removed.

## What a bot does

```text
spawn_worker(task, name?, instructions?, description?, runtime?, machine?, deadline_hours?)
list_workers()
cancel_worker(name, reason?)
```

`spawn_worker` returns at once with the spawn's `state`:

- `running`: a worker was created, and `task` was delegated to it as an
  ordinary task. The reply carries its `task_id` and `machine`. The result
  arrives as a `done` for that task, like any delegated task's.
- `queued`: every worker slot is busy. The reply carries `queue_position`, and
  the worker starts by itself when a slot frees.
- `failed`: it could not be placed, for a reason waiting will not fix (an
  unreachable repository, a missing runtime). `note` says why, and the parent
  is also sent a note.

A spawn without `name` is called `worker-N`. A queued spawn reserves its
name, so the parent can address and cancel it before it starts. `machine` is
`"here"`, the name of a linked machine, or omitted for any machine with a
free slot.

Workers cannot spawn workers. This prevents deadlock: workers holding every
slot while waiting on children that are stuck in the queue.

## Limits

Workers have a cap of their own, `max_workers_per_project` in `gravityd.toml`
(default 4). It counts the workers running on this machine, per project. It
is separate from `max_bots_per_project`, so a project whose permanent bots
fill their cap can still fan out. Workers do not count toward that cap.

Spawns past the cap wait in the project's queue. The queue is first in,
first out, and holds at most 200 spawns. A slot frees when a worker's task
closes and the worker is archived. The daemon reconciles the queue every few
seconds, and immediately whenever a task closes.

A worker's task follows the normal task rules: a deadline (24h by default,
`deadline_hours` up to 168), a reply budget, and the hop limit, which counts
through workers. One rule does not apply: tasks given to workers do not count
toward the three-open-task fan-out. The worker cap and the queue bound those
instead.

## Lifecycle

1. **Queued.** The spawn is stored in the `worker` table on the parent's
   daemon.
2. **Placed.** The first machine with a free slot gets it: this one first,
   then each linked machine that is online. Placing it creates a bot with
   `temporary` set and sends the brief to it as a `task`. Workers get no
   greeting and no "bot created" toast.
3. **Finished.** The worker calls `complete_task`, or the parent cancels it
   (`cancel_worker`, or `cancel_task` on its task), or the task expires.
4. **Retired.** Once its task is closed, and everything it sent across a peer
   link has left, the worker is archived like any deleted bot. Its history
   and workspace are kept until retention reclaims them. A temporary bot that
   is never given a task is retired after ten minutes.

If the parent is deleted, its queued spawns are dropped and its running
workers' tasks are cancelled. If the owner deletes a running worker, its task
is cancelled and the spawn closes as `cancelled`.

## Shared repository

Workers on different machines cannot read each other's disks, so a project
can name a shared git repository (`set_project_repo`, or Project settings →
Shared repository). When it has one:

- **Before a worker starts**, its machine fetches the branch into a bare
  per-project cache (`projects/<project>/repo.git`, cloned on first use). It
  then adds a worktree at the worker's `workspace/repo` on a branch of its
  own, `gravity/<name>-<id>`, starting at the tip it just fetched. Every
  worker therefore begins from the latest pushed work, whichever machine
  pushed it. If the checkout fails, the worker is retired and the spawn
  fails with git's error.
- **When the worker calls `complete_task`**, the daemon commits whatever it
  left (as the machine's git identity, or `<name> (Gravity worker)` when none
  is configured). It rebases that commit onto the branch and pushes. A push
  rejected because another worker pushed first is fetched and retried. If
  the rebase conflicts, the worker's own branch is pushed instead. Either
  way, one line is appended to the result the parent reads:
  `repo: pushed <commit> to <branch>`, or `repo: this work conflicts with
  <branch>, so it was pushed to branch <own> instead — merge that`.
- **Permanent bots** are told the repository's URL and branch. They keep
  their own clone and `git pull` before starting and after each worker
  reports.

Git runs non-interactively (`GIT_TERMINAL_PROMPT=0`, the `ext` transport
disabled) with each machine's own credentials. Clone, fetch and push time out
after five minutes. Operations on one project's cache run one at a time. A
retired worker's checkout is removed only once nothing in it is unpushed;
otherwise it is kept. Git hooks are not bypassed: a commit a hook refuses is
reported as a failed push, and the work stays in the checkout.

## Across machines

In a project linked with another daemon (see [peer bots](peer-bots.md)), a
spawn that finds this machine full is offered to each online linked machine
in turn. The offer is the existing `create_bot` peer frame with
`temporary: true`, plus `repo` when the project has one. The peer creates the
worker under its own worker cap, checks the repository out with its own
credentials, and replies. A full peer refuses with `at_capacity`, and the
spawn stays queued. The parent's daemon stands the worker in as a linked bot,
marked temporary, and delegates the brief to it. The task is mirrored and the
result comes back over the link, as for any linked bot.

A peer with no repository of its own for the linked project adopts the
asker's. Each machine reconciles its own workers: the worker is retired on
its machine once its result has crossed, and the stand-in follows with the
roster.

Peer requests time out after 60 seconds, so the first clone of a very large
repository on a peer can time out. The worker then fails, and the next spawn
reuses whatever the cache fetched.

## Not yet

- The desktop app does not show the queue (`list_workers` is MCP-only).
- Setting a repository does not propagate to linked projects until a worker
  is placed there.
- Work from a worker whose task was cancelled or expired is not pushed.

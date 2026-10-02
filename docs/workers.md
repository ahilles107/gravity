# Temporary workers

Status: implemented in the daemon and covered by
`crates/gravityd/tests/workers.rs` and `crates/gravityd/tests/worker_repo.rs`.
The desktop app marks workers in the bot list, and its project settings show
the project's workers (running, queued, recently finished, each cancellable)
and set the shared repository.

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
   link has left, the worker is archived like any deleted bot. A worker with
   a checkout first has its unpushed work saved (see below). Its history
   and workspace are kept until retention reclaims them. A temporary bot that
   is never given a task is retired after ten minutes.

If the parent is deleted, its queued spawns are dropped and its running
workers' tasks are cancelled. If the owner deletes a running worker, its task
is cancelled and the spawn closes as `cancelled`.

## Shared repository

Workers on different machines cannot read each other's disks, so a project
can name a shared git repository (`set_project_repo`, or Project settings →
Shared repository). When it has one, each worker handles git itself, as its
prompt tells it to, in its own terminal and in parallel with every other
worker. Placing a worker never waits on git, so a slow clone or an
unreachable remote never holds up a spawn.

- **Before it starts**, a worker clones the branch into `repo/` in its
  workspace (`git clone --filter=blob:none --branch <branch> <url> repo`), or
  pulls if `repo/` is already there. It therefore begins from the latest
  pushed work, whichever machine pushed it.
- **Before `complete_task`**, it commits, runs `git pull --rebase`, resolves
  any conflicts itself, and pushes to the branch. If it still cannot push, it
  pushes its own branch, `gravity/<name>-<id>`, instead. Its result names the
  commit or the branch.
- **When a worker retires with work not on the remote** (it forgot to push,
  its push failed, or its task was cancelled or expired), the daemon on its
  machine commits what it left and pushes the worker's own branch. The
  worker then sends whoever spawned it a note:
  `ch-3 stopped: its task was cancelled. Work it had not pushed is saved on
  branch gravity/ch-3-… of the project repository; merge it if you want it.`
  This runs in the background. The worker is archived only after the note
  has left, so it never speaks once archived. A worker with nothing unpushed
  retires without a note, and nothing unfinished reaches the shared branch.
- **Permanent bots** are told the repository's URL and branch. They keep
  their own clone and `git pull` before starting and after each worker
  reports.

Each machine uses its own git credentials, the same ones the bots use in
their terminals. The daemon's own git work (the save on retirement) runs
non-interactively (`GIT_TERMINAL_PROMPT=0`, the `ext` transport disabled),
with a five-minute limit on the push. Git hooks are not bypassed: a commit a
hook refuses is reported in the note, and the work stays in the worker's
workspace.

## Across machines

In a project linked with another daemon (see [peer bots](peer-bots.md)), a
spawn that finds this machine full is offered to each online linked machine
in turn. The offer is the existing `create_bot` peer frame with
`temporary: true`, plus `repo` when the project has one. The peer creates the
worker under its own worker cap and replies at once. Offers to linked machines
go out in parallel, after every spawn that fits here has started. A full peer refuses with `at_capacity`, and the
spawn stays queued. The parent's daemon stands the worker in as a linked bot,
marked temporary, and delegates the brief to it. The task is mirrored and the
result comes back over the link, as for any linked bot.

A peer with no repository of its own for the linked project adopts the
asker's. Each machine reconciles its own workers: the worker is retired on
its machine once its result has crossed, and the stand-in follows with the
roster.


## The app

Project settings list the project's workers under **Workers**: those running
(and on which machine), the queue with each spawn's position, and the most
recent that finished, with the spawning bot and the opening of each brief.
The owner can cancel any spawn that has not finished; a running worker is told
to stop in the owner's name. The list follows `workers_updated` pushes.

## Not yet

- Setting a repository does not propagate to linked projects until a worker
  is placed there.
- The Workers panel lists this daemon's spawns; a worker a linked machine's
  bot spawned here shows in the bot list, and in that machine's panel.

# Safety guards

Roadmap step 6. Guards decide whether *now* is a safe moment to economize.
They run between planning and execution, and only on `apply` and `dry-run`.
**Restore is never guarded**: recovery must always be possible.

```text
plan_changes → guard_plan → execute_plan → driver
```

## Two kinds of "no"

| Kind       | Meaning                                   | Field                  |
|------------|-------------------------------------------|------------------------|
| blocker    | cannot tell if it is safe, or bad input   | `ChangePlan.blockers`  |
| deferral   | can tell: the node is busy; retry later   | `ChangePlan.deferrals` |

Either one makes `plan.blocked` true, strips the actions, and makes drivers
refuse. The audit event `apply_refused` records both lists.
A plan with no actions is never touched: a node already at target is not
"blocked" just because it is busy.

## What is checked

| Check                         | Result  | How it is observed                              |
|-------------------------------|---------|-------------------------------------------------|
| established SSH sessions > 0  | defer   | `/proc/net/tcp`, `tcp6`: state `01`, local port 22 |
| protected process running     | defer   | `/proc/<pid>/comm` matched against `--protect`  |
| SSH state unreadable          | blocker | neither `tcp` nor `tcp6` could be read          |
| process list unreadable       | blocker | only when `--protect` was given                 |
| snapshot older than 60 s      | blocker | `captured_at` vs now                            |
| snapshot dated in the future  | blocker | more than 5 s ahead (small clock skew allowed)  |

Notes:

- "Protected process running" is deliberately conservative: it does not try to
  judge whether the process is busy. If it is running, Nordfir waits.
- Process names are kernel `comm` values, which are truncated to 15
  characters; the comparison truncates your name the same way.
- The SSH port is 22 (`collect_activity(ssh_port=...)` takes another).
- Only SSH connections *to* this node's port are counted.
- Original state is not age-limited: it is meant to live until a restore.
  Staleness of the *hardware* is covered by the frequency-range check in
  [CHANGE_PLAN.md](CHANGE_PLAN.md) and [RESTORE.md](RESTORE.md).

## CLI

```bash
nordfir dry-run economize --protect postgres --protect nginx
nordfir apply economize --protect postgres --confirm-system-power-write
```

`--protect` is repeatable. Exit code is 1 when a guard stops the plan; the
printed plan shows the `blockers` / `deferrals` lists.

Code: `src/nordfir/guards.py`; tests: `tests/test_guards.py` (uses temporary
`/proc` trees plus `tests/fixtures/proc/net/`).

## Not yet

- Per-service activity (CPU or network use) rather than presence.
- A config file for the protected list; for now it is a CLI option.
- Other session types (console logins, tmux) beyond SSH TCP connections.

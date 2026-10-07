# Deployment preflight

Roadmap step 10. `nordfir preflight` answers one question before you run
Nordfir on a host: **is this machine ready?** It collects *every* problem in
one run instead of stopping at the first.

## Read-only, strictly

It never creates the state directory, never opens the database for writing
(SQLite `mode=ro`), and never writes a sysfs file: write permission is asked
of the kernel with `os.access`, not tried. A test asserts nothing on disk
changes.

## Roles

| Role              | For                                        |
|-------------------|--------------------------------------------|
| `observe`         | the read-only observer loop                |
| `apply` (default) | also `apply` and `restore` (superset)      |

## Checks

| Check             | Roles   | Fails when                                                    | Warns when |
|-------------------|---------|---------------------------------------------------------------|------------|
| `ssh-visibility`  | both    | neither `/proc/net/tcp` nor `tcp6` readable (guards would block everything) | |
| `proc-basics`     | both    | `loadavg`, `meminfo` or `uptime` unreadable                   | |
| `state-dir`       | both    | not creatable; not a directory; not owned by the current user; group/other accessible (dir or db must be 0700/0600); not writable | |
| `database`        | both    | cannot open read-only; schema newer than supported            | |
| `audit-chain`     | both    | hash chain broken ([AUDIT.md](AUDIT.md))                      | schema < v3 (not chained yet) |
| `confinement`     | both    | never                                                         | root; capabilities held by a non-root user; `NoNewPrivs` unset; `/proc/self/status` unreadable |
| `cpufreq`         | apply   | a safe baseline cannot be captured (unknown values, inconsistent limits) | |
| `cpufreq-write`   | apply   | governor / max-frequency files not writable on any CPU        | |
| `original-state`  | apply   | never                                                         | none stored (apply stays blocked until `save-original`) |
| `pending-restore` | apply   | never                                                         | host differs from the stored original (changes are in place) |

`ready` is true when no check **fails**; warnings never block. Exit code 0 when
ready, 1 otherwise.

`confinement` reports facts from `/proc/self/status` (uid, `NoNewPrivs`,
`CapEff`, seccomp mode). The observer needs no privileges at all, so root is
flagged for it. It cannot prove a systemd sandbox is in place; it shows what
the process itself can see.

## CLI

```bash
nordfir preflight                     # role apply
nordfir preflight --role observe --state-dir /var/lib/nordfir
```

Example on a desktop without privileges: `cpufreq-write` fails ("N file(s)
not writable by this user"), which is correct: `apply` needs root or a udev
rule granting write access, while `--role observe` can still be ready.

Code: `src/nordfir/preflight.py`; tests: `tests/test_preflight.py`.

## Not yet

- Checking an actual systemd unit or container sandbox.
- Checking that the stored original state matches this hardware beyond the
  `pending-restore` comparison (the planners already do that).

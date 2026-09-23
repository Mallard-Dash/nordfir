# Nordfir Architecture

## One engine, several subsystems

Nordfir has one authoritative decision engine. It should not contain several
independent engines that can issue competing actions against the same node.

The main engine coordinates five focused subsystems:

1. **State** — collects and normalizes observations into snapshots.
2. **Safety** — guards, authority checks, and preflight validation.
3. **Energy** — power readings, estimates, REST/OFF economics, and later demand statistics.
4. **Timing** — availability windows, retry/defer timing, idle timers, and scheduled intents.
5. **Execution** — narrow drivers that perform approved actions and verification.

```text
                    +------------------+
                    |  Nordfir Engine  |
                    +--------+---------+
                             |
       +---------------------+---------------------+
       |           |             |          |      |
     State       Safety        Energy     Timing Execution
       |           |             |          |      |
   Watchers      Guards      Power data   Windows Drivers
   Snapshot      Authority   Economics    Retry   Verify
                Preflight    Statistics
```

Only the Nordfir Engine combines these signals into an action. Energy logic
cannot directly shut down a node. Timing logic cannot bypass a guard. A driver
cannot decide policy. This keeps authority auditable and prevents subsystems
from fighting each other.

## First implementation milestone

The first end-to-end path should be intentionally narrow:

```text
Observe Linux node
    -> build snapshot
    -> collect/estimate power
    -> receive Economize intent
    -> run guards
    -> check OptimizePower authority
    -> enter REST profile
    -> verify
    -> write audit event
```

Shutdown is not required for this milestone.

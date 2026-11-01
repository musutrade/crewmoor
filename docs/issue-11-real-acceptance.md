# Issue #11 core message acceptance

Batch `crewmoor-issue-11-001`; baseline
`29784e4af3bdcda4cf7fde47bbf6b0b092c91f61`.

Implementation and synthetic tests are in progress. A real Agent exchange is
not yet claimed. The approved plan is one dedicated interactive Claude
dispatcher and one worker on the existing default core, one request and one
active reply through the same CrewMoor entry, coordinated by the existing
team dispatcher. Existing user terminal `t1n`, old HOLD resources and the
external core service must remain intact. No new/updated service or migration
of current TUI sessions is permitted.

Read-only discovery confirmed core `0.1.15`, socket
`/run/user/1000/herdr-core/default.sock`, core id
`4a8f843e64e0a7778943d8a31c25cc26`. Current App version is **unknown**;
no App GUI observation is claimed. Installed Claude is `2.1.316`.

The actual native plain protocol was verified independently by developer and
auditor against version-specific source `4a3591aa835c197f57a5d1b8e8951a75e2a79faa`
and real read-only status/get calls. See [core message implementation](messages.md).
This confirms transport compatibility, not a completed Agent message loop.

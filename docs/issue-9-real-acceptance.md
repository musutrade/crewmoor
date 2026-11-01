# Issue #9 existing-session acceptance

Batch: `crewmoor-issue-9-001`. Baseline:
`dfd2e6e9848f38fb2f236da7c72fd4f892e186a4`.

The user authorized one request and one actively authored reply between the
existing dispatcher and worker in the same Herdr TUI instance. No additional
Agent/model/server is launched. The CLI is Herdr 0.9.5 and the existing server is
0.9.4; no upgrade/restart is performed.

The actual exchange is pending. Automated transport tests are synthetic and do
not count as this acceptance. Final evidence will distinguish native prompt
acceptance, the recipient's actual receipt and active reply, and the
dispatcher's confirmation of receiving that reply. Other Agent combinations,
herdr-core/App and hooks-event collection are outside this live acceptance.

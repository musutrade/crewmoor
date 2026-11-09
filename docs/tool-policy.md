# Role tool rules and independent Hook integration

The scanner is distributed separately as
[`hooks/tool-rule-hook`](../hooks/tool-rule-hook/README.md), program
`tool-rule-hook`. CrewMoor has no Shell/Python scanner implementation or parser
dependencies. The standalone package needs neither CrewMoor nor STS. Its README
covers installation/export, rules, exact exceptions, session control, native host
versions, writable boundaries and limitations.

## New role configuration

Install the independent program at a stable chosen path. Keep rules outside its
package. Roles opt in by selecting paths, without embedding their business rules
in CrewMoor:

```toml
[roles.tool_policy]
checker = "/chosen/prefix/bin/tool-rule-hook"
rules = "/absolute/external/builder-rules.json"
# OpenCode only:
opencode_plugin = "/chosen/fixed-opencode-plugin"
session_store = "/absolute/tool-rule-hook-state"
session_scope = "trusted-server-realm"
```

Relative paths are resolved against that role's `cwd` at start; they never use the
dispatcher's cwd. Generation preserves the provided selection. Start validates
all role selections/configurations before launching, then records resolved paths
and a trusted context snapshot. Changed path selections require a new explicit
launch; running teams are never rewritten. Optional fields are absent on old or
unrestricted roles, which receive no new Hook injection.

`tool-rule-hook validate RULES` is the external configuration validator. Checker
path/executable errors and invalid settings/overrides fail startup. CrewMoor does
not mirror scanner validation. Role names remain arbitrary; no role is named or
assumed to be the release owner.

## Fixed native command and trustworthy identity

For Codex/Claude, the command is the fixed absolute `checker` path plus `hook`.
The rule path and trusted verifier context are provided to the actual Agent's
launch environment as `TOOL_RULE_HOOK_RULES`, `TOOL_RULE_HOOK_VERIFY` and
`TOOL_RULE_HOOK_CONTEXT`. TUI uses native workspace environment injection;
App/core uses direct child argv environment assignments after removing inherited
role markers. This describes the intended launch, not evidence that a host loaded
or ran the Hook. Like the STS fixed plugin, tool rules add no Agent startup or
daemon flags. TUI retains the existing official `agent start` chain and shell
configuration; Core retains its existing direct-executable startup behavior.
Explicit Agent arguments remain operator owned. Verify the actual Hook process
environment for the chosen native CLI/app-server mode; shared daemon/remote
executor role isolation is not established by client variables alone.

CrewMoor's verifier checks its immutable registry context, actual instance marker,
unchanged registration and native session association. A role claim in tool text
is irrelevant. Another session, including an unassociated child, is rejected.
Verifier errors never fall back to independent mode. It returns context metadata
only; the independent checker owns rules, analysis and feedback. The command
`crewmoor tool-policy verify` is an optional adapter, not a standalone dependency.

For Codex 0.167.0, user/project/CLI Hook layers accumulate natively. Within one CLI
layer later same-key overrides replace earlier arrays. With no existing CLI
PreToolUse declaration, CrewMoor adds one. With exactly one supported separated
`-c`/`--config hooks.PreToolUse=ARRAY`, it preserves existing group order and
appends its group at the original argument position. Other overrides are kept.
Repeated/parent-table/child-field/quoted ambiguous declarations, invalid arrays
and conflicting hooks enablement are rejected. Joined argument forms are outside
this entrypoint's supported argument list. Existing groups retain indices and
source; newly added groups and plugin updates still require native trust review.
No trust state is modified. Codex failures/timeouts may fail open, and
`write_stdin` has no second pre-tool check.

Claude 2.1.319 gets one process-local `--settings` file. Existing explicit settings
are read relative to the role cwd, their fields/hooks preserved, and the new
synchronous PreToolUse/Bash group is appended with `onFailure:block`. The same three selected rule/verifier environment values are also set in this
process-local settings document, so an inherited settings.env selection cannot
silently replace the intended startup selection. Other environment fields remain
preserved. Original files and native default sources are retained. Repeated `--settings`, invalid
shapes or explicit settings environment keys conflicting with the new rule
namespace are rejected. Directory trust and tool permissions remain native.

The independent package also exports native Codex/Claude plugin assets, including
a Codex local marketplace catalog. Exporting files is not installation. Native
Codex marketplace/plugin add writes user configuration/cache and enables the
plugin at user scope; it is outside CrewMoor's role launch. No such installation,
user trust mutation or current-team reconfiguration occurs here.
The exported command uses the independent `optional-hook` entry: all three
selection variables absent returns `{}` without granting permission or reading
stdin; any one present invokes the same strict checker. Role-local `hook`
injection stays strict. Partial selection loss still denies; losing all three
variables is indistinguishable from never selecting this plugin. Check actual
host inheritance when deploying a global plugin.

## OpenCode: native session selection

The fixed V2 plugin directory is shared by roles. Its server options contain only
checker, independent store and a trusted persistent `session_scope` realm. It
uses actual `execute.before.sessionID`, calls native `session.get`, verifies
`Info.id`, and passes that session's directory/workspace to Rust. It never chooses
rules from client environment or Agent profile text, never reads messages or
`session.context`, and never automatically follows a parent/fork to a root role.

Rust stores `(opencode, realm, calling session)` separately from rule source,
revision and optional CrewMoor verifier. The controller binds a session only
after receiving and checking native `session.create` data, before first prompt.
CrewMoor calls the external `session-bind` after its existing enrollment path.
The bound verifier supplies the trusted role instance environment; the native
event supplies the actual caller session. The server's shared process environment
does not stand in for either role.

The independent plugin supports different policies in a shared server/cwd,
including explicit `rules:null` unrestricted bindings. **Unbound sessions in its
enabled scope are denied**, as are ended/mismatched locations. A location move
requires an explicit trusted update. Children/new IDs require their own binding.
HTTP public metadata omits workspace ID; CrewMoor's existing local launch path
does not infer remote workspace execution. See the package README for shared
server control and native source boundaries.

CrewMoor reuses its existing resource/session creation topology; this issue adds
no server manager or server just for the Hook. Its existing role launch currently
creates owned native server/session resources. That existing topology is not a
requirement of the independent plugin's shared-server mapping. It does not
silently attach this plugin to an existing user server.

A writer file lock covers revision comparison and atomic publication. Bind cannot
overwrite; update requires expected revision. Each mapping contains an immutable
validated rules snapshot, so editing its source file alone does not change it.
Next tool check reads a whole old/new revision; in-flight checks may finish under
the old one. Restart/resume of the same legitimate realm/session retains it.
Explicit end leaves a closed tombstone; idle/turn completion/uncertain shutdown
never erase bindings. No lifecycle ledger, automatic rebind or scheduler exists.

## Migration and existing teams

Old `roles[].tool_policy` inline entries are no longer accepted for **new starts**;
move them to an external rule file, add absolute `base_dir`, remove the old
`checker` field from that rules document, and select checker/rules as above.
Messages, state hooks and bindings for running teams are not rewritten. An
already-running pinned old binary remains unchanged by this source migration.

The legacy `crewmoor tool-policy hook|bridge SNAPSHOT` entry remains a temporary
thin adapter. It retains the old role/instance/session checks, translates the
inline policy, then forwards to the independent program. The checker must be
selected explicitly through absolute `TOOL_RULE_HOOK_EXECUTABLE`; no sibling/PATH
checker is guessed. Missing program/configuration, bad identity, process errors
or timeout return a native denial. It contains no scanner implementation. Old
native declarations/snapshots are not silently changed. Its removal condition is
the next incompatible release after operators retire/migrate all legacy guarded
teams; it is not an indefinite duplicate implementation.

## Evidence and coexistence

Static tests and synthetic native-shaped events are separate from actual host
loading, native blocking, feedback delivery and later Agent behavior. Installation
and trust require their normal native workflows and appropriate authorization;
this change does not install programs/plugins globally or modify existing STS
state. Other native hooks continue to apply. OpenCode callbacks are ordered and
a prior rejection can short-circuit later hooks; both matching plugins need not
produce two visible reasons. `no_match` is never permission approval.

These are engineering constraints, not a sandbox. Dynamic/indirect execution,
unsupported syntax, subsequent hook transformations, writable rules/context,
TOCTOU, host fail-open and unloaded/untrusted hooks retain their documented
boundaries. True project acceptance remains separately authorized.

# tool-rule-hook

A standalone Rust checker for configured command entries, supported shell wrappers
and literal Python launcher calls. It has no CrewMoor or STS dependency. It borrows
STS 0.2.5's separation of fixed host entrypoints and external session rules; it
implements its own namespace and no delegation/lifecycle ledger. It neither reads
nor changes STS contracts, storage or hooks. No STS source is copied.

## Build, install and remove

Use current stable Rust on Unix. Source/path installation and Cargo source
archives are supported; no registry publication is required.

```sh
cargo build --locked --manifest-path hooks/tool-rule-hook/Cargo.toml
cargo package --locked --manifest-path hooks/tool-rule-hook/Cargo.toml
cargo install --locked --path hooks/tool-rule-hook --root /chosen/prefix --bin tool-rule-hook
/chosen/prefix/bin/tool-rule-hook export-plugin /chosen/fixed-plugin
/chosen/prefix/bin/tool-rule-hook export-marketplace /chosen/fixed-marketplace
/chosen/prefix/bin/tool-rule-hook export-opencode /chosen/fixed-opencode-plugin
```

For an unpacked `.crate`, use its `Cargo.toml` and its directory for `--path`.
`cargo install` installs the binary. Both exporters embed all required assets in
that binary and create a **new** directory, refusing existing destinations. The
Codex/Claude exporter writes their native plugin manifests and hook documents.
Their command is the exporting binary's absolute path followed by `optional-hook`; no
project, role or rule path is embedded in it. The OpenCode exporter writes one
native server plugin directory. All roles reuse those fixed directories and the
same binary. Assets and examples are also included in the source archive,
including the hidden manifest directories.

Update the binary at the chosen stable path through your normal deployment flow;
export a new plugin directory if its declaration/code changes, then use the host's
normal reload/review procedure. Plugin content or declaration changes can require
new trust. Switching external rules is not a guarantee of avoiding trust prompts.
Remove this plugin's host entry through the host's normal configuration, then
`cargo uninstall --root /chosen/prefix tool-rule-hook` and remove only its chosen
export directories when no longer used. Session stores/rules remain operator
owned; do not delete STS or other plugin data. These instructions perform no
installation or user configuration changes automatically.

## Rules and matching

Copy [examples/rules.json](examples/rules.json) outside the package and replace
`base_dir` with the project's absolute directory. Validate before enabling:

```sh
/chosen/prefix/bin/tool-rule-hook validate /absolute/external/rules.json
```

`entries` are denied argv prefixes with a label. Executable paths use their
basename. `allow` contains exact complete argv exceptions extending a configured
entry; changing/adding an argument does not retain that exception. Matching uses
the same normalization for configuration and calls, with configuration paths
based on `base_dir` and call paths based on the native tool cwd. A no-match is
only this checker's result; it grants no host permission and clears no other
hook's denial.

Cargo retains the previous checker's finite global-option normalization. Exact
Cargo test exceptions need a concrete `--test` target or leading test filter;
full-sweep flags and wildcard test targets are rejected. Non-Cargo exceptions
are explicit project declarations: the checker cannot prove that an arbitrary
script/task is focused. Do not add a cosmetic argument and assume it is an
exception; configure the actual task, target and complete arguments.

Python recognizes `python`, `python3` and numbered `python3.N` basenames as
interpreters, while keeping their names distinct for entry matching. Supported
pre-source options are `-u -B -I -E -s -S -O -OO -q -b -bb -v -d`, separate or
attached `-W`/`-X` operands, and `--`. Unknown options remain unresolved; missing
operands fail. Script paths are lexically normalized to absolute paths using the
appropriate cwd. Thus `python3 -B ./tools/check.py full` and an absolute path to
the same configured script match; another directory's same-named script does
not. Symlinks are not resolved. Module execution (`-m`), dynamically supplied
sources and unlisted options are not inferred. Supported source inputs remain
`-c`, script files, stdin redirects, heredoc and here-string. No analyzed command
or script is executed.

npm recognizes `npm test` / `npm run test`, `npm run TASK`, and `--prefix DIR` /
`--prefix=DIR` before the task's `--` separator. Canonical matching preserves the
absolute project directory, task name and arguments after `--`. Different projects
never become the same task by removing `--prefix`. Other npm syntax is unresolved;
package scripts, aliases and dependency toolchains are not expanded.

An ordinary exact `allow` still scans supported inner source. Static Python AST
visits calls in branches/functions even if they might not run. To trust one known
fast path despite that limitation, put the exact Python script argv in **both**
`allow` and `skip_source`. This is the operator's trust declaration for that
invocation, not proof that the fast branch runs. Only that script's source scan
is omitted; extra argv, another script, shell command substitutions and other
commands in the surrounding sequence still undergo their own checks. Shell
wrappers cannot use `skip_source`. Otherwise prefer separate fast/full entry
scripts or explicit literal rules; no arbitrary branch execution is attempted.

`feedback` is either a reason string or `{reason,responsibility?,next?}`. These are
project text, with no built-in CrewMoor role assumptions. The fixed denial status,
matched rule and available selection context remain outside that text. Invalid
configuration, unreadable source, unsupported syntax, missing identity and timeout
produce checker denial; dynamic unrecognized execution may produce `no_match`
with `unresolved:true`.

## Codex and Claude: trusted process selection

Set `TOOL_RULE_HOOK_RULES=/absolute/external/rules.json` only in the actual Agent
process environment. The fixed exported `optional-hook` command reads it on each invocation; a
client's variable does not prove inheritance into a daemon/app-server. Supported
Codex integration is a local `--no-daemon` process (0.167.0); shared-daemon and
remote-executor selection are not supported by this integration. Claude 2.1.319
uses its own process environment and process-local settings/plugin loading.
Claude settings.env can override inherited environment: configure this package's
namespace consistently in the effective process-local settings, and confirm the
actual Hook source/result. CrewMoor places the same selected values in its owned
process settings and rejects conflicts in the explicit input settings. Arbitrary
runtime edits to effective settings remain outside a tamper-resistant boundary.

The global plugin's `optional-hook` entry returns `{}` only when all three
selection variables (`TOOL_RULE_HOOK_RULES`, `TOOL_RULE_HOOK_CONTEXT` and
`TOOL_RULE_HOOK_VERIFY`) are absent. This is this plugin's `no_match`, never a
native permission `allow`; other hooks and native permissions still apply. This
unselected path does not read or validate stdin, so empty/malformed events also
return `{}`. Any variable present, including an empty or non-UTF-8 value, selects
the strict checker. Missing/invalid rules, missing files, invalid input, partial
verifier/context settings, rejected identity and timeouts then deny. Rules that
do not match return the usual `{}`.

The explicit `hook` entry stays strict even with all selection variables absent;
CrewMoor's role-local injection uses that entry. Bare `bridge`, compatibility
envelopes and OpenCode `bridge-session` retain their separate strict contracts.
Environment selection cannot distinguish an intentionally unselected process
from one whose three selection variables were all lost upstream. Verify the
actual executing Hook's environment; a configured client alone is not evidence.

The exported `.codex-plugin/plugin.json` references `hooks/codex-hooks.json`;
Claude discovers `hooks/hooks.json` beside `.claude-plugin/plugin.json`. Install
through the native plugin flow after review. Claude supports session-local
`--plugin-dir /chosen/fixed-plugin`. Codex uses native marketplaces and `plugin
add`; it has no equivalent `--plugin-dir` here. The exported marketplace has
`.agents/plugins/marketplace.json` pointing to `./plugins/tool-rule-hook`, resolved
from the marketplace root. Operator-run native commands are:

```sh
codex plugin marketplace add /chosen/fixed-marketplace --json
codex plugin add tool-rule-hook@tool-rule-hook-local --json
```

These **write the actual user Codex configuration and plugin cache**, enable the
plugin at user scope and can affect other sessions through configuration refresh.
They are not performed by the exporter and are not a zero-user-write test path.
Loaded/trusted plugins can run in other processes at that scope; those processes
use the unselected `optional-hook` behavior above unless a selection variable is present.
They require appropriate installation authority and native trust review. Do not
change `CODEX_HOME`, copy credentials or edit trust to bypass those boundaries.
After stopping this plugin's test work, operator-run `codex plugin remove
tool-rule-hook@tool-rule-hook-local` and marketplace removal can remove its own
registration; native trust leftovers need separate review. Preserve other
registrations and concurrent user edits. Alternatively use the exported
fixed hook declaration with native process overrides as described in the
CrewMoor integration document, preserving existing groups. Never treat merely
having an exported manifest as an installed/enabled/trusted plugin.

Codex user/project/CLI hook layers accumulate. Within one CLI layer repeated
`-c hooks.PreToolUse=...` replaces the earlier array; explicitly append to a
single existing array. Its trust key depends on source/event/group/handler
indices and normalized declaration; plugin content can also be reviewed. Native
trust and managed restrictions still apply. Codex checker launch/output errors
and host timeouts may fail open; `write_stdin` has no second PreToolUse check.
Claude hook declarations use synchronous `onFailure:block`; project-directory
trust, configuration loading and tool permissions are distinct. Print mode's
implicit trust cannot prove interactive first-directory trust.

Process mode explicitly scopes rules to that trusted launch environment. It does
not infer roles or parent/child authority from messages. Launch separate roles
with separate environments. CrewMoor's optional verifier additionally pins its
native session and rejects another session/child; independent users requiring
session pinning must provide their trusted verifier or use the explicit native
session binding path below. Do not claim that inherited environments prove
children have the same role.

Publish process-rule changes atomically through:

```sh
/chosen/prefix/bin/tool-rule-hook rules-update /absolute/external/rules.json /absolute/new-rules.json
```

The source is validated and serialized before atomic replacement. Next hook read
sees a complete old or new document; an in-flight check may use the old one.
Concurrent process-rule publishers use last completed replacement (no CAS).
Direct in-place file edits are outside this atomic-publication guarantee. Rule
path changes need a new correctly configured Agent process; ordinary prompts and
tool inputs do not select/update a rule file. Ending a process does not alter
external rules. A restarted process uses its explicitly supplied environment.

Optional `TOOL_RULE_HOOK_VERIFY` is a JSON argv array for a trusted verifier;
`TOOL_RULE_HOOK_CONTEXT` is its absolute context file. Both must be absent or
both present. The verifier receives `{bridge:false,input:NATIVE_EVENT}` on stdin
and must return `{context:"trusted description"}` with success. Any error,
timeout or malformed response denies; there is no fallback to standalone mode.
This extension is optional and uses no CrewMoor types in the independent package.

## OpenCode 2.2.6: shared-server session bindings

Configure the exported **directory**, not one `.ts` file, in native server
configuration. Preserve all existing `plugins` entries/order:

```json
{"plugins":[{"package":"/chosen/fixed-opencode-plugin","options":{"checker":"/chosen/prefix/bin/tool-rule-hook","store":"/absolute/tool-rule-hook-state","scope":"operator-owned-server-realm"}}]}
```

The operator assigns `scope` to one persistent logical server/DB authority and
supplies the same value to its trusted controller. It is not a fabricated native
server UUID. Reusing a directory, port or bare session ID never proves authority.
Do not reuse that realm/store for an unrelated/imported server or database.
Restart/resume with the same legitimate realm and native session retains its
binding. Store, scope and checker come from trusted plugin configuration; tool
parameters cannot change them. The plugin runs at the server, so TUI environment
variables do not select its policies.

The trusted controller first creates a session through native `POST /api/session`,
checks the actual endpoint and returned `data.id` / `data.location.directory`,
and binds **before the first prompt**. HTTP public location omits `workspaceID`;
this local integration does not infer remote workspace identity. The server
plugin uses `ctx.session.get({sessionID:event.sessionID})`, checks the returned
Info's `id`, and supplies its real directory/workspace to Rust. It reads neither
messages nor `session.context`; parent IDs are facts, never policy inheritance.

```sh
/chosen/prefix/bin/tool-rule-hook session-bind /absolute/tool-rule-hook-state < /absolute/bind.json
```

See [examples/session-bind.json](examples/session-bind.json). The CLI caller is
a trusted controller, not native identity proof by itself: obtain IDs and
location from the verified host connection. A binding snapshots validated rules,
source path, optional verifier and revision 1. Every tool call selects exactly
`(opencode, scope, actual calling session)` and checks bound native location. Shell
cwd defaults to that session's directory; relative paths and `~/` use the native
local Unix convention. Windows/remote execution is unsupported by this scanner.
Later third-party shell hooks may transform input after this hook; those changes
are outside the checked input.

**Unknown sessions are denied throughout the plugin's enabled scope.** For an
unrestricted role in the same location/server, explicitly bind `"rules":null`;
it receives no scanner restriction while location and optional identity checks
still apply. Other sessions must be explicitly bound, including children, forks
and new IDs after restart. No automatic root fallback or parent-role inheritance
exists. Do not silently install this plugin into an existing user's server.

For updates, repeat a full binding selection with `expected_revision:N` through
`session-update STORE`. Rule snapshot + selection are atomically published under
a cross-process writer file lock; comparing the expected revision occurs inside
the same lock. Concurrent updates to one revision have one winner. Binding an
existing ID refuses replacement. Source-file edits alone do not update a session;
use this explicit operation. A hook reads one complete revision, and may finish
with the old revision if already in flight. Stop new work before updating if you
require no in-flight old-policy calls; this package adds no scheduler.

`session-show STORE` reads `{scope,session}` and reports source/revision/status.
`session-end STORE` requires `{scope,session,expected_revision}` and publishes a
closed tombstone. Closed sessions cannot update/rebind/resume; create a new native
session. Idle, turn completion, server shutdown and failed deletion do not erase
bindings. Operator removal of this plugin after all its sessions are stopped may
archive/delete its own entire store; there is no automatic garbage collector.

A binding's optional `verifier` is `{argv:[ABS_PROGRAM,...],context:ABS_FILE,
env:{KEY:VALUE}}`. These controller-provided values are persisted with the
selection. Rust passes only the native event and that configured context/env to
the verifier. Errors deny, including for explicit unrestricted bindings.

## Existing hooks, STS and writable boundaries

Hooks and plugins accumulate according to each host's native rules. This program
never rewrites another hook's results, clears native permissions, or reads/writes
STS state. OpenCode runs hooks in order: an earlier denial can prevent later hooks
from running. If both plugins would deny, the Agent need not see both reasons.
Keep existing order and record which hook actually ran. Native loading/trust,
real refusal, feedback delivery and the Agent's later behavior are separate
acceptance facts; synthetic checker tests do not establish them.

The deployer owns the installed binary/exported plugin. The controller owns the
external rules, store, realm and optional verifier context; the host supplies
actual session/tool/location facts. If the Agent can write those files or alter
the executing process/plugin selection, this is not a tamper-resistant boundary.
No permission changes, privilege escalation or security sandbox is added.

Static analysis cannot cover arbitrary dynamic assembly, imported modules,
encoded execution, unknown launchers or arbitrary binaries. File changes between
scan and execution remain possible. Resource/source limits are 1 MiB and 16
wrapper levels; scan timeout defaults to 3000 ms (1..5000). Context verifier has at
most 1000 ms; OpenCode bridge has a finite outer timeout and turns checker start,
parse, process and timeout failures into tool errors. A missing/unloaded plugin
provides no check. Supported-version source/schema findings do not assert real
host validation; keep host/launch/trust/feedback evidence separately.

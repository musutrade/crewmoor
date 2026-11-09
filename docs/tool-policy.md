# Optional role tool policy

`roles[].tool_policy` checks configured test/coverage commands before a new
role's supported shell tool executes. Omit the field on the submission role,
which continues to run the complete local quality gates. Role names are arbitrary;
there is no built-in `dev`/`release` classification or default command blacklist.
This is independent of `initial_prompt` and the later `message_constraint` reminder.

## Configure a role

The [generation template](../examples/team-tool-policy.toml) has an unrestricted
coordinator/submission role and a restricted builder. Its policy section is:

```toml
[roles.tool_policy]
checker = "/absolute/durable/crewmoor"
feedback = "Full tests and coverage belong to the submission role."
timeout_ms = 3000
allow = [["cargo", "test", "--test", "message", "only_this_test"]]

[[roles.tool_policy.entries]]
argv = ["cargo", "test"]
label = "full Cargo tests"

[[roles.tool_policy.entries]]
argv = ["cargo", "llvm-cov"]
label = "coverage"

[[roles.tool_policy.entries]]
argv = ["make", "full-check"]
label = "project full check"
```

`checker` must be an absolute executable path to a build containing this feature;
CrewMoor does not install or replace it. Generation validates and preserves the
policy without writing Hook settings. Start saves the role configuration and,
after registering the actual role instance, creates private role-local
`tool-policy.json` plus the required native configuration. The snapshot contains
the policy, registered context and registry path. Launch records retain the
derived `tool_guard` paths and intended arguments. Later configuration edits do
not update a running role. Existing teams and bindings are not modified.

Each entry matches a literal argv prefix. Exceptions match a complete argv exactly,
with no wildcards or interpretation of an Agent's request text. Configure executable
basenames (`cargo`, `full.sh`), not paths. Cargo's leading `+toolchain`,
`--locked/--offline/--frozen/--quiet/-q/--verbose/-v` and leading
`--color/--config/--manifest-path/--target-dir` options are normalized before
matching; the latter accept a separate value or `=value`. Other arguments remain
part of the exact comparison. Cargo test exceptions must include a concrete
`--test TARGET` or a leading test-name filter, and cannot include full-sweep
selectors such as `--workspace`, `--all-targets` or `--tests`. Every repeated
`--test TARGET` or `--test=TARGET` selector must be concrete; a valid selector does
not excuse a later wildcard. Arguments after Cargo's `--` belong to the test
harness and are not interpreted as Cargo target selectors. An extra arbitrary
flag does not grant an exception. An allowed wrapper still has its nested command
checked against the same rules.

For this example, `cargo test`, `cargo test --nocapture`, `cargo llvm-cov` and
`make full-check` are denied. Only the configured complete targeted command is
exempt; `cargo +stable test --test message only_this_test` normalizes to that
command. Known project aliases/tasks/scripts can be added as entries, for example
`argv = ["full.sh"]` or `["cargo", "my-full-alias"]`. Unlisted aliases are outside
the scanner's guarantees.

Policy strings are nonblank, have at most 16384 UTF-8 bytes, and contain no NUL.
`timeout_ms` is 1–5000 (default 3000). Source and incoming Hook JSON are separately
limited to 1 MiB; wrapper recursion is limited to 16. Configuration and derived
settings are private files and may retain explicitly supplied native settings;
keep them private and do not put secrets in policy text.

## Native entry and identity

The implementation uses these verified host versions and native entry points:

| Agent | Role-local entry | Failure boundary |
|---|---|---|
| Codex CLI 0.167.0 | Process `-c features.hooks=true` and inline `hooks.PreToolUse`, synchronous `^Bash$` command | A valid deny blocks; native start failure, timeout, invalid output or disabled/untrusted Hook can fail open. `write_stdin` has no second PreToolUse check. |
| Claude Code 2.1.319 | A process-only `--settings` file with synchronous `PreToolUse/^Bash$`, `onFailure: "block"` | Native command failures/timeouts/schema failures use that version's blocking failure option. Disabled/unloaded Hook configuration is not evidence of protection. |
| OpenCode v2.2.6 | The authorized V2 `ctx.tool.hook("execute.before")` bridge in the role's private native server | Rejected callback becomes native `Tool.Error` before `executeTool`; bridge start/exit/invalid-output/7-second timeout failures reject too. An unloaded plugin is not protection. |

Codex uses its native Hook trust review. This feature does not bypass trust or
write trust state. Hooks from native configuration layers remain subject to the
host's normal merging and managed policy. For Claude, a supplied `--settings`
JSON file/string is read and merged into a new role-local file; relative file
paths resolve from that role's startup `config.cwd`, while absolute paths and
inline JSON retain their meaning. The dispatcher cwd and later Hook execution
cwd are not used as that base. Existing settings
and Hook entries remain, the original file is unchanged, and other settings
sources are not disabled. Only the generated settings argument replaces that
role's explicit settings argument.

OpenCode's private server/TUI command receives an in-memory
`OPENCODE_CONFIG_CONTENT` overlay containing an absolute local plugin directory
and checker/snapshot options. Existing JSON content and plugin entries are
preserved; malformed/non-object content or a non-array plugin list is rejected.
No project/global plugin is installed, no project cwd file is edited, and the
configuration content is not copied into the persisted launch environment.
The V2 bridge only transports input and turns checker failures into rejected
Promises. Its native v2.2.6 adapter converts those errors to `Tool.Error`; all
command parsing, policy and exceptions remain Rust. The narrower permission
ask/reply API is not used as a substitute: saved approvals and missing original
input can bypass that API's observation path.

Every check verifies the snapshot against the registered context, actual
`CREWMOOR_FEEDBACK_INSTANCE`, Agent kind, unchanged native association and incoming
native session ID. Codex/Claude use their existing SessionStart association;
OpenCode uses the existing `session.create`/enrollment path. The role does not come
from a command, event profile name or self-reported text. Missing association,
changed context/session or mismatched instance requests denial. Current tool cwd
only resolves relative sources; it does not reassign the role. OpenCode's missing
`workdir` uses the owned main session's initial registered directory; arbitrary
later external changes to that native session location are not tracked.

The native command entry is `crewmoor tool-policy hook SNAPSHOT`; it reads one
Hook JSON input on stdin. Denials emit the native `PreToolUse` JSON
`permissionDecision: "deny"` and reason. No match emits `{}`, which grants no
permission and preserves the host's other checks. The bridge entry uses
`tool-policy bridge SNAPSHOT` and returns `deny` or `no_match` plus a detected
`unresolved` flag. No-match never means safe. Missing/invalid configuration,
identity, consumed input fields or supported source parsing/read failures produce
a valid denial response instead of an ordinary nonzero checker exit. The bounded
Rust scan also returns a denial on timeout; it never executes the inspected script.

## Static source coverage

Rust uses the Bash grammar AST, shell word decoding and the RustPython AST.
It examines executable command nodes, not text appearing in comments or data.
Lists/tuples of literal Python argv share the direct-command rules; dynamic argv
components cannot match an exact exception, while a known blocked prefix still
matches before such a component.

| Input | Supported analysis |
|---|---|
| Shell | Simple commands, assignments, literal quoting (including ANSI-C quoted segments and static concatenation), `;`, `&&`, `||`, pipelines, subshells and command substitutions exposed by the Bash AST. |
| Wrappers | `env` assignments, `-i`, `-u NAME`, `--`; `command -p`/`--`; `exec -a NAME`, `-c`, `-l`, `--`; `nohup`/`--`; shell option-prefix `-c`/`-lc`/`-ec`, `-l/-e`, and `-s` with available source. Interpreter option scanning stops at a script argument or `--`; later script arguments are data. `command -v/-V` is a lookup. Other wrapper options are unresolved. |
| Python source | `python`/`python3*` `-c` (including attached literal source), readable UTF-8 files, `-`/implicit stdin via literal heredoc, here-string or file redirection. Individual `-u/-B/-I/-E/-s/-S` flags are recognized. |
| Python calls | By default `subprocess.run/call/check_call/check_output/Popen` and `os.system/popen`; first positional argument or `args=`. String, AST list and AST tuple literal forms are handled. `python_calls` can replace the complete dotted-name list for other known first-argument launchers. |

Python literal strings at configured launch calls are analyzed as command strings;
the scanner does not execute Python or infer its runtime `shell=`/name-shadowing
semantics. Static possible calls in branches/functions may be inspected even if
they would not run. Plain lists, documentation strings, comments, `print` data
and unrelated Python calls do not become commands.

Configured Python launch calls retain a literal `cwd=` as that child call's local
source-resolution directory, including nested shell/Python file reads; sibling
calls resume the parent directory. `cwd=None` inherits it. Dynamic cwd and
`**kwargs` leave the child directory unresolved, so a required relative source
is rejected instead of being read from the parent directory. This does not
interpret arbitrary in-program working-directory mutations.

ANSI-C literal decoding supports the documented ASCII/control, octal/hex and
Unicode escape forms. NUL truncation, non-UTF8 byte escapes and unrecognized
escape forms request a checking failure rather than guessing a resolved argv.

Relative Python file reads after an in-command `cd` are rejected as unresolved;
set the tool working directory instead. Multiple stdin redirects are unsupported
and request denial. Unreadable/non-UTF8 files, malformed or
unsupported source and exceeded source/depth budgets request denial for the
supported source path. Dynamic configured Python calls and unsupported interpreter
modes are recorded as unresolved no-match, not as confirmed safe. Detection of
`unresolved` is not exhaustive for arbitrary programs.

## Feedback and limits

The denial reason includes the actual role, configured entry label (or checker
failure), configured explanation, before-execution refusal and next step: full
checks belong to the submission role, do not switch wrappers/languages to bypass,
and run only necessary explicitly allowed targeted tests. It returns directly to
the invoking Agent through the native Hook/tool error, not only to a dispatcher
or local log. The "not executed" reason describes a denial honored by the loaded,
supported native entry; checker stdout alone does not prove that fact.

This is an engineering constraint rather than a sandbox. Dynamic concatenation,
imported/indirect calls, encoded commands, unlisted binaries/aliases/languages,
unsupported tools, alternate execution paths and future native schema changes
are not guaranteed to be detected. Python files can change after scanning
(TOCTOU). Codex interactive `write_stdin`, remote execution file resolution,
untrusted/disabled Hooks, unloaded plugins, process crashes and host fail-open
behavior remain explicit limits. No filesystem sandbox, child-process takeover,
retry service, custom state protocol or blanket Python ban is added.

CrewMoor does not persist command/script bodies or tool input. Its response uses
only role, entry label and concise error/feedback. A native host may independently
include the original command in its own rejection message or logs.

`tests/tool_policy.rs` covers Rust source analysis, native-shaped checker I/O,
fixed identity, exact exceptions, settings preservation, timeout and failure
responses. Team/generation tests cover role-local propagation and omission on
other roles; the library overlay test covers OpenCode content preservation.
These tests are simulations, not actual host Hook calls, plugin loading or Agent
behavior. Actual Hook refusal, actual Agent receipt and subsequent behavior must
be recorded separately for each Agent/runtime combination. No existing team is
claimed to be mechanically blocked by merely building or injecting configuration.

Native references: [Codex Hooks](https://learn.chatgpt.com/docs/hooks),
[Claude Hook failure control](https://code.claude.com/docs/en/hooks#block-the-action-when-a-hook-fails),
[OpenCode V2 server plugins](https://opencode.ai/docs/build/plugins/server),
[OpenCode plugin configuration](https://opencode.ai/docs/plugins/).
Fixed source: Codex `086ccc833bc9cf01024ad2dd6af7dbd421b6bf9a`
([PreToolUse](https://github.com/openai/codex/blob/086ccc833bc9cf01024ad2dd6af7dbd421b6bf9a/codex-rs/hooks/src/events/pre_tool_use.rs));
OpenCode `dc2ffcf8bf26f9e669b6aa22b95f50e12be15e06`
([server environment](https://github.com/anomalyco/opencode/blob/dc2ffcf8bf26f9e669b6aa22b95f50e12be15e06/packages/cli/src/server-process.ts),
[Promise error conversion](https://github.com/anomalyco/opencode/blob/dc2ffcf8bf26f9e669b6aa22b95f50e12be15e06/packages/plugin/src/promise/adapter.ts)).

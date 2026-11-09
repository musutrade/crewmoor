// OpenCode v2.2.6 execute.before bridge. All policy decisions are made in Rust.
import { Plugin } from "@opencode/plugin"
import { spawn } from "node:child_process"

function check(checker: string, store: string, scope: string, event: unknown): Promise<void> {
  return new Promise((resolve, reject) => {
    const child = spawn(checker, ["bridge-session", store, scope], { stdio: ["pipe", "pipe", "pipe"] })
    let output = ""
    const timer = setTimeout(() => {
      child.kill("SIGKILL")
      reject(new Error("Tool rule hook timed out; refusing this tool call. Do not bypass with another wrapper."))
    }, 7000)
    const fail = (message: string) => {
      clearTimeout(timer)
      child.kill("SIGKILL")
      // V2.2.6 converts this Promise rejection to native Tool.Error before executeTool.
      reject(new Error(message))
    }
    child.on("error", () => fail("Tool rule hook could not start; refusing this tool call."))
    child.stdin.on("error", () => fail("Tool rule hook input failed; refusing this tool call."))
    child.stdout.on("data", (chunk) => {
      output += chunk.toString()
      if (output.length > 262144) {
        child.kill("SIGKILL")
        fail("Tool rule hook output invalid; refusing this tool call.")
      }
    })
    child.stderr.resume()
    child.on("close", (code) => {
      clearTimeout(timer)
      try {
        const result = JSON.parse(output)
        if (code !== 0 || result === null || typeof result !== "object" || typeof result.unresolved !== "boolean" || !["deny", "no_match"].includes(result.decision)) throw new Error("invalid checker result")
        if (result.decision === "deny") {
          if (typeof result.reason !== "string" || !result.reason.trim()) throw new Error("missing denial reason")
          fail(result.reason)
          return
        }
        if (result.reason !== null) throw new Error("invalid no-match reason")
        resolve()
      } catch {
        fail("Tool rule hook failed or returned invalid output; refusing this tool call. Do not bypass with another wrapper.")
      }
    })
    child.stdin.end(JSON.stringify(event))
  })
}

export default Plugin.define({
  id: "tool-rule-hook",
  async setup(ctx) {
    const { checker, store, scope } = ctx.options
    if (typeof checker !== "string" || typeof store !== "string" || typeof scope !== "string" || !scope.trim()) throw new Error("Tool rule hook configuration missing")
    await ctx.tool.hook("execute.before", async (event) => {
      const session = await ctx.session.get({ sessionID: event.sessionID })
      if (session.id !== event.sessionID) throw new Error("Tool rule hook native session mismatch")
      await check(checker, store, scope, {
        ...event,
        directory: session.location.directory,
        workspace: session.location.workspaceID ?? null,
        parentID: session.parentID ?? null,
      })
    })
  },
})

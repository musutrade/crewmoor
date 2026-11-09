// OpenCode v2.2.6 execute.before bridge. All policy decisions are made in Rust.
import { Plugin } from "@opencode/plugin"
import { spawn } from "node:child_process"

function check(checker: string, snapshot: string, event: unknown): Promise<void> {
  return new Promise((resolve, reject) => {
    const child = spawn(checker, ["tool-policy", "bridge", snapshot], { stdio: ["pipe", "pipe", "pipe"] })
    let output = ""
    const timer = setTimeout(() => {
      child.kill("SIGKILL")
      reject(new Error("CrewMoor role checker timed out; tool not executed. Do not bypass with another wrapper."))
    }, 7000)
    const fail = (message: string) => {
      clearTimeout(timer)
      child.kill("SIGKILL")
      // V2.2.6 converts this Promise rejection to native Tool.Error before executeTool.
      reject(new Error(message))
    }
    child.on("error", () => fail("CrewMoor role checker could not start; tool not executed."))
    child.stdin.on("error", () => fail("CrewMoor role checker input failed; tool not executed."))
    child.stdout.on("data", (chunk) => {
      output += chunk.toString()
      if (output.length > 262144) {
        child.kill("SIGKILL")
        fail("CrewMoor role checker output invalid; tool not executed.")
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
        fail("CrewMoor role checker failed or returned invalid output; tool not executed. Do not bypass with another wrapper.")
      }
    })
    child.stdin.end(JSON.stringify(event))
  })
}

export default Plugin.define({
  id: "crewmoor.role-tool-policy",
  async setup(ctx) {
    const { checker, snapshot } = ctx.options
    if (typeof checker !== "string" || typeof snapshot !== "string") throw new Error("CrewMoor role checker configuration missing")
    await ctx.tool.hook("execute.before", (event) => check(checker, snapshot, {
      ...event,
      instance: process.env.CREWMOOR_FEEDBACK_INSTANCE,
    }))
  },
})

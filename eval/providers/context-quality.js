/**
 * Context Quality Evaluation Provider for Promptfoo
 *
 * Exports a provider object with id() and callApi() methods.
 * Simulates the Knocode context engine behavior.
 */

/**
 * Mock context engine (simulates Knocode context building)
 */
function mockContextEngine(vars) {
  const task = vars.task || "";
  const max_tokens = vars.max_tokens || 12000;

  // Parse comma-separated strings
  const filesStr = vars.files_mentioned || "";
  const skillsStr = vars.skills_matched || "";
  const knowledgeStr = vars.knowledge_entries || "";

  const files = filesStr ? filesStr.split(",").map(s => s.trim()).filter(Boolean) : [];
  const skills = skillsStr ? skillsStr.split(",").map(s => {
    const [name, score] = s.split(":");
    return { name: name?.trim(), score: parseFloat(score) || 0.5 };
  }) : [];
  const knowledge = knowledgeStr ? knowledgeStr.split(",").map(s => {
    const [key, value] = s.split(":");
    return { key: key?.trim(), value: value?.trim() };
  }) : [];

  // Build mock context based on inputs
  const behavioral_skills = skills
    .filter(s => s.name)
    .map(s => `# ${s.name}\nScore: ${s.score}`)
    .join("\n\n");

  const docs_context = knowledge
    .filter(k => k.key)
    .map(k => `// ${k.key}: ${k.value}`)
    .join("\n");

  const code_context = files
    .map(f => `// ${f}\n// [file content]`)
    .join("\n\n");

  // Estimate tokens (rough: 1 token ≈ 4 chars)
  const total_content = behavioral_skills.length + docs_context.length + code_context.length;
  const total_tokens = Math.floor(total_content / 4);

  // Enforce budget
  const budget_remaining = Math.max(0, max_tokens - total_tokens);

  // Determine section ordering
  const has_skills = behavioral_skills.length > 0;
  const has_docs = docs_context.length > 0;
  const has_code = code_context.length > 0;

  return {
    behavioral_skills: behavioral_skills || "",
    docs_context: docs_context || "",
    code_context: code_context || "",
    token_usage: {
      total_tokens: Math.min(total_tokens, max_tokens),
      budget_remaining,
      by_source: {
        behavioral_skills: Math.floor(behavioral_skills.length / 4),
        docs_context: Math.floor(docs_context.length / 4),
        code_context: Math.floor(code_context.length / 4),
      },
    },
    // Metadata for assertions
    metadata: {
      has_skills,
      has_docs,
      has_code,
      task_length: task.length,
    },
  };
}

/**
 * Promptfoo provider — FIRST-CLASS v0.5.0: hits BuildContext via UDS MessagePack (length-prefix + rmp-serde)
 * Fallback: mockContextEngine only inside catch (fail-open).
 */
const net = require("net");
const fs = require("fs");
const path = require("path");

async function callBuildContextUDS(prompt, timeoutMs = 2000) {
  const socketPath = process.env.KNOCODE_SOCKET || "/tmp/knocode.sock";
  if (!fs.existsSync(socketPath)) throw new Error("UDS not found");
  return new Promise((resolve, reject) => {
    const socket = net.createConnection(socketPath);
    const timer = setTimeout(() => { socket.destroy(); reject(new Error("UDS timeout")); }, timeoutMs);
    socket.on("connect", () => {
      try {
        const msgpack = require("msgpack-lite");
        const req = { correlation_id: `req_eval_${Date.now()}`, hook_type: "PreGeneration", payload: { type: "MessageRewrite", session_id: "eval", message: prompt } };
        const body = msgpack.encode(req);
        const header = Buffer.alloc(4); header.writeUInt32BE(body.length, 0);
        socket.write(Buffer.concat([header, body]));
      } catch (e) { clearTimeout(timer); reject(e); }
    });
    let buf = Buffer.alloc(0);
    socket.on("data", (chunk) => { buf = Buffer.concat([buf, chunk]); if (buf.length >= 4) { const len = buf.readUInt32BE(0); if (buf.length >= 4+len) { clearTimeout(timer); try { const msgpack = require("msgpack-lite"); const resp = msgpack.decode(buf.slice(4, 4+len)); socket.destroy(); resolve(resp); } catch (e) { socket.destroy(); reject(e); } } } });
    socket.on("error", (e) => { clearTimeout(timer); reject(e); });
  });
}

module.exports = class ContextQualityProvider {
  id() {
    return "context-quality";
  }

  label = "Context Quality (UDS first-class v0.5.0)";

  async callApi(prompt, context) {
    // FIRST-CLASS: try UDS BuildContext
    try {
      const resp = await callBuildContextUDS(prompt);
      if (resp && resp.payload && resp.payload.type === "RewrittenMessage") {
        const pack = resp.payload.context_pack || {};
        return { output: JSON.stringify({ behavioral_skills: pack.behavioral_skills || "", docs_context: pack.docs_context || "", code_context: pack.code_context || "", token_usage: pack.token_usage || {}, routing: resp.payload.routing_decision || {}, source: "uds" }, null, 2) };
      }
    } catch (e) {
      console.warn(`[context-quality] UDS primary failed (${e.message}), fallback to mock`);
    }
    // FALLBACK only on Err
    const vars = context?.vars || {};
    const result = mockContextEngine(vars);
    result.source = "mock-fallback";
    return { output: JSON.stringify(result, null, 2) };
  }
};

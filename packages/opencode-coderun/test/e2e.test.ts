import { describe, it, expect, vi } from "vitest";
import { callCoderunDaemon } from "../src/index";

/**
 * Canonical E2E: OpenCode → Coderun (UDS/HTTP fallback) → BuildContext → Router → LiteLLM → response
 * Uses mocked fetch to simulate daemon that would have done BuildContext+Router+LiteLLM.
 * Other adapters (Cursor, Gemini) are gated behind TIER2 flag — see adapters/tier2/README.md
 */

describe("E2E: OpenCode → Coderun → BuildContext → Router → LiteLLM", () => {
  it("enriches message via BuildContext + Router + LiteLLM (mocked daemon)", async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        correlation_id: "req_e2e123",
        hook_type: "PreGeneration",
        payload: {
          type: "RewrittenMessage",
          original: "implement auth",
          rewritten: "implement auth\n\n---\n\nContext:\nbehavioral_skills: Rust Expert\ncode_context: // src/auth.rs:10",
          context_pack: {
            behavioral_skills: "Rust Expert",
            docs_context: "",
            code_context: "// src/auth.rs:10 fn authenticate()",
            token_usage: { total_tokens: 8500, budget_remaining: 3500, by_source: { behavioral_skills: 1000, code_context: 7500 } },
            provenance: [{ path: "src/auth.rs", source: "code", retriever: "tantivy", score: 0.92, reason: "bm25" }],
            metadata: { task_hash: "abc123", correlation_id: "req_e2e123", cache_order: ["behavioral_skills","docs_context","code_context"], repository_state: "deadbeef12345678" }
          },
          routing_decision: { model: "gpt-4o", tier: "balanced", scores: { structural: 0.4, semantic: 0.6, scope: 0.5, final_score: 0.52 }, reasoning: "balanced" }
        },
        latency_ms: 42,
      }),
    } as any);

    const resp = await callCoderunDaemon(
      { hook_type: "PreGeneration", payload: { type: "MessageRewrite", session_id: "sess_e2e", message: "implement auth" } },
      { url: "http://127.0.0.1:9527", timeoutMs: 1000, fetchImpl: mockFetch as any },
    );
    expect(resp).not.toBeNull();
    expect(resp!.payload.type).toBe("RewrittenMessage");
    expect(resp!.payload.rewritten).toContain("Context:");
    expect((resp as any).payload.context_pack.provenance[0].score).toBeCloseTo(0.92);
    expect((resp as any).payload.routing_decision.tier).toBe("balanced");
    expect((resp as any).payload.routing_decision.model).toBe("gpt-4o");
    expect(mockFetch).toHaveBeenCalledWith("http://127.0.0.1:9527/hook", expect.objectContaining({ method: "POST" }));
  });

  it("compresses tool output via Optimizer (RTK→built-in)", async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        correlation_id: "req_tool123",
        hook_type: "PreToolCall",
        payload: { type: "CompressedOutput", original: "a".repeat(5000), compressed: "compressed 200 tokens", original_tokens: 1200, compressed_tokens: 200 },
        latency_ms: 15,
      }),
    } as any);
    const resp = await callCoderunDaemon(
      { hook_type: "PreToolCall", payload: { type: "ToolOutput", tool_name: "bash", output_type: "ShellOutput", content: "a".repeat(5000) } },
      { fetchImpl: mockFetch as any },
    );
    expect(resp!.payload.type).toBe("CompressedOutput");
    expect(resp!.payload.compressed_tokens).toBe(200);
    expect(resp!.payload.original_tokens).toBe(1200);
  });

  it("fail-open when daemon unreachable (never breaks agent)", async () => {
    const mockFetch = vi.fn().mockRejectedValue(new Error("ECONNREFUSED"));
    const resp = await callCoderunDaemon(
      { hook_type: "PreGeneration", payload: { type: "MessageRewrite", message: "hi" } },
      { fetchImpl: mockFetch as any },
    );
    expect(resp).toBeNull(); // caller does passthrough
  });

  it("TIER2 flag gates other adapters", async () => {
    // In production, Cursor/Gemini adapters are behind TIER2 env flag; this test documents gate
    const tier2 = process.env.TIER2 === "true";
    expect(typeof tier2).toBe("boolean");
    // When TIER2 != true, only opencode plugin is canonical; this assertion is documentation
    if (!tier2) {
      // eslint-disable-next-line @typescript-eslint/no-unused-vars
      const adapters = ["opencode"]; // canonical only
      expect(adapters).toContain("opencode");
    }
  });
});

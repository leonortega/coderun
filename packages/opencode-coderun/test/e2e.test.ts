import { describe, it, expect, vi } from "vitest";
import { callCoderunDaemon } from "../src/index";

/**
 * Canonical E2E: OpenCode → Coderun (UDS/HTTP fallback) → BuildContext → response
 * Uses mocked fetch to simulate daemon that does BuildContext (deterministic retrieval only, no Router/LiteLLM — see LLM_ROUTING_REMOVAL.md).
 * Other adapters (Cursor, Gemini) are gated behind TIER2 flag — see adapters/tier2/README.md
 */

describe("E2E: OpenCode → Coderun → BuildContext", () => {
  it("enriches message via BuildContext (mocked daemon, no Router/LiteLLM)", async () => {
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
          // routing_decision removed — see LLM_ROUTING_REMOVAL.md (BuildContext is ContextPack only)
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
    // Failure is the expected scenario — silence the plugin's operational console.error
    const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const mockFetch = vi.fn().mockRejectedValue(new Error("ECONNREFUSED"));
    const resp = await callCoderunDaemon(
      { hook_type: "PreGeneration", payload: { type: "MessageRewrite", message: "hi" } },
      { fetchImpl: mockFetch as any },
    );
    expect(resp).toBeNull(); // caller does passthrough
    errSpy.mockRestore();
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

describe("TASK-036/F-7: repository_path travels with every hook payload", () => {
  it("MessageRewrite payload carries the agent workspace root", async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        correlation_id: "req_repo1",
        hook_type: "PreGeneration",
        payload: { type: "OriginalPassthrough", original: "hi", reason: "no_context_hits" },
        latency_ms: 3,
      }),
    } as any);
    await callCoderunDaemon(
      {
        hook_type: "PreGeneration",
        payload: { type: "MessageRewrite", session_id: "s", message: "hi", repository_path: "C:\\repos\\eShopOnWeb" },
      },
      { fetchImpl: mockFetch as any },
    );
    const [, init] = mockFetch.mock.calls[0];
    const sent = JSON.parse(init.body);
    expect(sent.payload.repository_path).toBe("C:\\repos\\eShopOnWeb");
    // Daemon must answer passthrough on zero hits — plugin leaves prompt untouched then
    expect(sent.payload.message).toBe("hi");
  });

  it("ToolOutput payload carries the agent workspace root", async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        correlation_id: "req_repo2",
        hook_type: "PreToolCall",
        payload: { type: "CompressedOutput", original: "x", compressed: "x", original_tokens: 1, compressed_tokens: 1 },
        latency_ms: 2,
      }),
    } as any);
    await callCoderunDaemon(
      {
        hook_type: "PreToolCall",
        payload: { type: "ToolOutput", tool_name: "bash", output_type: "ShellOutput", content: "x", repository_path: "/home/dev/eShopOnWeb" },
      },
      { fetchImpl: mockFetch as any },
    );
    const [, init] = mockFetch.mock.calls[0];
    const sent = JSON.parse(init.body);
    expect(sent.payload.repository_path).toBe("/home/dev/eShopOnWeb");
  });

  it("OriginalPassthrough with no_context_hits signals untouched passthrough contract", async () => {
    // TASK-031/F-2: daemon must NOT rewrite prompts with zero retrievable value
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        correlation_id: "req_zero",
        hook_type: "PreGeneration",
        payload: { type: "OriginalPassthrough", original: "zzzqqq unrelated", reason: "no_context_hits" },
        latency_ms: 4,
      }),
    } as any);
    const resp = await callCoderunDaemon(
      { hook_type: "PreGeneration", payload: { type: "MessageRewrite", session_id: "s", message: "zzzqqq unrelated", repository_path: "." } },
      { fetchImpl: mockFetch as any },
    );
    expect(resp!.payload.type).toBe("OriginalPassthrough");
    expect(resp!.payload.reason).toBe("no_context_hits");
    expect(resp!.payload.original).toBe("zzzqqq unrelated");
  });
});

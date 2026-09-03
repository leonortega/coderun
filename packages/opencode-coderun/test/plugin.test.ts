import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  getOutputType,
  getDaemonUrl,
  getTimeoutMs,
  getReadyTimeoutMs,
  waitForDaemonReady,
  callCoderunDaemon,
  mcpCall,
  requestContextEnrichment,
  requestOutputCompression,
  DEFAULT_DAEMON_URL,
  DEFAULT_TIMEOUT_MS,
  DEFAULT_READY_TIMEOUT_MS,
} from "../src/index";

describe("getOutputType", () => {
  it("maps read variants to FileRead", () => {
    expect(getOutputType("read")).toBe("FileRead");
    expect(getOutputType("READ")).toBe("FileRead");
    expect(getOutputType("readfile")).toBe("FileRead");
  });

  it("maps search variants", () => {
    expect(getOutputType("grep")).toBe("SearchResult");
    expect(getOutputType("search")).toBe("SearchResult");
  });

  it("maps shell variants", () => {
    expect(getOutputType("bash")).toBe("ShellOutput");
    expect(getOutputType("shell")).toBe("ShellOutput");
    expect(getOutputType("exec")).toBe("ShellOutput");
  });

  it("defaults to Other", () => {
    expect(getOutputType("unknown_tool")).toBe("Other");
    expect(getOutputType("mytool")).toBe("Other");
  });
});

describe("getDaemonUrl / getTimeoutMs", () => {
  const origEnv = { ...process.env };

  afterEach(() => {
    process.env = { ...origEnv };
  });

  it("returns default when env not set", () => {
    delete process.env.CODERUN_DAEMON_URL;
    expect(getDaemonUrl()).toBe(DEFAULT_DAEMON_URL);
  });

  it("respects CODERUN_DAEMON_URL", () => {
    process.env.CODERUN_DAEMON_URL = "http://example:9999";
    expect(getDaemonUrl()).toBe("http://example:9999");
  });

  it("returns default timeout when not set", () => {
    delete process.env.CODERUN_TIMEOUT_MS;
    expect(getTimeoutMs()).toBe(DEFAULT_TIMEOUT_MS);
  });

  it("parses CODERUN_TIMEOUT_MS", () => {
    process.env.CODERUN_TIMEOUT_MS = "5000";
    expect(getTimeoutMs()).toBe(5000);
  });

  it("falls back on invalid timeout", () => {
    process.env.CODERUN_TIMEOUT_MS = "not-a-number";
    expect(getTimeoutMs()).toBe(DEFAULT_TIMEOUT_MS);
  });
});

describe("waitForDaemonReady", () => {
  beforeEach(() => vi.restoreAllMocks());

  const healthUrl = "http://127.0.0.1:9527/health";
  const okJson = (state: string | undefined) =>
    Promise.resolve({
      ok: true,
      status: 200,
      json: async () => ({ status: "ok", state, index_files: 42 }),
    } as any);

  it("returns true immediately when /health reports ready", async () => {
    const mockFetch = vi.fn().mockImplementation(async () => okJson("ready"));
    const ready = await waitForDaemonReady({
      url: "http://127.0.0.1:9527",
      timeoutMs: 1000,
      fetchImpl: mockFetch as any,
    });
    expect(ready).toBe(true);
    expect(mockFetch).toHaveBeenCalledTimes(1);
    expect(mockFetch).toHaveBeenCalledWith(healthUrl, expect.objectContaining({ signal: expect.anything() }));
  });

  it("treats a 200 without a parseable state as ready (live daemon)", async () => {
    const mockFetch = vi.fn().mockImplementation(async () => okJson(undefined));
    const ready = await waitForDaemonReady({
      url: "http://127.0.0.1:9527",
      timeoutMs: 1000,
      fetchImpl: mockFetch as any,
    });
    expect(ready).toBe(true);
  });

  it("treats a 200 with a non-JSON body as ready", async () => {
    const mockFetch = vi.fn().mockImplementation(async () => ({
      ok: true,
      status: 200,
      json: async () => {
        throw new Error("not json");
      },
    } as any));
    const ready = await waitForDaemonReady({
      url: "http://127.0.0.1:9527",
      timeoutMs: 1000,
      fetchImpl: mockFetch as any,
    });
    expect(ready).toBe(true);
  });

  it("polls while indexing and returns true once ready", async () => {
    const responses = [okJson("indexing"), okJson("indexing"), okJson("ready")];
    const mockFetch = vi.fn().mockImplementation(async () => responses.shift());
    const ready = await waitForDaemonReady({
      url: "http://127.0.0.1:9527",
      timeoutMs: 1000,
      pollMs: 10,
      fetchImpl: mockFetch as any,
    });
    expect(ready).toBe(true);
    expect(mockFetch).toHaveBeenCalledTimes(3);
  });

  it("returns false when the budget expires while still indexing", async () => {
    const mockFetch = vi.fn().mockImplementation(async () => okJson("indexing"));
    const ready = await waitForDaemonReady({
      url: "http://127.0.0.1:9527",
      timeoutMs: 60,
      pollMs: 10,
      fetchImpl: mockFetch as any,
    });
    expect(ready).toBe(false);
    // ~6 polls in 60ms — proves it did not hang past the deadline
    expect(mockFetch.mock.calls.length).toBeGreaterThanOrEqual(4);
    expect(mockFetch.mock.calls.length).toBeLessThanOrEqual(10);
  });

  it("returns false fast when the daemon is unreachable (connection refused)", async () => {
    const mockFetch = vi.fn().mockRejectedValue(new Error("ECONNREFUSED"));
    const t0 = Date.now();
    const ready = await waitForDaemonReady({
      url: "http://127.0.0.1:9527",
      timeoutMs: 10_000, // generous budget: must NOT be burned on a down daemon
      fetchImpl: mockFetch as any,
    });
    expect(ready).toBe(false);
    expect(Date.now() - t0).toBeLessThan(500);
    expect(mockFetch).toHaveBeenCalledTimes(1);
  });
});

describe("getReadyTimeoutMs", () => {
  const origEnv = { ...process.env };

  afterEach(() => {
    process.env = { ...origEnv };
  });

  it("returns default when env not set", () => {
    delete process.env.CODERUN_READY_TIMEOUT_MS;
    expect(getReadyTimeoutMs()).toBe(DEFAULT_READY_TIMEOUT_MS);
  });

  it("parses CODERUN_READY_TIMEOUT_MS", () => {
    process.env.CODERUN_READY_TIMEOUT_MS = "5000";
    expect(getReadyTimeoutMs()).toBe(5000);
  });

  it("falls back on invalid value", () => {
    process.env.CODERUN_READY_TIMEOUT_MS = "nope";
    expect(getReadyTimeoutMs()).toBe(DEFAULT_READY_TIMEOUT_MS);
  });
});

describe("callCoderunDaemon", () => {
  beforeEach(() => vi.restoreAllMocks());

  it("returns parsed json on success", async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        correlation_id: "req_123",
        hook_type: "PreGeneration",
        payload: { type: "RewrittenMessage", rewritten: "hello enriched" },
        latency_ms: 42,
      }),
    } as any);

    const res = await callCoderunDaemon(
      { hook_type: "PreGeneration", payload: { type: "MessageRewrite", message: "hello" } },
      { url: "http://127.0.0.1:9527", timeoutMs: 1000, fetchImpl: mockFetch as any },
    );

    expect(mockFetch).toHaveBeenCalledWith(
      "http://127.0.0.1:9527/hook",
      expect.objectContaining({ method: "POST" }),
    );
    expect(res?.payload.rewritten).toBe("hello enriched");
  });

  it("returns null on non-ok response", async () => {
    // Failure is the expected scenario — silence the plugin's operational console.error
    const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const mockFetch = vi.fn().mockResolvedValue({ ok: false, status: 500 } as any);
    const res = await callCoderunDaemon(
      { hook_type: "PreGeneration", payload: { type: "MessageRewrite", message: "hi" } },
      { fetchImpl: mockFetch as any },
    );
    expect(res).toBeNull();
    expect(errSpy).toHaveBeenCalled(); // fail-open logging still exercised
    errSpy.mockRestore();
  });

  it("returns null on fetch throw (fail-open)", async () => {
    const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const mockFetch = vi.fn().mockRejectedValue(new Error("ECONNREFUSED"));
    const res = await callCoderunDaemon(
      { hook_type: "PreGeneration", payload: { type: "MessageRewrite", message: "hi" } },
      { fetchImpl: mockFetch as any },
    );
    expect(res).toBeNull();
    errSpy.mockRestore();
  });

  it("sends correct body for tool compression", async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        correlation_id: "r2",
        hook_type: "PreToolCall",
        payload: { type: "CompressedOutput", compressed: "compressed..." },
        latency_ms: 10,
      }),
    } as any);

    await callCoderunDaemon(
      {
        hook_type: "PreToolCall",
        payload: { type: "ToolOutput", tool_name: "read", output_type: "FileRead", content: "big content" },
      },
      { fetchImpl: mockFetch as any },
    );

    const body = JSON.parse(mockFetch.mock.calls[0][1].body);
    expect(body.payload.tool_name).toBe("read");
    expect(body.payload.output_type).toBe("FileRead");
  });
});

describe("CoderunPlugin", () => {
  it("exposes expected hooks", async () => {
    const { CoderunPlugin } = await import("../src/index");
    const hooks = await CoderunPlugin({
      project: {},
      client: { app: { log: async () => {} } },
      $: null as any,
      directory: "/tmp",
      worktree: "/tmp",
    } as any);

    expect(hooks).toHaveProperty("chat.message");
    expect(hooks).toHaveProperty("message.updated");
    expect(hooks).toHaveProperty("tool.execute.before");
  });
});

describe("mcpCall (JSON-RPC over POST /mcp)", () => {
  beforeEach(() => vi.restoreAllMocks());

  const mcpUrl = "http://127.0.0.1:9527/mcp";
  const okResponse = (result: any) =>
    Promise.resolve({ ok: true, status: 200, json: async () => ({ jsonrpc: "2.0", id: 1, result }) } as any);

  it("returns result on success and posts a JSON-RPC envelope", async () => {
    const mockFetch = vi.fn().mockImplementation(async () => okResponse({ content: [{ type: "text", text: "ctx" }] }));
    const out = await mcpCall("tools/call", { name: "coderun_context", arguments: { prompt: "hi" } }, {
      url: "http://127.0.0.1:9527",
      timeoutMs: 1000,
      fetchImpl: mockFetch as any,
    });
    expect(out.kind).toBe("ok");
    if (out.kind === "ok") expect(out.result.content[0].text).toBe("ctx");
    const [, init] = mockFetch.mock.calls[0];
    const body = JSON.parse(init.body);
    expect(body.jsonrpc).toBe("2.0");
    expect(body.method).toBe("tools/call");
    expect(body.id).toBeGreaterThan(0);
    expect(mockFetch).toHaveBeenCalledWith(mcpUrl, expect.objectContaining({ method: "POST" }));
  });

  it("reports unsupported on 404 (legacy daemon without /mcp)", async () => {
    const mockFetch = vi.fn().mockResolvedValue({ ok: false, status: 404 } as any);
    const out = await mcpCall("ping", {}, { url: "http://127.0.0.1:9527", fetchImpl: mockFetch as any });
    expect(out.kind).toBe("unsupported");
  });

  it("reports application JSON-RPC errors (e.g. -32001 daemon_indexing)", async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ jsonrpc: "2.0", id: 1, error: { code: -32001, message: "daemon_indexing" } }),
    } as any);
    const out = await mcpCall("tools/call", {}, { url: "http://127.0.0.1:9527", fetchImpl: mockFetch as any });
    expect(out.kind).toBe("error");
    if (out.kind === "error") expect(out.code).toBe(-32001);
  });

  it("fails open on network error", async () => {
    const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const mockFetch = vi.fn().mockRejectedValue(new Error("ECONNREFUSED"));
    const out = await mcpCall("ping", {}, { url: "http://127.0.0.1:9527", fetchImpl: mockFetch as any });
    expect(out.kind).toBe("failure");
    errSpy.mockRestore();
  });
});

describe("requestContextEnrichment (MCP coderun_context + /hook fallback)", () => {
  beforeEach(() => vi.restoreAllMocks());

  const mcpToolResult = (text: string, structured: any = {}, isError = false) =>
    Promise.resolve({
      ok: true,
      status: 200,
      json: async () => ({
        jsonrpc: "2.0",
        id: 1,
        result: { content: [{ type: "text", text }], structuredContent: structured, isError },
      }),
    } as any);

  it("returns the enriched text from coderun_context", async () => {
    const mockFetch = vi.fn().mockImplementation(async () =>
      mcpToolResult("implement auth\n\n---\n\nContext:\ncode_context: auth", { type: "context", passthrough: false }),
    );
    const enriched = await requestContextEnrichment("implement auth", "/repo", "sess", {
      url: "http://127.0.0.1:9527",
      timeoutMs: 1000,
      fetchImpl: mockFetch as any,
    });
    expect(enriched).toContain("Context:");
    const [, init] = mockFetch.mock.calls[0];
    const body = JSON.parse(init.body);
    expect(body.params.name).toBe("coderun_context");
    expect(body.params.arguments.repository_path).toBe("/repo");
  });

  it("returns null on daemon passthrough (zero context hits)", async () => {
    const mockFetch = vi.fn().mockImplementation(async () =>
      mcpToolResult("unrelated", { type: "context", passthrough: true, reason: "no_context_hits" }),
    );
    const enriched = await requestContextEnrichment("unrelated", "/repo", undefined, {
      fetchImpl: mockFetch as any,
    });
    expect(enriched).toBeNull();
    expect(mockFetch).toHaveBeenCalledTimes(1); // no /hook fallback on MCP passthrough
  });

  it("returns null on -32001 indexing error and does NOT fall back to /hook", async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ jsonrpc: "2.0", id: 1, error: { code: -32001, message: "daemon_indexing" } }),
    } as any);
    const enriched = await requestContextEnrichment("hi", "/repo", undefined, { fetchImpl: mockFetch as any });
    expect(enriched).toBeNull();
    expect(mockFetch).toHaveBeenCalledTimes(1);
  });

  it("falls back to the /hook rewrite on a legacy daemon (404 /mcp)", async () => {
    const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const mockFetch = vi
      .fn()
      .mockResolvedValueOnce({ ok: false, status: 404 } as any)
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({
          correlation_id: "r",
          hook_type: "PreGeneration",
          payload: { type: "RewrittenMessage", rewritten: "legacy enriched auth" },
          latency_ms: 5,
        }),
      } as any);
    const enriched = await requestContextEnrichment("implement auth", "/repo", "sess", {
      fetchImpl: mockFetch as any,
    });
    expect(enriched).toBe("legacy enriched auth");
    expect(mockFetch).toHaveBeenCalledTimes(2);
    expect(mockFetch.mock.calls[1][0]).toContain("/hook");
    errSpy.mockRestore();
  });
});

describe("requestOutputCompression (MCP coderun_compress + /hook fallback)", () => {
  beforeEach(() => vi.restoreAllMocks());

  it("returns compressed text + token counts via MCP", async () => {
    const mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        jsonrpc: "2.0",
        id: 1,
        result: {
          content: [{ type: "text", text: "compressed..." }],
          structuredContent: { type: "compress", original_tokens: 1200, compressed_tokens: 200 },
          isError: false,
        },
      }),
    } as any);
    const res = await requestOutputCompression("a".repeat(5000), "bash", "/repo", {
      fetchImpl: mockFetch as any,
    });
    expect(res?.compressed).toBe("compressed...");
    expect(res?.originalTokens).toBe(1200);
    const [, init] = mockFetch.mock.calls[0];
    const body = JSON.parse(init.body);
    expect(body.params.name).toBe("coderun_compress");
    expect(body.params.arguments.output_type).toBe("ShellOutput");
  });

  it("falls back to the /hook ToolOutput contract on legacy daemons", async () => {
    const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const mockFetch = vi
      .fn()
      .mockResolvedValueOnce({ ok: false, status: 404 } as any)
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({
          correlation_id: "r2",
          hook_type: "PreToolCall",
          payload: { type: "CompressedOutput", original: "x", compressed: "legacy compressed", original_tokens: 100, compressed_tokens: 30 },
          latency_ms: 3,
        }),
      } as any);
    const res = await requestOutputCompression("x", "read", "/repo", { fetchImpl: mockFetch as any });
    expect(res?.compressed).toBe("legacy compressed");
    expect(mockFetch.mock.calls[1][0]).toContain("/hook");
    errSpy.mockRestore();
  });

  it("returns null when unreachable (fail-open)", async () => {
    const errSpy = vi.spyOn(console, "error").mockImplementation(() => {});
    const mockFetch = vi.fn().mockRejectedValue(new Error("ECONNREFUSED"));
    const res = await requestOutputCompression("x", "bash", "/repo", { fetchImpl: mockFetch as any });
    expect(res).toBeNull();
    errSpy.mockRestore();
  });
});

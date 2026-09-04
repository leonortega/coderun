import { describe, it, expect, vi, afterEach } from "vitest";
import {
  getDaemonUrl,
  getTimeoutMs,
  mcpCall,
  requestContextEnrichment,
} from "../src/daemon";

const URL = "http://127.0.0.1:9527";

const jsonResponse = (body: any) =>
  ({ ok: true, status: 200, json: async () => body } as any);

describe("getDaemonUrl / getTimeoutMs", () => {
  const orig = { ...process.env };
  afterEach(() => {
    process.env = { ...orig };
  });

  it("uses the default daemon URL", () => {
    delete process.env.KNOCODE_DAEMON_URL;
    expect(getDaemonUrl()).toBe("http://127.0.0.1:9527");
  });

  it("respects KNOCODE_DAEMON_URL", () => {
    process.env.KNOCODE_DAEMON_URL = "http://example:9999";
    expect(getDaemonUrl()).toBe("http://example:9999");
  });

  it("parses KNOCODE_TIMEOUT_MS and falls back on invalid", () => {
    process.env.KNOCODE_TIMEOUT_MS = "5000";
    expect(getTimeoutMs()).toBe(5000);
    process.env.KNOCODE_TIMEOUT_MS = "nope";
    expect(getTimeoutMs()).toBe(30_000);
  });
});

describe("mcpCall", () => {
  afterEach(() => vi.restoreAllMocks());

  it("returns the result on success (knocode_context)", async () => {
    const mockFetch = vi.fn().mockResolvedValue(
      jsonResponse({ jsonrpc: "2.0", id: 1, result: { content: [{ type: "text", text: "ctx" }] } }),
    );
    const out = await mcpCall(
      "tools/call",
      { name: "knocode_context", arguments: { prompt: "hi" } },
      { url: URL, timeoutMs: 1000, fetchImpl: mockFetch as any },
    );
    expect(out.kind).toBe("ok");
    if (out.kind === "ok") {
      expect(out.result.content[0].text).toBe("ctx");
    }
    const body = JSON.parse(mockFetch.mock.calls[0][1].body);
    expect(body.method).toBe("tools/call");
    expect(body.params.name).toBe("knocode_context");
  });

  it("returns an error envelope for JSON-RPC application errors (e.g. -32001 indexing)", async () => {
    const mockFetch = vi.fn().mockResolvedValue(
      jsonResponse({ jsonrpc: "2.0", id: 1, error: { code: -32001, message: "daemon_indexing" } }),
    );
    const out = await mcpCall("tools/call", {}, { url: URL, timeoutMs: 1000, fetchImpl: mockFetch as any });
    expect(out.kind).toBe("error");
  });

  it("is unsupported for legacy daemons (404 /mcp)", async () => {
    const mockFetch = vi.fn().mockResolvedValue({ ok: false, status: 404 } as any);
    const out = await mcpCall("tools/call", {}, { url: URL, timeoutMs: 1000, fetchImpl: mockFetch as any });
    expect(out.kind).toBe("unsupported");
  });

  it("fails open when unreachable", async () => {
    const mockFetch = vi.fn().mockRejectedValue(new Error("ECONNREFUSED"));
    const out = await mcpCall("tools/call", {}, { url: URL, timeoutMs: 1000, fetchImpl: mockFetch as any });
    expect(out.kind).toBe("failure");
  });
});

describe("requestContextEnrichment", () => {
  afterEach(() => vi.restoreAllMocks());

  it("returns context text on success and forwards repository_path", async () => {
    const mockFetch = vi.fn().mockResolvedValue(
      jsonResponse({ jsonrpc: "2.0", id: 1, result: { content: [{ type: "text", text: "repo digest" }], structuredContent: {}, isError: false } }),
    );
    const text = await requestContextEnrichment("implement auth", "C:/repo", {
      url: URL,
      timeoutMs: 1000,
      fetchImpl: mockFetch as any,
    });
    expect(text).toBe("repo digest");
    const body = JSON.parse(mockFetch.mock.calls[0][1].body);
    expect(body.params.arguments.repository_path).toBe("C:/repo");
  });

  it("returns null when the daemon is unreachable (fail-open)", async () => {
    const mockFetch = vi.fn().mockRejectedValue(new Error("ECONNREFUSED"));
    const text = await requestContextEnrichment("hi", "C:/repo", {
      url: URL,
      timeoutMs: 1000,
      fetchImpl: mockFetch as any,
    });
    expect(text).toBeNull();
  });

  it("returns null on passthrough (zero context hits)", async () => {
    const mockFetch = vi.fn().mockResolvedValue(
      jsonResponse({ jsonrpc: "2.0", id: 1, result: { content: [{ type: "text", text: "x" }], structuredContent: { passthrough: true }, isError: false } }),
    );
    const text = await requestContextEnrichment("hi", "C:/repo", {
      url: URL,
      timeoutMs: 1000,
      fetchImpl: mockFetch as any,
    });
    expect(text).toBeNull();
  });

  it("returns null when the daemon is mid-index (-32001)", async () => {
    const mockFetch = vi.fn().mockResolvedValue(
      jsonResponse({ jsonrpc: "2.0", id: 1, error: { code: -32001, message: "daemon_indexing" } }),
    );
    const text = await requestContextEnrichment("hi", "C:/repo", {
      url: URL,
      timeoutMs: 1000,
      fetchImpl: mockFetch as any,
    });
    expect(text).toBeNull();
  });
});
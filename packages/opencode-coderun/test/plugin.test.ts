import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import {
  getOutputType,
  getDaemonUrl,
  getTimeoutMs,
  callCoderunDaemon,
  DEFAULT_DAEMON_URL,
  DEFAULT_TIMEOUT_MS,
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

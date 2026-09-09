import { describe, expect, it } from "vitest";
import { QueryClient } from "@tanstack/react-query";
import {
  DEFAULT_STALE_MS,
  STREAM_STALE_MS,
  applyQueryDefaults,
  staleTimeFor,
} from "./queryDefaults";

describe("stale time by key", () => {
  it("a session key never goes stale on its own", () => {
    for (const key of [["me"], ["health"], ["deployment"]]) {
      expect(staleTimeFor(key)).toBe(Infinity);
    }
  });

  it("a key the event stream keeps fresh stays fresh for the stream window", () => {
    expect(staleTimeFor(["graph", "kb-1", "focus", 200])).toBe(STREAM_STALE_MS);
    expect(staleTimeFor(["documents", "kb-1"])).toBe(STREAM_STALE_MS);
  });

  it("a key with nothing behind it goes stale within the short default", () => {
    expect(staleTimeFor(["tokens"])).toBe(DEFAULT_STALE_MS);
    // 前缀匹配是按元素比的：reviewSummary 不是 review
    expect(staleTimeFor(["reviewSummary", "kb-1"])).toBe(DEFAULT_STALE_MS);
    expect(staleTimeFor([42])).toBe(DEFAULT_STALE_MS);
  });

  it("the table is what the client actually applies", () => {
    const client = new QueryClient({
      defaultOptions: { queries: { staleTime: DEFAULT_STALE_MS } },
    });
    applyQueryDefaults(client);
    expect(client.getQueryDefaults(["me"]).staleTime).toBe(Infinity);
    expect(client.getQueryDefaults(["review", "kb-1", "queue", 2]).staleTime).toBe(
      STREAM_STALE_MS,
    );
    expect(client.getQueryDefaults(["reviewSummary", "kb-1"]).staleTime).toBeUndefined();
    expect(client.getDefaultOptions().queries?.staleTime).toBe(DEFAULT_STALE_MS);
  });
});

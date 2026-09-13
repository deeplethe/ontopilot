import { describe, expect, it } from "vitest";
import type { Source } from "./api";
import { answeredWithoutSources, type Turn } from "./liveAnswer";

const answer = (over: Partial<Turn> = {}): Turn => ({
  role: "assistant",
  content: "以下是我找到的内容",
  ...over,
});
const source = { n: 1, kind: "chunk" } as unknown as Source;

describe("answeredWithoutSources (#547)", () => {
  it("marks a finished answer with no sources", () => {
    expect(answeredWithoutSources(answer({ sources: [] }), false)).toBe(true);
  });

  it("does not mark an answer that cites something", () => {
    expect(answeredWithoutSources(answer({ sources: [source] }), false)).toBe(false);
  });

  it("waits for the stream to finish before deciding", () => {
    expect(answeredWithoutSources(answer(), true)).toBe(false);
    expect(answeredWithoutSources(answer(), false)).toBe(true);
  });

  it("marks a replayed turn, whose empty sources are stored as undefined", () => {
    expect(answeredWithoutSources(answer({ sources: undefined }), false)).toBe(true);
  });

  it("leaves user turns and bare errors alone", () => {
    expect(answeredWithoutSources({ role: "user", content: "hi" }, false)).toBe(false);
    expect(answeredWithoutSources(answer({ content: "", error: "boom" }), false)).toBe(false);
    expect(answeredWithoutSources(answer({ error: "stopped" }), false)).toBe(true);
  });
});

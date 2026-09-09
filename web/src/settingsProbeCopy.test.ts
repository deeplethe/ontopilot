import { describe, expect, it } from "vitest";
import { en } from "./i18n/en";
import { zh } from "./i18n/zh";

const EMBEDDING_DIMENSION = 1024;
const PROBE_REPLY = "OK";

describe("model connectivity result copy", () => {
  it("says the successful probes establish reachability and authentication", () => {
    expect(en.settings.ok(PROBE_REPLY)).toBe(
      `Reachable and authenticated (${PROBE_REPLY})`,
    );
    expect(en.settings.okDim(EMBEDDING_DIMENSION)).toBe(
      `Reachable and authenticated (dim ${EMBEDDING_DIMENSION})`,
    );
    expect(zh.settings.ok(PROBE_REPLY)).toBe(
      `已连通并通过认证（${PROBE_REPLY}）`,
    );
    expect(zh.settings.okDim(EMBEDDING_DIMENSION)).toBe(
      `已连通并通过认证（维度 ${EMBEDDING_DIMENSION}）`,
    );
  });
});

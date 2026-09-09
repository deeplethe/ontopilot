import { describe, expect, it } from "vitest";

import { dsSpecs } from "./Settings";

/** 每一档引擎的连接串拼装。
 *
 * 这里只钉一条性质：**一格都没填的时候不许抛**。
 * #573 报的就是这个——刚选中「连接串」那一档、还没来得及输入，
 * `build` 就在 `undefined` 上调了 `.trim()`，整页换成错误屏。
 * 其余几档当时没炸，只因为它们用模板串把 `undefined` 安静地拼了进去。
 * 所以断言对所有档一起做，新加引擎时自动落进这张网。 */
describe("dsSpecs", () => {
  it("空表单不抛", () => {
    for (const spec of dsSpecs()) {
      expect(() => spec.build({}), `${spec.id} 在空表单上抛了`).not.toThrow();
    }
  });

  it("空表单只填了一部分也不抛", () => {
    for (const spec of dsSpecs()) {
      const half = Object.fromEntries(
        spec.fields.slice(0, 1).map((f) => [f.key, "x"]),
      );
      expect(() => spec.build(half), `${spec.id} 在半填表单上抛了`).not.toThrow();
    }
  });

  it("连接串那一档原样返回去掉首尾空白的输入", () => {
    const raw = dsSpecs().find((s) => s.id === "raw");
    expect(raw).toBeDefined();
    expect(raw!.build({ conn: "  postgres://u@h/db  " })).toBe("postgres://u@h/db");
  });
});

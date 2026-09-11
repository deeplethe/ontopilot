import { describe, expect, it } from "vitest";
import { mix } from "./graphVisuals";

/* 混色的两端都可能是令牌读回来的 `rgb(...)`，不只是 `#rrggbb`。
   认不出的那一端从前会悄悄变成中灰——节点那圈环就是这么发闷的。 */
describe("mix", () => {
  it("mixes two hex colours", () => {
    expect(mix("#000000", "#ffffff", 0.5)).toBe("rgb(128,128,128)");
  });

  it("takes rgb() on either end, the way a token reads back", () => {
    // 红 → 白，一半：认得出 rgba 才会是 255,128,128；认不出就退成 192,64,64
    expect(mix("#ff0000", "rgba(255,255,255,1)", 0.5)).toBe("rgb(255,128,128)");
    expect(mix("rgb(255,0,0)", "#ffffff", 0.5)).toBe("rgb(255,128,128)");
  });

  it("mixes toward ink on paper, not toward grey", () => {
    // 浅色下 INK 是 rgb(23,23,23)：绿往墨里走三成，绿该变暗而不是发灰
    const ring = mix("#4ea172", "rgb(23,23,23)", 0.35);
    expect(ring).toBe("rgb(59,113,82)");
  });
});

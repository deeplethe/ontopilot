import { describe, expect, it } from "vitest";
import { mix } from "./graphVisuals";

/* 混色的两端都可能是令牌读回来的 `rgb(...)`，不只是 `#rrggbb`。
   认不出的那一端从前会悄悄变成中灰——节点那圈环就是这么发闷的。 */
describe("mix", () => {
  it("mixes two hex colours", () => {
    expect(mix("#000000", "#ffffff", 0.5)).toBe("rgb(128,128,128)");
  });

  /* **短写法是构建器压出来的，不是谁手写的。**令牌里写的是 `#ffffff`，
     Lightning CSS 打包时压成 `#fff`，读回来就是三位。只认六位的解析器在这里
     会退回中灰——而 `mix(白, 类型色, t)` 混的一端成了灰，浅色下每个节点就变成
     一块灰疙瘩。dev 不压缩，所以只有打包产物发作，看着像主题的毛病。 */
  it("takes the short hex a minifier leaves behind", () => {
    expect(mix("#fff", "#000", 0.5)).toBe("rgb(128,128,128)");
    // 认不出会退成中灰，于是白混白也还是 128——这一条正是要挡住它
    expect(mix("#fff", "#fff", 0.5)).toBe("rgb(255,255,255)");
    expect(mix("#f00", "#ffffff", 0.5)).toBe("rgb(255,128,128)");
  });

  it("takes hex with alpha on either end", () => {
    expect(mix("#ffffffff", "#000000", 0.5)).toBe("rgb(128,128,128)");
    expect(mix("#fff8", "#ffffff", 0)).toBe("rgb(255,255,255)");
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

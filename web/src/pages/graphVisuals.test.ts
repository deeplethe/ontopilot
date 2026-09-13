import { afterEach, describe, expect, it } from "vitest";
import { flattenOnLight, mix } from "./graphVisuals";

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

/* 浅色下 sigma 的边着色器是**加法**混合（ONE, ONE_MINUS_SRC_ALPHA，而且不预乘
   RGB）：半透明压不暗一条线，只会把它加到纸上。所以边色必须在交给 sigma 之前
   按底色摊平成不透明的 rgb。
   这一组要摸 document（主题标记 + 底色令牌），而这个测试台按房规不起 DOM
   （vite.config.ts：纯逻辑单测）。两个最小替身就够——要测的是「读到什么、
   算出什么」，不是浏览器。 */
describe("flattenOnLight", () => {
  const stub = (theme: string, ground = "250,250,250") => {
    const g = globalThis as Record<string, unknown>;
    g.document = { documentElement: { dataset: { theme } } };
    g.getComputedStyle = () => ({
      getPropertyValue: (n: string) => (n === "--u-ground-rgb" ? ground : ""),
    });
  };
  afterEach(() => {
    const g = globalThis as Record<string, unknown>;
    delete g.document;
    delete g.getComputedStyle;
  });

  it("leaves a colour alone outside the light theme", () => {
    stub("dark");
    expect(flattenOnLight("rgba(176,120,20,0.6)")).toBe("rgba(176,120,20,0.6)");
  });

  it("flattens rgba against the paper", () => {
    stub("light");
    expect(flattenOnLight("rgba(176,120,20,0.6)")).toBe("rgb(206,172,112)");
  });

  /* **压缩之后的写法也要认。**令牌写的是 rgba(176,120,20,0.6)，打包器压成
     #b0781499；只认 rgba(...) 的话它整个漏过去，那条派生边就被加成一道亮黄。 */
  it("flattens the hex-with-alpha a minifier leaves behind", () => {
    stub("light");
    expect(flattenOnLight("#b0781499")).toBe("rgb(206,172,112)");
    expect(flattenOnLight("#1717175c")).not.toContain("#");
  });

  it("leaves an opaque colour as it is", () => {
    stub("light");
    expect(flattenOnLight("#e4e4e4")).toBe("#e4e4e4");
    expect(flattenOnLight("rgb(20,20,20)")).toBe("rgb(20,20,20)");
  });
});

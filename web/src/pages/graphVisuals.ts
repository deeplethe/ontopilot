// 节点视觉：色彩数学 + 四层壳配色 + 胶囊标签/悬浮卡 + 世界坐标网格。
// 从 Graph.tsx 抽出——这几样是「怎么画一个带类型色的节点」，不含任何实例图
// 语义（没有 derived/contested/temporal 这些概念），本体模式图原样复用，
// 图谱与模式图因此长得像同一个引擎画的，而不是各自一套视觉识别。
//
// **边的画法没有抽到这里**：/graph 的边携带派生/争议/时态这些实例图独有的
// 状态，模式图的边是 subClassOf/relation/disjoint 三种本体语义，两者的
// 「边意味着什么」根本不是同一件事，抽成共用只会把两套语义焊在一起。
import type Sigma from "sigma";

/* 画布调色板 —— 结构取自 Semantica GraphWorkspace 源码；基色已中性化：
   Semantica 原版是钢蓝系（#0B1320/#5A7A9E/#7A92AE），按"chrome 零色偏、
   彩色只属于数据"的既定原则换成同明度纯灰，类型色混入比例不变 */
export let NODE_SHELL_BASE = "#121212"; // 节点外壳深底（原 #0B1320 的中性化）
export let NODE_CORE_BASE = "#767676"; // 节点核心灰（原 #5A7A9E 的中性化）
export let NODE_BORDER_BASE = "#909090"; // 节点描边（原 #7A92AE 的中性化）
/* 配方的**比例两个主题共用**，随主题变的只有上面那三个底色。节点永远是
   「壳 + 彩心」：壳跟着纸走（深色 #121212，浅色 #ffffff），类型色只按 14%
   渗进壳里，核心收 50% ——于是两个主题里中间那一点都是这个类自己的颜色，
   变的只是它坐在黑底上还是白底上。 */
export const NODE_TINT_MIX = 0.14; // 类型色混入外壳的比例
export const NODE_CORE_MIX = 0.5; // 类型色混入核心的比例
/* 状态环取**节点自己的类型色**，不是写死的色相。往白里混而不是直接用原色：
   环画在节点自己身上，同色同亮度就看不出是个环。**悬停混得更白、选中混得
   更少**——悬停时全图不压暗，环要在一片乱线里立刻跳出来；选中时其余都
   压暗了，节点本来就孤立着，这时候环该说的是「它是谁」，所以更贴近它自己
   的颜色。 */
export const RING_HOVER_MIX = 0.7; // 悬停：偏白，为的是跳出来
export const RING_SELECT_MIX = 0.35; // 选中：偏本色，为的是认得出
export const TRANSPARENT = "rgba(0,0,0,0)";
export let MUTED_SHELL = "#151515";
/* 悬停时其余的压暗程度。**比选中轻**（选中是压到底）：悬停是随鼠标走的、
   每划过一个节点就换一次，压到底会让整张画布不停明灭 */
export const HOVER_MUTE = 0.78;
export let PILL_BG = "rgba(12,12,12,0.9)";
/** 指到的那一块底。静止那档是 `PILL_BG`，选中那档反色，见 `drawNodeLabel` */
export let PILL_BG_HOVER = "rgba(42,42,42,0.96)";
/** 选中那一档的底（反色）。**不等于 `PILL_TEXT`**：纸底上的墨是近黑的 */
export let PILL_INVERT = "#ededed";
export let PILL_BORDER = "rgba(255,255,255,0.14)"; // --u-line-strong
export let PILL_TEXT = "#ededed"; // --u-text
/* 裸字的光晕：与画布同色（--u-ground）的一圈描边，只为把从字底下穿过的
   连线压住。不是阴影——阴影会在一片细线里糊成一团脏 */
export let LABEL_HALO = "rgba(10,10,10,0.92)";
/** 浮层的面，抄 `.u-pop`（tooltip / toast 用的那一档近实底） */
export let POP_BG = "rgba(16,16,16,0.98)";

/* 画布上的字与界面同一套刻度。**canvas 读不到 CSS 变量**，所以这里镜像一份
   `styles.css` 的值——它是源头，改那边记得回来改这里。
   从前这几个数是自己长出来的（节点 11、边 9、类型行 10、字色 #e5e5e5 /
   #a1a1a1）：字号整体上移一档之后，画布成了全站唯一还在用旧刻度的地方，
   而 9px 比界面里最小的字还小一半 */
export const CANVAS_FONT = '"Geist", "Inter", "Noto Sans SC", sans-serif';
export let CANVAS_TEXT = "#ededed"; // --u-text
export let CANVAS_TEXT_2 = "#a8a8a8"; // --u-text-2
/* 画在节点与连线之间的字比界面的底再小一档（11）。**画布不是界面**：
   这些字压在一片线和点上，与它们比邻的是 5–13px 的节点，不是页面上的正文；
   12 在这里显得比它标注的东西还重。浮在画布之上的悬浮卡不算——那是 tooltip，
   走界面的刻度（见 CANVAS_TITLE_SIZE / CANVAS_META_SIZE） */
export const CANVAS_LABEL_SIZE = 11;
export const CANVAS_TITLE_SIZE = 14; // --text-body
export const CANVAS_META_SIZE = 12; // --text-fine

/** `#rgb` / `#rgba` / `#rrggbb` / `#rrggbbaa` 都收。
 *
 * **短写法是必须收的，不是顺手**：这些值是从 CSS 令牌读回来的，而构建时
 * Lightning CSS 会把 `#ffffff` 压成 `#fff`。从前这里只认六位，于是浅色主题
 * 的白色令牌全部落到下面那句兜底上——静静地变成中灰 128,128,128。画布上
 * 看到的就是「浅色下每个节点都是一块灰疙瘩」：节点外壳的底色本该是纸白，
 * 读成了中灰，四层配方一层塌了。dev 下 CSS 不压缩，所以只有打包产物发作。 */
export function hexToRgb(hex: string): [number, number, number] {
  const [r, g, b] = hexToRgba(hex);
  return [r, g, b];
}

/** 同上，连 alpha 一起。八位写法的最后两位是 alpha；没写就是 1 */
export function hexToRgba(hex: string): [number, number, number, number] {
  const m = /^#?([0-9a-f]{3,8})$/i.exec(hex.trim());
  if (!m) return [128, 128, 128, 1];
  let h = m[1];
  // 短写法每一位翻倍：#1a2 → #11aa22，#1a2f → #11aa22ff
  if (h.length === 3 || h.length === 4) h = [...h].map((c) => c + c).join("");
  if (h.length !== 6 && h.length !== 8) return [128, 128, 128, 1];
  const v = parseInt(h.slice(0, 6), 16);
  const a = h.length === 8 ? parseInt(h.slice(6), 16) / 255 : 1;
  return [(v >> 16) & 255, (v >> 8) & 255, v & 255, a];
}

/** c1 向 c2 按 t 比例混色 */
/** 两色按比例混。**两端都走 `parseRgba`，不是 `hexToRgb`**：令牌读回来的值
 *  是 `rgb(23,23,23)` 这种写法，用只认 `#rrggbb` 的解析器会静静地退回中灰
 *  （`hexToRgb` 认不出就返回 128,128,128），于是 `mix(类型色, INK, t)` 混的
 *  不是墨色而是一团灰——选中与悬停的那圈环因此在两个主题里都发闷。
 *  从前 `INK` 是字面量 "#ffffff" 才没露馅，0038 把它改成从令牌读之后才显出来。 */
export function mix(c1: string, c2: string, t: number): string {
  const [r1, g1, b1] = parseRgba(c1);
  const [r2, g2, b2] = parseRgba(c2);
  const f = (a: number, b: number) => Math.round(a + (b - a) * t);
  return `rgb(${f(r1, r2)},${f(g1, g2)},${f(b1, b2)})`;
}

/** rgb / rgba / #hex 都收，按 t 从 from 渐到 to，**alpha 也一起渐**。
 *
 * 与 `mix` 分工：那个只吃 hex、只管把类型色按比例调进壳色（节点的配方）；
 * 这个要处理边的 `rgba(...)` 与淡入淡出，两边都得能解析、alpha 不能丢 */
function parseRgba(c: string): [number, number, number, number] {
  if (c.startsWith("#")) return hexToRgba(c);
  const m = c.match(
    /rgba?\(\s*([\d.]+)[,\s]+([\d.]+)[,\s]+([\d.]+)(?:[,\s/]+([\d.]+))?/,
  );
  if (!m) return [128, 128, 128, 1];
  return [+m[1], +m[2], +m[3], m[4] !== undefined ? +m[4] : 1];
}

export function lerpColor(from: string, to: string, t: number): string {
  const a = parseRgba(from);
  const b = parseRgba(to);
  const f = (i: number) => a[i] + (b[i] - a[i]) * t;
  return `rgba(${Math.round(f(0))},${Math.round(f(1))},${Math.round(f(2))},${f(3).toFixed(3)})`;
}

/* 节点标签：**平时是一行裸字，指到或选中的那一个才补一块底**。
   从前它一直是个胶囊（深底 + 一圈 14% 的白描边），而在界面的语汇里那副样子
   说的是「状态」——Ready、3 dropped、System admin。节点的名字不是状态，它就是
   这个东西本身，却穿着状态的衣服，还比周围任何一个 chip 都亮。
   现在画布安静下来，注意力在哪儿哪儿才实——这与「悬停压暗其余、选中只留一条
   路」是同一条逻辑。 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function drawNodeLabel(
  ctx: CanvasRenderingContext2D,
  data: any,
  _settings: any,
): void {
  if (!data.label) return;
  /* 正被指着的那一个不在这一层画。sigma 每帧走两趟：`renderLabels` 铺标签层，
     `renderHighlightedNodes` 把 hoveredNode 交给 drawHoverCard 铺在上面的高亮层，
     **而前者并不排除后者**。两趟都画，同一块底牌就在两张叠着的画布上各来一遍：
     填色是实色时看不出来，投影看得出来——`shadowBlur` 叠两次，那圈黑深了一倍；
     未选中时底是 `rgba(12,12,12,0.9)`，叠完接近 0.99，比令牌定的实。
     让开的是这一层，因为高亮层画在上面 */
  if (data.hideBaseLabel) return;
  const size = CANVAS_LABEL_SIZE;
  /* 选中的那一个名字**加粗一档**：500 → 600，与界面里其余的强调同一档，
     不另起一个字重。反色的底牌要看向它才读得出来，字重在余光里就分得清——
     一屏名字里只有一个更重的。
     **得在量宽之前设好**：底牌是按 `measureText` 撑开的，先量后改字重，
     粗出来的字会顶到底牌外面去 */
  ctx.font = `${data.labelBold ? 600 : 500} ${size}px ${CANVAS_FONT}`;
  ctx.textBaseline = "middle";
  const padX = 8;
  const padY = 3;
  const w = ctx.measureText(data.label).width;
  /* 名字默认挂在节点的右上方。**贴着画布边的那些翻过来**：放大之后节点常常
     靠在视口边缘，名字照原样画就整条落在画布外——看着就是"放大之后字没了"。
     够不着就翻到左边 / 下边：节点在屏幕上，名字就在屏幕上 */
  const dx = Math.max(data.size * 0.7, 12);
  const dy = Math.max(data.size * 0.9, 10) + size / 2 + padY;
  const dpr = window.devicePixelRatio || 1;
  const vw = ctx.canvas.width / dpr;
  const x = data.x + dx + w + padX > vw ? data.x - dx - w : data.x + dx;
  const y = data.y - dy - size / 2 < 0 ? data.y + dy : data.y - dy;
  ctx.save();
  /* **名字总是一块牌子**：一块底 + 一圈细边，圆角 cell（4）。三档只换颜色，
     形状与位置一动不动——
       静止   `--u-pill-bg` + `--u-line-strong` 的边
       指到的 `--u-pill-bg-hover`，底亮一档，并浮起来（投影）
       选中的 反色（浅底深字），加粗一档，也浮起来
     从前静止那档是裸字加一圈画布色描边，只有指到/选中的才有底。裸字在一片
     连线和节点之间读起来费劲——描边压住的是线，压不住线背后深浅不一的底，
     而一块牌子自带一个稳定的底。代价是画面更满，所以牌子的边只有一档细线、
     静止那档不带投影：一屏几十块牌子，每块都浮着就糊成一片。 */
  const h = size + padY * 2;
  if (data.labelLift) {
    ctx.shadowColor = token("--u-shadow", "rgba(0,0,0,0.6)");
    ctx.shadowBlur = 12;
  }
  ctx.beginPath();
  ctx.roundRect(x - padX, y - h / 2, w + padX * 2, h, 4);
  ctx.fillStyle = data.labelInvert
    ? PILL_INVERT
    : data.labelLift
      ? PILL_BG_HOVER
      : PILL_BG;
  ctx.fill();
  // 投影只跟着底走：描边再来一次会把那圈黑描重一倍
  ctx.shadowBlur = 0;
  /* 反色那一档不描边：浅底自己就与画布分得开，再描一圈看着像两层皮 */
  if (!data.labelInvert) {
    ctx.lineWidth = 1;
    ctx.strokeStyle = PILL_BORDER;
    ctx.stroke();
  }
  ctx.fillStyle = data.labelInvert ? LABEL_HALO : PILL_TEXT;
  ctx.fillText(data.label, x, y);
  ctx.restore();
}

/* 指到一个节点时画什么。**就是那块标签底牌**（`drawNodeLabel`），不是另一张卡。
   从前这里是一张浮起来的两行卡片——名字一行、类型一行，位置在节点右上方——
   于是"指着"和"选中"这两个相邻的状态长成了两种完全不同的东西：一个浮层，
   一个贴着节点的底牌。类型那一行的信息在右边面板里说得更清楚，代价是每划过
   一个节点画面上就多一张卡。
   保留这个函数名是因为 sigma 的 `defaultDrawNodeHover` 认它 */
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export function drawHoverCard(
  ctx: CanvasRenderingContext2D,
  data: any,
  settings: any,
): void {
  // 这一层不认 `hideBaseLabel`：那道闸门说的正是"这一个交给高亮层画"，
  // 高亮层自己再让开就没人画了
  drawNodeLabel(
    ctx,
    data.hideBaseLabel ? { ...data, hideBaseLabel: false } : data,
    settings,
  );
}

/* 世界坐标网格：随相机缩放分级淡入淡出 */
const GRID_BASE_WORLD = 24; // 基准世界格距（匹配 ~300 尺度的布局）
const GRID_FADE_IN_PX = 13;
const GRID_FULL_PX = 52;
const GRID_MAX_LEVEL_PX = 480;
const GRID_MAX_ALPHA = 0.055;

export function drawWorldGrid(canvas: HTMLCanvasElement, sigma: Sigma): void {
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const { width, height } = sigma.getDimensions();
  const dpr = window.devicePixelRatio || 1;
  const pw = Math.round(width * dpr);
  const ph = Math.round(height * dpr);
  if (canvas.width !== pw || canvas.height !== ph) {
    canvas.width = pw;
    canvas.height = ph;
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, width, height);
  if (width <= 0 || height <= 0) return;

  // 世界→屏幕：两个探针点求每世界单位像素数与原点位置（无相机旋转场景）
  const p0 = sigma.graphToViewport({ x: 0, y: 0 });
  const p1 = sigma.graphToViewport({ x: 1, y: 0 });
  const ppw = p1.x - p0.x;
  if (!Number.isFinite(ppw) || ppw <= 0) return;

  // 最细可见层级：屏幕间距 ≥ 淡入阈值的最小 4 幂格距
  let spacing = GRID_BASE_WORLD;
  while (spacing * ppw < GRID_FADE_IN_PX) spacing *= 4;
  while (spacing * ppw >= GRID_FADE_IN_PX * 4) spacing /= 4;

  for (let sp = spacing; sp * ppw < GRID_MAX_LEVEL_PX; sp *= 4) {
    const ss = sp * ppw;
    const t = Math.min(
      1,
      (ss - GRID_FADE_IN_PX) / (GRID_FULL_PX - GRID_FADE_IN_PX),
    );
    if (t <= 0) continue;
    // 网格是墨色不是白色：暗底上是白，纸底上是近黑（0038）。透明度不变
    ctx.strokeStyle = `rgba(${INK_RGB},${(GRID_MAX_ALPHA * t).toFixed(4)})`;
    ctx.lineWidth = 1;
    ctx.beginPath();
    const startX = ((p0.x % ss) + ss) % ss;
    for (let x = startX; x <= width; x += ss) {
      const px = Math.round(x) + 0.5;
      ctx.moveTo(px, 0);
      ctx.lineTo(px, height);
    }
    const startY = ((p0.y % ss) + ss) % ss;
    for (let y = startY; y <= height; y += ss) {
      const py = Math.round(y) + 0.5;
      ctx.moveTo(0, py);
      ctx.lineTo(width, py);
    }
    ctx.stroke();
  }
}

/* ---------- 令牌的读者（0038 浅色版） ----------
 * canvas 读不到 var()，所以画布用的每一个颜色都从 <html> 上算好的令牌里读一遍。
 * 上面那些 `let` 的初值是暗色，只在第一帧前、以及没有 DOM 的测试里生效；
 * `refreshPalette()` 在启动和每次切主题时把它们全部换成当前主题的值。
 * ES 模块的导出是活绑定：这里赋值，import 的那一侧读到的就是新值。 */
export let EDGE = "rgba(163,163,163,0.2)";
export let EDGE_INFERRED = "rgba(163,163,163,0.1)";
export let EDGE_DIM = "#141414";
export let EDGE_FOCUS = "rgba(255,255,255,0.55)";
export let EDGE_DERIVED = "rgba(231,197,124,0.42)";
export let EDGE_DERIVED_DIM = "rgba(231,197,124,0.14)";
export let EDGE_FOCUS_DERIVED = "rgba(255,214,140,0.95)";
export let EDGE_CONTEST = "rgba(255,106,61,0.55)";
export let EDGE_FOCUS_CONTEST = "rgba(255,106,61,1)";
export let EDGE_SUBCLASS = "rgba(235,235,235,0.55)";
export let EDGE_SUBCLASS_FOCUS = "rgba(255,255,255,0.95)";
export let EDGE_RELATION = "rgba(128,128,128,0.3)";
export let EDGE_RELATION_FOCUS = "rgba(255,255,255,0.6)";
export let EDGE_DISJOINT = "rgba(255,157,175,0.45)";
export let EDGE_DISJOINT_FOCUS = "rgba(255,157,175,0.9)";
export let EDGE_RULE = "rgba(196,165,255,0.5)";
export let EDGE_RULE_FOCUS = "rgba(196,165,255,0.95)";
export let EDGE_SCHEMA_DIM = "rgba(48,48,48,0.4)";
export let LEGEND_SUBCLASS = "#ebebeb";
export let LEGEND_RELATION = "#8c8c8c";
export let LEGEND_DISJOINT = "#ff9daf";
export let LEGEND_RULE = "#c4a5ff";
export let SCRUB_PAST = "rgba(255,255,255,0.32)";
export let SCRUB_PLAY = "rgba(255,255,255,0.62)";
export let SCRUB_FUTURE = "rgba(255,255,255,0.04)";
/** 墨色最亮的那一档（暗底白、浅底近黑）：环往它混、登录场景的粒子用它 */
export let INK = "#ffffff";
/** 墨的三元组，给需要自己调透明度的地方 */
export let INK_RGB = "255,255,255";

function token(name: string, fallback: string): string {
  if (typeof document === "undefined") return fallback;
  const v = getComputedStyle(document.documentElement).getPropertyValue(name).trim();
  return v || fallback;
}
function rgbOf(triplet: string, alpha: number): string {
  return `rgba(${triplet.replace(/\s+/g, "")},${alpha})`;
}

/** 墨色按透明度：登录场景的粒子、边、脉冲都用它，页面里不再自己拼 rgba */
export function inkAt(alpha: number): string {
  return rgbOf(INK_RGB, alpha);
}

/** 浅色下把一个 rgba 边色按底色摊平成不透明的 rgb。
 *
 * sigma 的边着色器用预乘混合（ONE, ONE_MINUS_SRC_ALPHA）却不预乘 RGB，
 * 于是透明度压不暗一条边，只会把它**加**到底上：黑底上加一点灰正好是一条淡线，
 * 纸底上同一个 rgba(90,90,90,0.25) 加上去就溢出成白——「没选中时所有线都是白的」
 * 就是这么来的。暗度必须编码进 RGB（Graph.tsx 里 EDGE_DIM 那条注释说的同一件事）。
 * 暗色那一套是加法下调出来的，不动；浅色把 α 在这里就混掉。 */
export function flattenOnLight(color: string): string {
  if (typeof document === "undefined" || document.documentElement.dataset.theme !== "light") {
    return color;
  }
  /* **两种写法都要认。**令牌里写的是 `rgba(176,120,20,0.6)`，打包时 Lightning CSS
     把它压成 `#b0781499` —— 同一个颜色，十六进制带 alpha。从前这里只有一条
     `rgba(...)` 的正则，压缩之后的写法整个漏过去，半透明没摊平就交给了 sigma，
     于是那条派生边在纸底上被**加**成一道亮黄。（它没早点炸，是因为另一个 bug
     兜住了：只认六位的 `hexToRgb` 把 `#b0781499` 读成不透明的中灰，线画成了灰的。
     两个错凑成一个看着还行的结果，修好一个另一个就露出来。）
     走 `parseRgba` 就不用管写法；alpha 已经是 1 的原样退回，省一次无谓的改写。 */
  const [r, g, b, a] = parseRgba(color);
  if (a >= 1) return color;
  const ground = token("--u-ground-rgb", "250,250,250")
    .split(",")
    .map((v) => Number(v.trim()));
  const ch = (v: number, i: number) => Math.round(v * a + ground[i] * (1 - a));
  return `rgb(${ch(r, 0)},${ch(g, 1)},${ch(b, 2)})`;
}

/** 启动时和切主题后调一次，然后 `sigma.refresh()`。 */
export function refreshPalette() {
  NODE_SHELL_BASE = token("--u-node-shell", NODE_SHELL_BASE);
  NODE_CORE_BASE = token("--u-node-core", NODE_CORE_BASE);
  NODE_BORDER_BASE = token("--u-node-border", NODE_BORDER_BASE);
  MUTED_SHELL = token("--u-node-muted", MUTED_SHELL);
  PILL_BG = token("--u-pill-bg", PILL_BG);
  PILL_BG_HOVER = token("--u-pill-bg-hover", PILL_BG_HOVER);
  PILL_INVERT = token("--u-pill-invert", PILL_INVERT);
  PILL_BORDER = token("--u-line-strong", PILL_BORDER);
  PILL_TEXT = token("--u-text", PILL_TEXT);
  LABEL_HALO = token("--u-halo", LABEL_HALO);
  POP_BG = token("--u-pop-surface", POP_BG);
  CANVAS_TEXT = token("--u-text", CANVAS_TEXT);
  CANVAS_TEXT_2 = token("--u-text-2", CANVAS_TEXT_2);
  EDGE = token("--u-edge", EDGE);
  EDGE_INFERRED = token("--u-edge-inferred", EDGE_INFERRED);
  EDGE_DIM = token("--u-edge-dim", EDGE_DIM);
  EDGE_FOCUS = token("--u-edge-focus", EDGE_FOCUS);
  EDGE_DERIVED = token("--u-edge-derived", EDGE_DERIVED);
  EDGE_DERIVED_DIM = token("--u-edge-derived-dim", EDGE_DERIVED_DIM);
  EDGE_FOCUS_DERIVED = token("--u-edge-derived-focus", EDGE_FOCUS_DERIVED);
  EDGE_SUBCLASS = token("--u-edge-subclass", EDGE_SUBCLASS);
  EDGE_SUBCLASS_FOCUS = token("--u-edge-subclass-focus", EDGE_SUBCLASS_FOCUS);
  EDGE_RELATION = token("--u-edge-relation", EDGE_RELATION);
  EDGE_RELATION_FOCUS = token("--u-edge-relation-focus", EDGE_RELATION_FOCUS);
  EDGE_SCHEMA_DIM = token("--u-edge-schema-dim", EDGE_SCHEMA_DIM);
  SCRUB_PAST = token("--u-scrub-past", SCRUB_PAST);
  SCRUB_PLAY = token("--u-scrub-play", SCRUB_PLAY);
  SCRUB_FUTURE = token("--u-scrub-future", SCRUB_FUTURE);
  INK_RGB = token("--u-ink-rgb", INK_RGB);
  INK = rgbOf(INK_RGB, 1);
  // 语义色按三元组调透明度：争议边、不相交边、规则边
  const contest = token("--u-contest-rgb", "255,106,61");
  const danger = token("--u-danger-rgb", "255,157,175");
  const violet = token("--u-violet-rgb", "196,165,255");
  EDGE_CONTEST = rgbOf(contest, 0.55);
  EDGE_FOCUS_CONTEST = rgbOf(contest, 1);
  EDGE_DISJOINT = rgbOf(danger, 0.45);
  EDGE_DISJOINT_FOCUS = rgbOf(danger, 0.9);
  EDGE_RULE = rgbOf(violet, 0.5);
  EDGE_RULE_FOCUS = rgbOf(violet, 0.95);
  // 画布上的边：浅色下按底色摊平（见 flattenOnLight）
  EDGE = flattenOnLight(EDGE);
  EDGE_INFERRED = flattenOnLight(EDGE_INFERRED);
  EDGE_FOCUS = flattenOnLight(EDGE_FOCUS);
  EDGE_DERIVED = flattenOnLight(EDGE_DERIVED);
  EDGE_DERIVED_DIM = flattenOnLight(EDGE_DERIVED_DIM);
  EDGE_FOCUS_DERIVED = flattenOnLight(EDGE_FOCUS_DERIVED);
  EDGE_CONTEST = flattenOnLight(EDGE_CONTEST);
  EDGE_FOCUS_CONTEST = flattenOnLight(EDGE_FOCUS_CONTEST);
  EDGE_SUBCLASS = flattenOnLight(EDGE_SUBCLASS);
  EDGE_SUBCLASS_FOCUS = flattenOnLight(EDGE_SUBCLASS_FOCUS);
  EDGE_RELATION = flattenOnLight(EDGE_RELATION);
  EDGE_RELATION_FOCUS = flattenOnLight(EDGE_RELATION_FOCUS);
  EDGE_DISJOINT = flattenOnLight(EDGE_DISJOINT);
  EDGE_DISJOINT_FOCUS = flattenOnLight(EDGE_DISJOINT_FOCUS);
  EDGE_RULE = flattenOnLight(EDGE_RULE);
  EDGE_RULE_FOCUS = flattenOnLight(EDGE_RULE_FOCUS);
  EDGE_SCHEMA_DIM = flattenOnLight(EDGE_SCHEMA_DIM);
  LEGEND_SUBCLASS = token("--u-text", LEGEND_SUBCLASS);
  LEGEND_RELATION = token("--u-text-2", LEGEND_RELATION);
  LEGEND_DISJOINT = token("--u-danger", LEGEND_DISJOINT);
  LEGEND_RULE = token("--u-violet", LEGEND_RULE);
}

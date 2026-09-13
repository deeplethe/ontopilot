/* 实体类型的数据色（规矩 4：彩色只属于数据）。从 ui/index.tsx 搬到这里（0038），
 * 因为 style-guard 现在不许页面和组件里出现色值本身，而这一份**是数据**，且要与
 * `crates/utopia-store/src/palette.rs` 逐字一致。浅色版暂时沿用同一套：换一套浅底
 * 专用的数据色得连服务端那份一起换，另开一刀。 */

/* ---------- ColorPicker（精选色板 + hex 兜底；实体色刻意不开放全色域） ----------
   **改这里就得改 `crates/utopia-store/src/palette.rs`**：手动挑的色与自动按 key
   取的色必须来自同一组，否则一张图里会出现两套配色。那边有测试盯着，改漏了会红。 */
export const ENTITY_PALETTE = [
  "#7fd0ff",
  "#5fa8ff",
  "#5fd4d0",
  "#63e2b7",
  "#4cc38a",
  "#a8d878",
  "#ffd479",
  "#f2b66d",
  "#ff9d76",
  "#ff8a9e",
  "#ff9daf",
  "#e797d8",
  "#c4a5ff",
  "#9fa8ff",
  "#8ea5bd",
  "#b3b9c4",
];

/**
 * 类的 key → 颜色。**必须与 `crates/utopia-store/src/palette.rs` 的
 * `color_for_key` 逐位一致**：新建类时前端先按 key 挑一个显示出来，
 * 用户不改就这么存下去；而导入/消解那条路是后端算的。两边算得不一样，
 * 同一个 key 就会因为「谁建的」而拿到不同颜色。
 *
 * FNV-1a + 雪崩混合。用 BigInt 是因为 JS 的位运算是 32 位的，
 * 而这里要的是 64 位乘法——用 Number 做会静默丢高位，
 * 算出来跟 Rust 对不上，且不会有任何报错。
 */
export function colorForKey(key: string): string {
  let h = 0xcbf29ce484222325n;
  const M = (1n << 64n) - 1n;
  for (const b of new TextEncoder().encode(key)) {
    h = (h ^ BigInt(b)) & M;
    h = (h * 0x100000001b3n) & M;
  }
  h = (h ^ (h >> 33n)) & M;
  h = (h * 0xff51afd7ed558ccdn) & M;
  h = (h ^ (h >> 33n)) & M;
  return ENTITY_PALETTE[Number(h % BigInt(ENTITY_PALETTE.length))];
}

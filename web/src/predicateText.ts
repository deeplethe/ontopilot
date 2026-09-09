/** 谓词在**句子里**怎么读。
 *
 * 库里存的是小驼峰（见迁移 0042）：`worksFor`、`acceptedAnswer`、`accessTo`。
 * 那是词表该有的样子——管本体的那几个界面（左栏、表格、画布边标签、详情面板）
 * 就照这个显示，因为在那儿你认的是这个词本身，`acceptedAnswer` 才是能拿去查
 * schema.org 文档的那一个。
 *
 * 可事实行不是词表，**它是一句话**：
 *
 *   Li Si — works for — Meridian Systems
 *   Li Si — worksFor — Meridian Systems     ← 像代码，不像话
 *
 * 所以只有读成句子的地方（事实行、时间线、Chat 的回答）走这里拆一次。
 *
 * **缩写整段留着大写**：`productID` 拆成 “product ID” 而不是 “product id”，
 * `checkoutPageURLTemplate` 拆成 “checkout page URL template”。判断方法是把
 * 连续的大写看作一个词，只有当它后面紧跟一个小写字母时，最后那个大写才是
 * 下一个词的开头。
 */
export function predicateSentence(label: string): string {
  const words: string[] = [];
  let cur = "";
  const chars = [...label.trim()];
  for (let i = 0; i < chars.length; i++) {
    const c = chars[i];
    if (c === " " || c === "_" || c === "-") {
      if (cur) words.push(cur);
      cur = "";
      continue;
    }
    const isUpper = c !== c.toLowerCase() && c === c.toUpperCase();
    if (!isUpper) {
      cur += c;
      continue;
    }
    const prev = chars[i - 1];
    const next = chars[i + 1];
    const prevUpper = prev !== undefined && prev === prev.toUpperCase() && prev !== prev.toLowerCase();
    const nextLower = next !== undefined && next === next.toLowerCase() && next !== next.toUpperCase();
    // 大写起新词的两种情形：前面是小写（worksFor 的 F），
    // 或前面也是大写但后面跟着小写（URLTemplate 的 T）
    if (cur && (!prevUpper || nextLower)) {
      words.push(cur);
      cur = "";
    }
    cur += c;
  }
  if (cur) words.push(cur);
  if (words.length === 0) return label;
  // 首词小写；**整段大写的词原样留着**，那是缩写
  return words
    .map((w, i) => {
      const allCaps = w.length > 1 && w === w.toUpperCase() && w !== w.toLowerCase();
      if (allCaps) return w;
      return i === 0 ? w.charAt(0).toLowerCase() + w.slice(1) : w.toLowerCase();
    })
    .join(" ");
}

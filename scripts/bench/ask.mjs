#!/usr/bin/env node
// 问数的端到端测量台（#520）：人问一句话，拿回来的数对不对。
//
// **与 `mappings.mjs` 量的不是一回事，而且一个推不出另一个。** 口径确认得
// 再准，答案照样可能错——模型会挑错源、join 错、按错的日期列过滤，或者压根
// 不看语义层、照着 schema 文档自己写 SQL。反过来，一个一条确认映射都没有的
// 库照样答得出问题，靠的是 schema 文档，而且有时是对的。
//
// 判分材料只有两样，因为 `query_data` 记得很少（step 里只有源名与用途）：
//   1. 助手最后那段话；
//   2. `tool_exchange` 里的工具调用——**模型真正跑过的 SQL**。
// 后者是有用的那个：**把它跑过的 SQL 重跑一遍，跟 gold 的结果比数**。SQL 很短，
// 逃得过 `tool_exchange` 的截断，而它的输出逃不过。
//
// 两栏而不是一栏：
//   sql_right    它跑的那条 SQL 算的是问题问的那个数
//   answer_right 它印出来的数就是那个数
// 两者会分家。查对了却把单位说错、四舍五入错、或者转述成另一个数字的模型，
// 在只看 SQL 的分里是对的，而人读到的答案是错的。
//
// 用法：
//   node scripts/bench/ask.mjs --kb <id>              # 跑全部问题
//   node scripts/bench/ask.mjs --kb <id> --only disc_revenue
//
// 环境变量见 lib.mjs。

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  BASE, api, login, psql, value, same, roughly, log, parseArgs, cookieHeader,
} from "./lib.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const args = parseArgs(process.argv);
const corpusName = args.corpus || "tpch";
const truth = JSON.parse(fs.readFileSync(path.join(HERE, "truth", `${corpusName}.mappings.json`), "utf8"));
const qs = JSON.parse(fs.readFileSync(path.join(HERE, "truth", `${corpusName}.questions.json`), "utf8"));
const CORPUS_DB = process.env.BENCH_CORPUS_DB || `bench_${corpusName}`;

/// 一轮问答。SSE 帧是 `event: X\ndata: {...}\n\n`。
async function ask(kb, message) {
  const res = await fetch(`${BASE}/api/v1/kbs/${kb}/chat`, {
    method: "POST",
    headers: { "content-type": "application/json", cookie: cookieHeader() },
    body: JSON.stringify({ message }),
  });
  if (!res.ok) throw new Error(`chat -> ${res.status} ${(await res.text()).slice(0, 200)}`);
  const reader = res.body.getReader();
  const dec = new TextDecoder();
  let buf = "", text = "", conversation = null, error = null;
  const steps = [];
  for (;;) {
    const { done, value: chunk } = await reader.read();
    if (done) break;
    buf += dec.decode(chunk, { stream: true });
    let i;
    while ((i = buf.indexOf("\n\n")) >= 0) {
      const frame = buf.slice(0, i);
      buf = buf.slice(i + 2);
      const ev = /^event: ?(.*)$/m.exec(frame)?.[1];
      const data = frame
        .split("\n")
        .filter((l) => l.startsWith("data:"))
        .map((l) => l.slice(5).replace(/^ /, ""))
        .join("\n");
      try {
        if (ev === "conversation") conversation = JSON.parse(data).id;
        else if (ev === "delta") text += JSON.parse(data).text ?? "";
        else if (ev === "step") steps.push(JSON.parse(data));
        else if (ev === "error") error = data;
      } catch {
        /* 半帧或非 JSON：下一帧再说 */
      }
    }
  }
  return { conversation, text, steps, error };
}

/// 模型跑过的 SQL。**递归找**，因为工具调用的 `arguments` 本身是一段 JSON
/// 字符串，而这一层的形状随模型端而变（有的给 tool_calls，有的给 function_call）。
function sqlsIn(node, out = []) {
  if (typeof node === "string") {
    const s = node.trim();
    if (s.startsWith("{") || s.startsWith("[")) {
      try { sqlsIn(JSON.parse(s), out); } catch { /* 就是一段普通文本 */ }
    }
    return out;
  }
  if (Array.isArray(node)) { for (const n of node) sqlsIn(n, out); return out; }
  if (node && typeof node === "object") {
    for (const [k, v] of Object.entries(node)) {
      if (k === "sql" && typeof v === "string" && v.trim()) out.push(v.trim());
      else sqlsIn(v, out);
    }
  }
  return out;
}

/// 答案文本里的数字。千分位逗号去掉；引用标记 `[3]` 也会被算进来，
/// 但它撞上一条真值的概率可以忽略——真值里最小的是 1.21
function numbersIn(text) {
  const out = [];
  for (const m of text.matchAll(/-?\d[\d,]*\.?\d*/g)) {
    const n = Number(m[0].replace(/,/g, ""));
    if (Number.isFinite(n)) out.push(n);
  }
  return out;
}

const main = async () => {
  await login();
  const kb = args.kb;
  if (!kb || kb === true) throw new Error("要一个 --kb <id>");

  // 真值：单值口径 + 那些站得住但 22 条查询没说的
  const gold = new Map(
    [...truth.metrics, ...(truth.plausible ?? [])].map((m) => [m.id, { ...m, value: value(CORPUS_DB, m.gold).n }]),
  );

  // **这个问题需要的口径，有没有一条确认过的映射？** 闭环就在这一句上：
  // 答错的题分两种，一种是口径没配（去补映射），一种是配了还错（去看提示词
  // 或工具）。两种该做的事完全不同，而从前它们在结果里长得一样
  const confirmed = JSON.parse(psql(`SELECT coalesce(json_agg(x), '[]') FROM (
      SELECT m.table_name, m.expr, m.sql FROM concept_mappings m
       WHERE m.kb_id = '${kb}' AND m.status = 'confirmed') x`));
  const mapped = new Set();
  for (const m of confirmed) {
    const sql = m.sql?.trim()
      || (m.expr && m.table_name
        ? `SELECT ${m.expr} FROM ${m.table_name.includes(".") ? m.table_name : `${truth.db_schema}.${m.table_name}`}`
        : null);
    if (!sql) continue;
    const got = value(CORPUS_DB, sql);
    if (got.n === undefined) continue;
    for (const [id, g] of gold) if (same(g.value, got.n)) mapped.add(id);
  }

  const questions = qs.questions.filter((q) => (args.only && args.only !== true ? q.id === args.only : true));
  const c = { right: 0, sql_only: 0, answer_only: 0, wrong: 0, no_sql: 0, failed: 0 };
  const rows = [];
  for (const q of questions) {
    const g = gold.get(q.id);
    if (!g || !Number.isFinite(g.value)) { log(`跳过 ${q.id}：真值算不出来`); continue; }
    let r;
    try {
      r = await ask(kb, q.ask);
    } catch (e) {
      c.failed++;
      rows.push(`FAILED  ${q.id} — ${String(e.message).slice(0, 120)}`);
      continue;
    }
    const exchange = r.conversation
      ? JSON.parse(psql(`SELECT coalesce(json_agg(tool_exchange), '[]') FROM conversation_messages
                          WHERE conversation_id = '${r.conversation}' AND role = 'assistant'`))
      : [];
    const sqls = sqlsIn(exchange);
    const ran = sqls.map((s) => ({ s, n: value(CORPUS_DB, s.replace(/;\s*$/, "")).n }));
    const sqlRight = ran.some((x) => same(x.n, g.value));
    const answerRight = numbersIn(r.text).some((n) => roughly(n, g.value));

    let verdict;
    if (sqlRight && answerRight) { verdict = "RIGHT"; c.right++; }
    else if (sqlRight) { verdict = "SQL-ONLY"; c.sql_only++; }
    else if (answerRight) { verdict = "ANSWER-ONLY"; c.answer_only++; }
    else if (sqls.length === 0) { verdict = "NO-SQL"; c.no_sql++; }
    else { verdict = "WRONG"; c.wrong++; }

    const flag = mapped.has(q.id) ? "mapped" : "UNMAPPED";
    log(`${verdict.padEnd(11)} ${q.id} (${flag})`);
    if (verdict !== "RIGHT") {
      rows.push(
        `${verdict}  ${q.id} (${flag})  truth ${g.value}\n` +
        `    asked: ${q.ask}\n` +
        (ran.length
          ? ran.map((x) => `    ran:   ${x.s.replace(/\s+/g, " ").slice(0, 150)}  → ${x.n}`).join("\n")
          : "    ran:   (没跑任何 SQL)") +
        `\n    said:  ${r.text.replace(/\s+/g, " ").slice(0, 200)}`,
      );
    }
  }

  const n = questions.length;
  const pct = (a) => (n ? `${Math.round((a / n) * 100)}%` : "—");
  console.log(JSON.stringify({
    kb, corpus: corpusName, questions: n,
    right: `${c.right}/${n} (${pct(c.right)})`,
    sql_only: c.sql_only,
    answer_only: c.answer_only,
    wrong: c.wrong,
    no_sql: c.no_sql,
    failed: c.failed,
    // 口径有确认映射的题占多少。**「映射全」有了确定的意思**：这一栏满了，
    // 剩下的错就都不是覆盖率的问题
    mapped_definitions: `${[...gold.keys()].filter((id) => mapped.has(id)).length}/${gold.size}`,
  }, null, 2));
  if (rows.length) console.log("\n" + rows.join("\n\n"));
};

main().catch((e) => { console.error(e); process.exit(1); });

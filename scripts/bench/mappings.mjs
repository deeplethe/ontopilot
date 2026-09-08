#!/usr/bin/env node
// 映射探索的测量台（#501）：agent 从 schema 提议的口径，对不对、漏了多少。
//
// **打分不看名字，看数。** 治理那边的真值按两个名字键，因为它判的是二分类；
// 这里概念名是模型自己起的，「Revenue」与「已支付 GMV」按名字对不上任何一条。
// 所以真值一条是「一个业务口径 + 一条 gold SQL」，打分把提议真跑一遍，
// 跟 gold 的结果比数——数一样就是同一个口径，叫什么无所谓。
//
// 四栏，含义分开：
//   covered  真值 N 条，被至少一条提议算出来的有几条（漏没漏）
//   right    提议 K 条，跑得通且对上某条真值的有几条
//   wrong    **跑得通但一条都对不上**，附它算出的数与最接近的真值
//   broken   跑不通（列不存在、语法错）
//
// wrong 是决定 #504 能不能默认开的那个数。跑不通的提议无害，它失败得很响；
// 跑得通而算错的才是全部风险——问数会拿它印出一个看起来完全正常的数字。
//
// 用法：
//   node scripts/bench/mappings.mjs --fresh                  # 新库 → 挂源 → 探索 → 打分
//   node scripts/bench/mappings.mjs --fresh --no-comments    # 同上，但语料不带列注释
//   node scripts/bench/mappings.mjs --kb <id> --score        # 只打分，不动库
//
// 环境变量：BENCH_BASE（默认 http://127.0.0.1:8322）、BENCH_EMAIL / BENCH_PASSWORD、
//           BENCH_PSQL（默认 docker exec … psql -d utopia -tAc）、
//           BENCH_CORPUS_CONN（服务端用来连语料库的连接串）。

import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const BASE = process.env.BENCH_BASE || "http://127.0.0.1:8322";
const EMAIL = process.env.BENCH_EMAIL || "bench@test.local";
const PASSWORD = process.env.BENCH_PASSWORD || "benchbench123";

const args = Object.fromEntries(
  process.argv.slice(2).reduce((acc, cur, i, arr) => {
    if (cur.startsWith("--")) acc.push([cur.slice(2), arr[i + 1]?.startsWith("--") ? true : (arr[i + 1] ?? true)]);
    return acc;
  }, []),
);
const corpusName = args.corpus || "tpch";
const truth = JSON.parse(fs.readFileSync(path.join(HERE, "truth", `${corpusName}.mappings.json`), "utf8"));
const CORPUS_DB = process.env.BENCH_CORPUS_DB || `bench_${corpusName}`;
const CORPUS_CONN = process.env.BENCH_CORPUS_CONN || `postgres://utopia:utopia@localhost:5432/${CORPUS_DB}`;

let cookie = "";
async function api(method, url, body) {
  const init = { method, headers: {} };
  if (cookie) init.headers.cookie = cookie;
  if (body !== undefined) { init.headers["content-type"] = "application/json"; init.body = JSON.stringify(body); }
  const r = await fetch(BASE + url, init);
  for (const c of r.headers.getSetCookie?.() ?? []) cookie = c.split(";")[0];
  const text = await r.text();
  if (!r.ok) throw new Error(`${method} ${url} -> ${r.status} ${text.slice(0, 200)}`);
  return text ? JSON.parse(text) : null;
}

const PSQL = process.env.BENCH_PSQL || "docker exec -e PGPASSWORD=utopia landscapebi-db-1 psql -U utopia -d utopia -tAc";
// 应用库与语料库都不是 BENCH_PSQL 里写的那个：**测量台跑在自己的库上**
// （bench/README 的第一条规则），而 BENCH_PSQL 与 govern.mjs 共用，指着开发库。
// 头一轮就栽在这里：脚本一直在开发库里找这个 kb 的任务，找不到，等满超时——
// 而任务早就跑完了，十二条提议好端端躺在另一个库里
const APP_DB = process.env.BENCH_APP_DB || "utopia_mapbench";
function run(cmdline, sql) {
  const parts = cmdline.split(" ");
  return execFileSync(parts[0], [...parts.slice(1), sql], { encoding: "utf8", maxBuffer: 64 << 20 }).trim();
}
const psql = (sql) => run(PSQL.replace(/-d \S+/, `-d ${APP_DB}`), sql);
// 语料库是另一个库：把 -d 的目标换掉，其余照旧（凭据、容器名都跟着 BENCH_PSQL 走）
const corpusPsql = (sql) => run(PSQL.replace(/-d \S+/, `-d ${CORPUS_DB}`), `SET statement_timeout = '20s'; ${sql}`);
const num = (sql) => Number(psql(sql) || 0);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const log = (...a) => console.error(new Date().toISOString().slice(11, 19), ...a);
async function until(fn, everyMs, stallMs) {
  let deadline = Date.now() + stallMs, last = null;
  for (;;) {
    const r = await fn();
    if (r === true) return;
    if (typeof r === "number" && r !== last) { last = r; deadline = Date.now() + stallMs; }
    if (Date.now() > deadline) throw new Error(`等超时：${Math.round(stallMs / 60000)} 分钟没有任何进展`);
    await sleep(everyMs);
  }
}

// ---------- 语料 ----------
//
// **每一组一个新库**（bench/README 的第一条规则），这里还多一层理由：
// `propose` 的 `ON CONFLICT … WHERE status = 'proposed'` 让第二轮探索
// 继承第一轮的行，同一个库上跑两次，第二次的分不是第二次的。
function loadCorpus() {
  const db = CORPUS_DB;
  const exists = run(PSQL.replace(/-d \S+/, "-d postgres"), `SELECT 1 FROM pg_database WHERE datname='${db}'`);
  if (!exists) {
    run(PSQL.replace(/-d \S+/, "-d postgres"), `CREATE DATABASE ${db}`);
    log(`建库 ${db}`);
  }
  const ddl = fs.readFileSync(path.join(HERE, "schemas", `${corpusName}.sql`), "utf8");
  corpusPsql(ddl);
  log(`${corpusName} 表与行就位`);
  // 注释是自变量：不加载就是一份同构但没有注释的语料，两轮之差即注释值多少分
  if (!args["no-comments"]) {
    corpusPsql(fs.readFileSync(path.join(HERE, "schemas", `${corpusName}.comments.sql`), "utf8"));
    log("列注释已加载");
  } else {
    log("列注释**未**加载（--no-comments）");
  }
}

// ---------- 新库 + 挂源 + 探索 ----------
async function fresh() {
  loadCorpus();
  const ws = (await api("GET", "/api/v1/workspaces"))[0].id;
  const stamp = new Date().toISOString().slice(0, 16).replace(/[:T]/g, "-");
  const label = args.label || (args["no-comments"] ? "no-comments" : "commented");
  const kb = (await api("POST", `/api/v1/workspaces/${ws}/kbs`, { name: `mappings ${corpusName} ${label} ${stamp}` })).id;
  await sleep(4000);

  // **源名就叫语料名，各轮复用同一个源。**
  //
  // 第一轮给它起了 `tpch-2026-09-08-12-41`（想让各轮的源并存着比），结果是
  // 零提议：`explore_mappings` 拿模型回的 `source` 去比挂载源的名字
  // （`eq_ignore_ascii_case`），而模型看着 schema 回的是 `tpch`，对不上就
  // 整条 `continue`，十二条一条不剩。任务照样 done，页面上只有一条
  // 「没提出来」的告警。**源名是提议能不能落地的隐性依赖**，真实的库里
  // 一样会踩——它该进 #503 的覆盖率报告，而不是靠人猜。
  const dsName = corpusName;
  const existing = (await api("GET", "/api/v1/admin/data-sources")).data_sources.find((d) => d.name === dsName);
  const ds = existing?.id
    ?? (await api("POST", "/api/v1/admin/data-sources", { name: dsName, conn_string: CORPUS_CONN })).id;
  await api("PUT", `/api/v1/admin/data-sources/${ds}/grants/${ws}`);
  const mounted = await api("PUT", `/api/v1/kbs/${kb}/data-sources/${ds}`);
  log(`kb ${kb}，源 ${dsName} 已挂载，schema 文档 ${mounted.schema_tables ?? "?"} 张表`);
  if (mounted.schema_error) log(`  schema 同步报错：${mounted.schema_error}`);

  await api("POST", `/api/v1/kbs/${kb}/data-sources/explore`);
  log("探索已入队，等提议落库");
  let said = "";
  await until(async () => {
    const row = psql(`SELECT status || E'\\t' || attempts || E'\\t' || coalesce(left(last_error, 160), '')
                        FROM jobs WHERE kind='explore_mappings' AND payload->>'kb_id'='${kb}'
                        ORDER BY id DESC LIMIT 1`);
    const [status, attempts, err] = (row || "queued\t0\t").split("\t");
    // **任务重试期间就把错话说出来。** 头一轮等满十分钟拿到的是「没有任何进展」，
    // 而真正该看见的是「模型密钥解不开」——它第一次失败时就已经写在 last_error 里了
    if (err && err !== said) { said = err; log(`  第 ${attempts} 次失败：${err}`); }
    if (status === "done" || status === "failed") return true;
    return num(`SELECT count(*) FROM concept_mappings WHERE kb_id='${kb}'`);
  }, 5000, 600000);
  return kb;
}

// ---------- 打分 ----------
const qualify = (t) => (t && t.includes(".") ? t : `${truth.db_schema}.${t}`);

// 提议怎么变成一条能跑的 SQL：给了 sql 就用 sql，否则 expr + table 拼一条。
// 两者都没有就只有 table_name——那不是一个可执行的口径，算 broken。
function proposalSql(m) {
  if (m.sql && m.sql.trim()) return m.sql.trim().replace(/;\s*$/, "");
  if (m.expr && m.table_name) return `SELECT ${m.expr} FROM ${qualify(m.table_name)}`;
  return null;
}

function value(sql) {
  try {
    const out = corpusPsql(sql);
    // **命令标签也走 stdout。** `SET statement_timeout` 先打一行 `SET`，
    // 而 -tA 下数据行不带标签——不滤掉它，每条口径读到的第一行都是 `SET`，
    // 于是二十四条真值与十二条提议齐刷刷 NaN，看起来像全军覆没
    const first = out.split("\n").map((l) => l.trim()).filter((l) => l !== "" && l !== "SET")[0];
    if (first === undefined) return { empty: true };
    return { n: Number(String(first).split("|")[0]) };
  } catch (e) {
    return { error: String(e.stderr || e.message).split("\n").filter((l) => l.trim())[0]?.slice(0, 120) };
  }
}

// 相对误差。**容差要松到吃得下小数舍入，紧到分得开两个口径**——
// tpch 里 charge 与 order_total 是同一个业务量的两条算法，差在分位上，
// 判成同一条是对的；而 disc_revenue 与 discount_given 差着一个数量级
const same = (a, b) => {
  if (!Number.isFinite(a) || !Number.isFinite(b)) return false;
  if (a === b) return true;
  return Math.abs(a - b) <= 1e-6 * Math.max(Math.abs(a), Math.abs(b), 1);
};

function score(kb) {
  const rows = JSON.parse(psql(`SELECT coalesce(json_agg(x), '[]') FROM (
      SELECT m.id, e.canonical_name AS concept, t.key AS kind, m.source, m.table_name,
             m.expr, m.sql, m.unit, m.status
        FROM concept_mappings m
        JOIN entities e ON e.id = m.concept_id
        JOIN entity_types t ON t.id = e.type_id
       WHERE m.kb_id = '${kb}' ORDER BY e.canonical_name) x`));

  const gold = truth.metrics.map((m) => ({ ...m, value: value(m.gold).n }));
  // 22 条查询没说、但读得懂这个 schema 的人不会反对的口径（退货率、客均余额）。
  // **单独一栏，不算对也不算错**——头一轮把退货率记成 wrong，而它没有任何毛病，
  // 错的是真值不全。govern.mjs 的 `unlabeled` 是同一件事
  const plausible = (truth.plausible || []).map((m) => ({ ...m, value: value(m.gold).n }));
  const dimCols = new Set(truth.dimensions.map((d) => d.column.toLowerCase()));
  // 陷阱分角色。**同一列在两个角色下不是同一件事**：`p_size` 当维度是对的
  // （Q16 就按它分组），当指标求和才没有意义；`l_comment` 反过来。
  // 第三轮上这条把一条正确的维度提议记成了踩陷阱
  const trapFor = (role) =>
    new Map(
      truth.traps
        .filter((t) => (t.as ?? "both") === "both" || t.as === role)
        .map((t) => [t.column.toLowerCase(), t.why]),
    );
  const dimTraps = trapFor("dimension");
  const trapCols = trapFor("metric");

  const c = { right: 0, wrong: 0, broken: 0, traps: 0, plausible: 0, dim_right: 0, dim_wrong: 0 };
  const hit = new Set(), wrong = [], broken = [], fair = [];

  for (const m of rows) {
    // 维度没有数可比：判它指的那一列在不在真值的 group-by 集合里，
    // 以及有没有落到陷阱列上（把一个键或一段自由文本当成维度）
    if (m.kind === "dimension") {
      const col = `${qualify(m.table_name)}.${(m.expr || "").replace(/[^\w.]/g, "")}`.toLowerCase();
      if (dimCols.has(col)) c.dim_right++;
      else { c.dim_wrong++; wrong.push(`dim  "${m.concept}" → ${col}${dimTraps.has(col) ? ` **trap: ${dimTraps.get(col)}**` : ""}`); }
      if (dimTraps.has(col)) c.traps++;
      continue;
    }
    const sql = proposalSql(m);
    if (!sql) { c.broken++; broken.push(`"${m.concept}" 没有可执行的定义（只有 table_name=${m.table_name}）`); continue; }
    const got = value(sql);
    if (got.error) { c.broken++; broken.push(`"${m.concept}" → ${got.error}`); continue; }
    if (got.empty) { c.broken++; broken.push(`"${m.concept}" 返回空`); continue; }

    const matched = gold.filter((g) => same(g.value, got.n));
    if (matched.length) { c.right++; matched.forEach((g) => hit.add(g.id)); continue; }
    const plaus = plausible.find((g) => same(g.value, got.n));
    if (plaus) { c.plausible++; fair.push(`"${m.concept}" = ${plaus.id} (${plaus.label})`); continue; }

    c.wrong++;
    // 最接近的真值：告诉人这条错在哪个方向，而不是只说它错了
    const near = gold
      .filter((g) => Number.isFinite(g.value))
      .sort((a, b) => Math.abs(a.value - got.n) - Math.abs(b.value - got.n))[0];
    const trap = [...trapCols.keys()].find((k) => (m.expr || "").toLowerCase().includes(k.split(".").pop()));
    if (trap) c.traps++;
    wrong.push(
      `"${m.concept}" = ${sql}\n     算出 ${got.n}，最近的真值 ${near?.id} = ${near?.value}` +
      (trap ? `\n     **trap: ${trapCols.get(trap)}**` : ""),
    );
  }

  const pct = (a, b) => (b ? `${Math.round((a / b) * 100)}%` : "—");
  const missed = gold.filter((g) => !hit.has(g.id));
  // **那一轮带没带注释，问库名而不是问命令行参数。** `--score` 重打一次分时
  // 命令行上没有 `--no-comments`，照参数写就把不带注释的那轮报成带注释的
  const kbName = psql(`SELECT name FROM knowledge_bases WHERE id = '${kb}'`);
  const out = {
    kb, corpus: corpusName,
    comments: !kbName.includes("no-comments"),
    proposals: rows.length,
    metrics: { right: c.right, plausible: c.plausible, wrong: c.wrong, broken: c.broken },
    dimensions: { right: c.dim_right, wrong: c.dim_wrong },
    covered: `${hit.size}/${gold.length} (${pct(hit.size, gold.length)})`,
    traps_hit: c.traps,
  };
  console.log(JSON.stringify(out, null, 2));
  if (fair.length) console.log("\nPLAUSIBLE — 22 条查询没说，但站得住的口径\n  " + fair.join("\n  "));
  if (wrong.length) console.log("\nWRONG — 跑得通，算的不是任何一条真值\n  " + wrong.join("\n  "));
  if (broken.length) console.log("\nBROKEN — 跑不通（无害，人一眼看得见）\n  " + broken.join("\n  "));
  if (missed.length) console.log("\nMISSED — 真值里没人提的口径\n  " + missed.map((g) => `${g.id} (${g.label})`).join("\n  "));
}

// ---------- 主流程 ----------
const main = async () => {
  try {
    await api("POST", "/api/v1/auth/login", { email: EMAIL, password: PASSWORD });
  } catch {
    // 测量台该能在一个空库上从头跑起来。首个注册的人建 org 与 workspace 并且是
    // admin——注册数据源要 admin，所以这条 fallback 不只是省事
    log(`${EMAIL} 登录不上，按首用户注册`);
    await api("POST", "/api/v1/auth/register", {
      email: EMAIL, password: PASSWORD, display_name: "bench", org_name: "bench",
    });
  }
  const kb = args.kb && args.kb !== true ? args.kb : await fresh();
  score(kb);
};
main().catch((e) => { console.error(e); process.exit(1); });

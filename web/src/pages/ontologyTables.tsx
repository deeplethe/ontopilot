// 本体读成三张表（#498）。
//
// 从前这一页一次只讲一个类：左栏一列名字，画布一张图，右边一块面板讲你点中的
// 那一个。凡是横跨本体的问题都没有地方问——哪些属性声明了唯一、导入了九百个类
// 之后哪些一个实例都没有、哪些关系连 domain 都没写（于是抽取拿不到签名）、
// 这个库到底有哪些属性。最后一个尤其：属性只出现在某个类的面板里，一次一个类，
// 「这个库有哪些属性」在界面上没有任何地方回答得了。
//
// 这个月别的同类页面都已经是表了——令牌、数据源、成员、部署用户、业务规则
// （#482 里那句「一列卡片跟我们刚重做的任何东西都不像」）。本体是最后一个，
// 也是列数最多的一个。
//
// **层级怎么办**：`subClassOf` 是本体的骨架，拍平就丢了。所以默认按层级排、
// 名字列带缩进；按别的列排序就拍平，而父类那一列一直在，拍平不丢信息，只是
// 换了一种读法。文件管理器就是这么做的。

import { useMemo, useState } from "react";

import type { EntityTypeView, RelationTypeView } from "../api";
import { S } from "../i18n";
import {
  Chip,
  cn,
  GroupLabel,
  Input,
  LinkButton,
  ROW_TRAILING,
  Segmented,
  Table,
  TBody,
  Td,
  Th,
  THead,
  Tr,
} from "../ui";

export type TableTab = "classes" | "properties" | "attributes";

/** 这一行挂在谁下面。**没有主父类就退回第一个父类**：`primary_parent` 只在
 *  人手动指定时才写，而导入的包一个都没有——schema.org 那 916 个类里是 0 个。
 *  只认它的话，任何导入来的本体在树里都是平的一长条，层级完全看不见。
 *
 *  **一个类可以有多个父类**（`parents` 是个数组；schema.org 这 916 个里有 48 个
 *  是这样，FOAF 的 Person 同时是 Agent 与 SpatialThing）。一行只能挂在一处，
 *  所以缩进选的是其中之一——那一列把全部父类都列出来，行上再标一个记号说
 *  「还有别的」，缩进就不至于冒充成唯一的真相 */
export function treeParent(t: EntityTypeView): string | null {
  return t.primary_parent ?? t.parents[0] ?? null;
}

/** 名字列的缩进：一层 12px。与左栏那棵树同一个做法 */
const INDENT = 12;

/* ---------- 排序 ---------- */

type Sort = { col: string; dir: "asc" | "desc" } | null;

/** 可排序的表头。**排序把层级拍平**（见文件头）——所以名字列不排序，它就是
 *  层级本身；点别的列才进入「平铺」那种读法 */
function SortTh({
  col,
  sort,
  onSort,
  className,
  children,
}: {
  col: string;
  sort: Sort;
  onSort: (s: Sort) => void;
  className?: string;
  children: React.ReactNode;
}) {
  const active = sort?.col === col;
  return (
    <Th className={className}>
      <LinkButton
        className="inline-flex items-center gap-1"
        onClick={() =>
          onSort(
            active && sort.dir === "desc"
              ? null
              : { col, dir: active ? "desc" : "asc" },
          )
        }
      >
        {children}
        {active && <span className="u-num">{sort.dir === "asc" ? "↑" : "↓"}</span>}
      </LinkButton>
    </Th>
  );
}

function sorted<T>(rows: T[], sort: Sort, key: (r: T) => string | number) {
  if (!sort) return rows;
  const out = [...rows].sort((a, b) => {
    const x = key(a);
    const y = key(b);
    return typeof x === "number" && typeof y === "number"
      ? x - y
      : String(x).localeCompare(String(y));
  });
  return sort.dir === "asc" ? out : out.reverse();
}

/* ---------- 类 ---------- */

/** 按 primary_parent 排成层级序，带每行的深度。过滤或排序时拍平 */
function classRows(
  types: EntityTypeView[],
  flat: boolean,
): { t: EntityTypeView; depth: number }[] {
  if (flat) return types.map((t) => ({ t, depth: 0 }));
  const children = new Map<string | null, EntityTypeView[]>();
  for (const t of types) {
    const p = treeParent(t);
    if (!children.has(p)) children.set(p, []);
    children.get(p)!.push(t);
  }
  const out: { t: EntityTypeView; depth: number }[] = [];
  const seen = new Set<string>();
  const walk = (parent: string | null, depth: number) => {
    for (const t of children.get(parent) ?? []) {
      if (seen.has(t.id)) continue; // 父类互指成环也走不死
      seen.add(t.id);
      out.push({ t, depth });
      walk(t.id, depth + 1);
    }
  };
  walk(null, 0);
  // 父类不在这份清单里的（被过滤掉了）挂不上树，补在末尾——不能因为排不进
  // 层级就不显示
  for (const t of types) if (!seen.has(t.id)) out.push({ t, depth: 0 });
  return out;
}

function ClassesTable({
  types,
  attributes,
  filter,
  onOpen,
  onSeeInstances,
}: {
  types: EntityTypeView[];
  attributes: RelationTypeView[];
  filter: string;
  onOpen: (t: EntityTypeView) => void;
  onSeeInstances: (t: EntityTypeView) => void;
}) {
  const [sort, setSort] = useState<Sort>(null);
  const nameOf = useMemo(
    () => new Map(types.map((t) => [t.id, t.label])),
    [types],
  );
  const attrCount = useMemo(() => {
    const m = new Map<string, number>();
    for (const a of attributes)
      for (const d of a.domains) m.set(d, (m.get(d) ?? 0) + 1);
    return m;
  }, [attributes]);

  const q = filter.trim().toLowerCase();
  const hit = (t: EntityTypeView) =>
    !q || t.label.toLowerCase().includes(q) || t.key.toLowerCase().includes(q);
  const shown = types.filter(hit);
  // 过滤中或排序中都拍平：两种情况下层级都不再是这份清单的次序
  const rows = classRows(sorted(shown, sort, (t) =>
    sort?.col === "usage" ? t.usage
    : sort?.col === "attrs" ? (attrCount.get(t.id) ?? 0)
    : sort?.col === "parent" ? (treeParent(t) ? (nameOf.get(treeParent(t)!) ?? "") : "")
    : t.label,
  ), !!sort || !!q);

  return (
    <Table>
      <THead>
        <Tr>
          <Th>{S.ontology.colName}</Th>
          <Th className="hidden md:table-cell">{S.ontology.colKey}</Th>
          <SortTh col="parent" sort={sort} onSort={setSort}>
            {S.ontology.parent}
          </SortTh>
          <Th className="hidden lg:table-cell">{S.ontology.disjoint}</Th>
          <SortTh col="usage" sort={sort} onSort={setSort} className="text-right">
            {S.ontology.colInstances}
          </SortTh>
          <SortTh col="attrs" sort={sort} onSort={setSort} className="text-right">
            {S.ontology.schemaTabAttributes}
          </SortTh>
        </Tr>
      </THead>
      <TBody>
        {rows.map(({ t, depth }) => (
          <Tr key={t.id} interactive onClick={() => onOpen(t)}>
            <Td>
              <span
                className="flex items-center gap-2"
                style={{ paddingLeft: depth * INDENT }}
              >
                <span
                  className={cn(
                    "h-2 w-2 shrink-0",
                    t.shape === "square" ? "scale-90" : "rounded-full",
                  )}
                  style={{ background: t.color }}
                />
                <span className="truncate">{t.label}</span>
                {/* 多父类：缩进只挂得住一处，这个记号说还有几处，父类那一列
                    把它们都列着 */}
                {t.parents.length > 1 && (
                  <span
                    className="u-chip u-chip-neutral shrink-0 !px-2 !text-fine"
                    title={S.ontology.multiParentHint}
                  >
                    +{t.parents.length - 1}
                  </span>
                )}
                {t.builtin && <Chip tone="neutral">{S.ontology.builtin}</Chip>}
              </span>
            </Td>
            <Td className="hidden font-mono text-small text-ink-2 md:table-cell">
              {t.key}
            </Td>
            <Td className="text-small text-ink-2">
              {t.parents.length
                ? t.parents.map((p) => nameOf.get(p) ?? p).join(", ")
                : "—"}
            </Td>
            <Td className="hidden text-small text-ink-2 lg:table-cell">
              {t.disjoint.length
                ? t.disjoint.map((d) => nameOf.get(d) ?? d).join(", ")
                : "—"}
            </Td>
            <Td className="text-right">
              {t.usage > 0 ? (
                <LinkButton
                  className="u-num"
                  onClick={(e) => {
                    e.stopPropagation();
                    onSeeInstances(t);
                  }}
                >
                  {t.usage}
                </LinkButton>
              ) : (
                <span className="u-num text-ink-2">0</span>
              )}
            </Td>
            <Td className="u-num text-right text-ink-2">
              {attrCount.get(t.id) ?? 0}
            </Td>
          </Tr>
        ))}
      </TBody>
    </Table>
  );
}

/* ---------- 关系 ---------- */

/** 公理做成几个字母的芯片而不是六个勾：一行里六个复选框读不出「这条关系
 *  保证了什么」，几个字母能顺着一列扫下去 */
function Axioms({ r }: { r: RelationTypeView }) {
  const on: [boolean, string, string][] = [
    [r.functional, "F", S.ontology.functional],
    [r.inverse_functional, "IF", S.ontology.inverseFunctional],
    [r.is_transitive, "T", S.ontology.axiomTransitive],
    [r.is_symmetric, "S", S.ontology.axiomSymmetric],
    [r.is_asymmetric, "A", S.ontology.axiomAsymmetric],
    [r.is_irreflexive, "IR", S.ontology.axiomIrreflexive],
  ];
  const shown = on.filter(([v]) => v);
  if (shown.length === 0) return <span className="text-ink-2">—</span>;
  return (
    <span className="flex flex-wrap gap-1">
      {shown.map(([, mark, title]) => (
        <span key={mark} className="u-chip u-chip-neutral !px-2 !text-fine" title={title}>
          {mark}
        </span>
      ))}
    </span>
  );
}

function PropertiesTable({
  relations,
  types,
  filter,
  onOpen,
}: {
  relations: RelationTypeView[];
  types: EntityTypeView[];
  filter: string;
  onOpen: (r: RelationTypeView) => void;
}) {
  const [sort, setSort] = useState<Sort>(null);
  const nameOf = useMemo(
    () => new Map(types.map((t) => [t.id, t.label])),
    [types],
  );
  const relName = useMemo(
    () => new Map(relations.map((r) => [r.id, r.label])),
    [relations],
  );
  const q = filter.trim().toLowerCase();
  const shown = relations.filter(
    (r) =>
      !q || r.label.toLowerCase().includes(q) || r.key.toLowerCase().includes(q),
  );
  const side = (ids: string[]) =>
    ids.length ? ids.map((i) => nameOf.get(i) ?? i).join(", ") : S.ontology.anyType;
  const rows = sorted(shown, sort, (r) =>
    sort?.col === "usage" ? r.usage
    : sort?.col === "temporal" ? r.temporal
    : r.label,
  );

  return (
    <Table>
      <THead>
        <Tr>
          <Th>{S.ontology.colName}</Th>
          <Th className="hidden md:table-cell">{S.ontology.colKey}</Th>
          <Th>{S.ontology.colSignature}</Th>
          <SortTh col="temporal" sort={sort} onSort={setSort}>
            {S.ontology.temporal}
          </SortTh>
          <Th>{S.ontology.axioms}</Th>
          <Th className="hidden lg:table-cell">{S.ontology.inverseOf}</Th>
          <SortTh col="usage" sort={sort} onSort={setSort} className="text-right">
            {S.ontology.colFacts}
          </SortTh>
        </Tr>
      </THead>
      <TBody>
        {rows.map((r) => (
          <Tr key={r.id} interactive onClick={() => onOpen(r)}>
            <Td>
              <span className="flex items-center gap-2">
                <span className="truncate">{r.label}</span>
                {r.builtin && <Chip tone="neutral">{S.ontology.builtin}</Chip>}
              </span>
            </Td>
            <Td className="hidden font-mono text-small text-ink-2 md:table-cell">
              {r.key}
            </Td>
            <Td className="text-small text-ink-2">
              {side(r.domains)} → {side(r.ranges)}
            </Td>
            <Td className="text-small text-ink-2">
              {r.temporal === "event"
                ? S.ontology.temporalEvent
                : r.temporal === "eternal"
                  ? S.ontology.temporalEternal
                  : S.ontology.temporalState}
            </Td>
            <Td>
              <Axioms r={r} />
            </Td>
            <Td className="hidden text-small text-ink-2 lg:table-cell">
              {r.inverse_of ? (relName.get(r.inverse_of) ?? "—") : "—"}
            </Td>
            <Td className="u-num text-right text-ink-2">{r.usage}</Td>
          </Tr>
        ))}
      </TBody>
    </Table>
  );
}

/* ---------- 属性 ---------- */

/** 属性单开一张表，是这三张里最能说明问题的一张：从前属性只在某个类的面板里
 *  露面，一次一个类，「这个库有哪些属性」在界面上没有任何地方回答得了 */
function AttributesTable({
  attributes,
  types,
  filter,
  onOpen,
}: {
  attributes: RelationTypeView[];
  types: EntityTypeView[];
  filter: string;
  onOpen: (a: RelationTypeView) => void;
}) {
  const [sort, setSort] = useState<Sort>(null);
  const nameOf = useMemo(
    () => new Map(types.map((t) => [t.id, t.label])),
    [types],
  );
  const q = filter.trim().toLowerCase();
  const shown = attributes.filter(
    (a) =>
      !q || a.label.toLowerCase().includes(q) || a.key.toLowerCase().includes(q),
  );
  const rows = sorted(shown, sort, (a) =>
    sort?.col === "usage" ? a.usage
    : sort?.col === "datatype" ? (a.datatype ?? "")
    : sort?.col === "domain" ? (a.domains[0] ? (nameOf.get(a.domains[0]) ?? "") : "")
    : a.label,
  );

  return (
    <Table>
      <THead>
        <Tr>
          <Th>{S.ontology.colName}</Th>
          <Th className="hidden md:table-cell">{S.ontology.colKey}</Th>
          <SortTh col="domain" sort={sort} onSort={setSort}>
            {S.ontology.colOnClass}
          </SortTh>
          <SortTh col="datatype" sort={sort} onSort={setSort}>
            {S.ontology.colDatatype}
          </SortTh>
          <Th>{S.ontology.colUnit}</Th>
          <Th>{S.ontology.colSingleValued}</Th>
          <SortTh col="usage" sort={sort} onSort={setSort} className="text-right">
            {S.ontology.colFacts}
          </SortTh>
        </Tr>
      </THead>
      <TBody>
        {rows.map((a) => (
          <Tr key={a.id} interactive onClick={() => onOpen(a)}>
            <Td className="truncate">{a.label}</Td>
            <Td className="hidden font-mono text-small text-ink-2 md:table-cell">
              {a.key}
            </Td>
            <Td className="text-small text-ink-2">
              {a.domains.length
                ? a.domains.map((d) => nameOf.get(d) ?? d).join(", ")
                : "—"}
            </Td>
            <Td className="text-small text-ink-2">{a.datatype ?? "—"}</Td>
            <Td className="text-small text-ink-2">{a.unit || "—"}</Td>
            <Td className="text-small text-ink-2">
              {a.functional ? S.ontology.yes : "—"}
            </Td>
            <Td className="u-num text-right text-ink-2">{a.usage}</Td>
          </Tr>
        ))}
      </TBody>
    </Table>
  );
}

/* ---------- 三张表 ---------- */

export function OntologyTables({
  entityTypes,
  relationTypes,
  onOpenClass,
  onOpenProperty,
  onOpenAttribute,
  onSeeInstances,
}: {
  entityTypes: EntityTypeView[];
  relationTypes: RelationTypeView[];
  onOpenClass: (t: EntityTypeView) => void;
  onOpenProperty: (r: RelationTypeView) => void;
  onOpenAttribute: (a: RelationTypeView) => void;
  onSeeInstances: (t: EntityTypeView) => void;
}) {
  const [tab, setTab] = useState<TableTab>("classes");
  const [filter, setFilter] = useState("");
  const relations = relationTypes.filter((r) => r.kind === "relation");
  const attributes = relationTypes.filter((r) => r.kind === "attribute");
  const counts: Record<TableTab, number> = {
    classes: entityTypes.length,
    properties: relations.length,
    attributes: attributes.length,
  };

  return (
    <div className="flex h-full min-w-0 flex-col">
      {/* 操作表里内容的控件在表**外面**（规矩 6）：过滤清空之后表会变成空状态，
          而把过滤框放进去就等于把唯一能改回来的东西一起藏了 */}
      <div className="flex shrink-0 items-center gap-4 px-8 pt-6 pb-3">
        <Segmented<TableTab>
          value={tab}
          onChange={setTab}
          options={(["classes", "properties", "attributes"] as const).map(
            (v) => ({
              value: v,
              label:
                v === "classes"
                  ? S.ontology.tabClasses
                  : v === "properties"
                    ? S.ontology.tabProperties
                    : S.ontology.schemaTabAttributes,
              count: counts[v],
            }),
          )}
        />
        <Input
          icon={<span className="text-ink-2">/</span>}
          className="w-58"
          placeholder={S.ontology.filter}
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
        <GroupLabel className={cn(ROW_TRAILING, "shrink-0")}>
          {S.ontology.rowsShown(
            tab === "classes"
              ? counts.classes
              : tab === "properties"
                ? counts.properties
                : counts.attributes,
          )}
        </GroupLabel>
      </div>
      <div className="u-scroll min-h-0 flex-1 overflow-y-auto px-8 pb-6">
        {tab === "classes" && (
          <ClassesTable
            types={entityTypes}
            attributes={attributes}
            filter={filter}
            onOpen={onOpenClass}
            onSeeInstances={onSeeInstances}
          />
        )}
        {tab === "properties" && (
          <PropertiesTable
            relations={relations}
            types={entityTypes}
            filter={filter}
            onOpen={onOpenProperty}
          />
        )}
        {tab === "attributes" && (
          <AttributesTable
            attributes={attributes}
            types={entityTypes}
            filter={filter}
            onOpen={onOpenAttribute}
          />
        )}
      </div>
    </div>
  );
}

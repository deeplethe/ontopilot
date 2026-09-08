// 停靠面板：本体页与图谱页 schema 层共用的那一块（#497）。
//
// 从前它长在 Ontology.tsx 里，只有那一页用得上。图谱页有了 schema 层之后，
// 选中一个类同样要有地方说它是什么——两页各写一份的话，「停靠面板只有一种」
// 这条规矩第二天就不成立了（设计规矩见 web/DESIGN.md 第 6 条）。
//
// **两页的用法不同，这是故意的**：本体页那一份带铅笔、带「新建子类」，改动从
// 那里开弹窗；图谱页这一份只读，末尾给一条去本体页编辑的路。读在图谱页，
// 写在本体页——0012 说本体是数据要守的合同，那份合同在哪儿改，就该只有一处。

import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { ExternalLink, X } from "lucide-react";

import { api, type EntityTypeView, type RelationTypeView } from "../api";
import { S } from "../i18n";
import {
  Chip,
  cn,
  IconButton,
  Pager,
  ROW_TRAILING,
  rowClass,
  Segmented,
} from "../ui";

export function DockedPanel({
  header,
  actions,
  tabs,
  exiting,
  onClose,
  children,
}: {
  header: React.ReactNode;
  /** 关闭键左边的动作（编辑）：面板只展示，改动从这里开弹窗 */
  actions?: React.ReactNode;
  /** 分段控件，跟着标题一起固定在顶上——它要能一直点得到 */
  tabs?: React.ReactNode;
  /** 正在退场：演动画，期间不再接受点击（u-dock-out 里带了 pointer-events） */
  exiting?: boolean;
  onClose: () => void;
  children: React.ReactNode;
}) {
  return (
    <div
      // 与图谱页的实体面板同一副壳：同宽（w-96）、同一个顶部起点（给顶上那排
      // 药丸让位），同一个头部解剖。两页并排看是同一件东西
      className={`${exiting ? "u-dock-out" : "u-dock-in"} glass-strong absolute top-14 right-3 bottom-3 w-96 z-10 rounded-overlay shadow-2xl flex flex-col`}
    >
      <div className="shrink-0 flex items-start justify-between gap-2 px-4 py-4 border-b border-line">
        <div className="min-w-0">{header}</div>
        <div className="-mr-1 -mt-1 flex shrink-0 items-center gap-1">
          {actions}
          <IconButton size="sm" label={S.ontology.schemaClosePanel} onClick={onClose}>
            <X size={15} />
          </IconButton>
        </div>
      </div>
      {tabs && <div className="shrink-0 px-4 pt-3 pb-1">{tabs}</div>}
      <div className="u-scroll flex-1 min-h-0 overflow-y-auto flex flex-col gap-3 px-4 py-3">
        {children}
      </div>
    </div>
  );
}


/* 面板标题：色点 + 名字 + 第二行的小字。与图谱页实体面板的头一个写法 */
export function PanelHeader({
  color,
  square,
  title,
  sub,
  builtin,
}: {
  color?: string;
  square?: boolean;
  title: string;
  sub?: string;
  builtin?: boolean;
}) {
  return (
    <>
      <div className="flex items-center gap-2">
        {color && (
          <span
            className={`h-2.5 w-2.5 shrink-0 ${square ? "scale-90" : "rounded-full"}`}
            style={{ background: color, boxShadow: `0 0 8px ${color}55` }}
          />
        )}
        <span
          className="truncate text-title font-semibold tracking-tight text-ink"
          style={{ fontFamily: "var(--font-display)" }}
        >
          {title}
        </span>
        {builtin && <Chip tone="neutral">{S.ontology.builtin}</Chip>}
      </div>
      {sub && <div className="mt-1 text-small text-ink-2">{sub}</div>}
    </>
  );
}


export function Def({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="border-b border-line py-2 last:border-0">
      <div className="text-small text-ink-2">{label}</div>
      <div className="mt-1 break-words text-body text-ink">{children}</div>
    </div>
  );
}

export function Description({ text }: { text: string | null | undefined }) {
  return text?.trim() ? (
    <span className="whitespace-pre-wrap">{text}</span>
  ) : (
    <span className="text-ink-2">{S.ontology.noDescription}</span>
  );
}



/* ---------- 图谱页 schema 层的只读类面板 ---------- */

/** 选中一个类之后说它是什么：定义、它参与的关系、它的实例。
 *
 * **只读**。想改去本体页——面板末尾那条路就是干这个的。 */
export function SchemaClassPanel({
  kbId,
  cls,
  allTypes,
  relations,
  exiting,
  onClose,
}: {
  kbId: string;
  cls: EntityTypeView;
  allTypes: EntityTypeView[];
  relations: RelationTypeView[];
  exiting?: boolean;
  onClose: () => void;
}) {
  const [tab, setTab] = useState<"definition" | "relations" | "instances">(
    "definition",
  );
  const nameOf = (id: string) =>
    allTypes.find((t) => t.id === id)?.label ?? id;
  /** 这个类挂着的关系：从它出发的、指向它的。属性不在这儿——那是字面值 */
  const mine = relations.filter(
    (r) =>
      r.kind === "relation" &&
      (r.domains.includes(cls.id) || r.ranges.includes(cls.id)),
  );

  return (
    <DockedPanel
      exiting={exiting}
      onClose={onClose}
      header={
        <PanelHeader
          color={cls.color}
          square={cls.shape === "square"}
          title={cls.label}
          sub={`${cls.key} · ${S.ontology.usage(cls.usage)}`}
          builtin={cls.builtin}
        />
      }
      tabs={
        <Segmented<"definition" | "relations" | "instances">
          value={tab}
          onChange={setTab}
          options={[
            { value: "definition", label: S.ontology.schemaTabDefinition },
            { value: "relations", label: S.ontology.schemaTabRelations },
            { value: "instances", label: S.ontology.schemaTabInstances },
          ]}
        />
      }
    >
      {tab === "definition" && (
        <div>
          <Def label={S.ontology.parent}>
            {cls.parents.length > 0
              ? cls.parents.map(nameOf).join(", ")
              : S.ontology.noParent}
          </Def>
          <Def label={S.ontology.disjoint}>
            {cls.disjoint.length > 0
              ? cls.disjoint.map(nameOf).join(", ")
              : S.ontology.noDisjoint}
          </Def>
          <Def label={S.ontology.description}>
            <Description text={cls.description} />
          </Def>
          {/* 去本体页改它。**这一条是这块面板唯一的出口**：图谱页负责读，
              合同在哪儿改只该有一处 */}
          <Link
            to="/kb/$kbId/ontology"
            params={{ kbId }}
            className={cn(rowClass(), "-mx-2 mt-2")}
          >
            <ExternalLink size={14} className="shrink-0 text-ink-2" />
            <span className="truncate">{S.ontology.editInOntology}</span>
          </Link>
        </div>
      )}
      {tab === "relations" &&
        (mine.length === 0 ? (
          <p className="text-small text-ink-2">{S.ontology.schemaNoRelationships}</p>
        ) : (
          <div>
            {mine.map((r) => {
              const out = r.domains.includes(cls.id);
              return (
                <div key={r.id} className={cn(rowClass(), "-mx-2")}>
                  <span className="truncate text-body text-ink">{r.label}</span>
                  <span className={cn(ROW_TRAILING, "max-w-40 truncate")}>
                    {(out ? r.ranges : r.domains).map(nameOf).join(", ") ||
                      S.ontology.anyType}
                  </span>
                </div>
              );
            })}
          </div>
        ))}
      {tab === "instances" && <InstanceList kbId={kbId} type={cls} />}
    </DockedPanel>
  );
}

/** 这个类的实体，服务端分页。点一条就下到实例层并选中它——
 *  从一个类走到它的一个例子，是这块面板最实在的一条路 */
function InstanceList({ kbId, type }: { kbId: string; type: EntityTypeView }) {
  const PER = 12;
  const [page, setPage] = useState(0);
  useEffect(() => setPage(0), [type.id]);
  const q = useQuery({
    queryKey: ["type-entities", kbId, type.id, page],
    queryFn: () => api.typeEntities(kbId, type.id, page, PER),
  });
  const total = q.data?.total ?? 0;
  const rows = q.data?.entities ?? [];
  if (!q.isPending && total === 0)
    return <p className="text-small text-ink-2">{S.ontology.schemaNoInstances}</p>;
  return (
    <div>
      <div>
        {rows.map((e) => (
          <Link
            key={e.id}
            to="/kb/$kbId/graph"
            params={{ kbId }}
            search={{ entity: e.id }}
            className={cn(rowClass(), "-mx-2")}
          >
            <span
              className={`h-2 w-2 shrink-0 ${type.shape === "square" ? "scale-90" : "rounded-full"}`}
              style={{ background: type.color }}
            />
            <span className="truncate">{e.name}</span>
            <span className={cn("u-num", ROW_TRAILING)}>
              {S.ontology.instanceFacts(e.fact_count)}
            </span>
          </Link>
        ))}
      </div>
      <Pager total={total} pageSize={PER} page={page} onPage={setPage} />
    </div>
  );
}

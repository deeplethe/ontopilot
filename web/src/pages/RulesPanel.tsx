/** 业务规则的编写台（0021 / #277）。
 *
 *  **规则读起来要像一句话**：「Well 及其子类，当 全烃 大于 8 且 解释 属于 {…}，
 *  得出 GasBearingWell」。所以这里不做表达式输入框——结构化的下拉与数字框既是
 *  录入方式，也是它唯一的显示方式，两者不会漂移。
 *
 *  条件整组提交而不是逐条打补丁：一个合取是一个整体，没有「改到一半」的状态。
 *
 *  样式按 web/DESIGN.md 那五条：字号五档、间距六档、颜色只用 token、控件与状态
 *  一律从 ui/ 来。 */
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Pencil, Play, Plus, Search, Trash2 } from "lucide-react";
import {
  api,
  type BusinessRule,
  type EntityTypeView,
  type RelationTypeView,
  type RuleCondition,
} from "../api";
import { S } from "../i18n";
import {
  Button,
  Chip,
  DangerConfirm,
  Dropdown,
  IconButton,
  Input,
  LinkButton,
  PageHeader,
  Dialog,
  Field,
  Table,
  TBody,
  Td,
  Th,
  THead,
  Tr,
} from "../ui";
import { toast } from "../toast";

/** op → 那句话里的动词。**数字与集合两类分开**，因为它们的操作数长得不一样 */
const OPS: {
  value: string;
  label: () => string;
  operand: "num" | "range" | "set" | "none";
}[] = [
  { value: "gt", label: () => S.ontology.ruleOpGt, operand: "num" },
  { value: "gte", label: () => S.ontology.ruleOpGte, operand: "num" },
  { value: "lt", label: () => S.ontology.ruleOpLt, operand: "num" },
  { value: "lte", label: () => S.ontology.ruleOpLte, operand: "num" },
  { value: "between", label: () => S.ontology.ruleOpBetween, operand: "range" },
  { value: "in", label: () => S.ontology.ruleOpIn, operand: "set" },
  { value: "present", label: () => S.ontology.ruleOpPresent, operand: "none" },
];

const operandKind = (op: string) =>
  OPS.find((o) => o.value === op)?.operand ?? "num";

/** 一条规则可以被搜到的全部文本。**判据也算**——「哪条规则用到了 Clearance」
    是找规则最常见的问法，只搜名字的话得先记住自己当初叫它什么 */
function searchText(r: BusinessRule): string {
  return [
    r.name,
    r.description ?? "",
    r.subject_label,
    r.conclude_type_label ?? "",
    r.conclude_predicate_label ?? "",
    ...r.conditions.map(
      (c) => `${c.predicate_label} ${operandText(c.op, c.operand)}`,
    ),
  ]
    .join(" ")
    .toLowerCase();
}

/** 条件的操作数 → 输入框里的文本。回读要与写入是同一套，否则编辑一次就变形 */
function operandText(op: string, operand: unknown): string {
  const kind = operandKind(op);
  if (kind === "none") return "";
  if (kind === "set") return Array.isArray(operand) ? operand.join(", ") : "";
  if (kind === "range") return Array.isArray(operand) ? operand.join(" - ") : "";
  return operand === null || operand === undefined ? "" : String(operand);
}

/** 输入框文本 → 操作数。**解析不出来就返回 undefined**，由调用方拦在保存之前 */
function parseOperand(op: string, text: string): unknown | undefined {
  const kind = operandKind(op);
  if (kind === "none") return undefined;
  const t = text.trim();
  if (kind === "set") {
    const set = t
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
    return set.length ? set : undefined;
  }
  if (kind === "range") {
    const parts = t.split(/[-~]/).map((s) => Number(s.trim()));
    return parts.length === 2 && parts.every((n) => Number.isFinite(n))
      ? parts
      : undefined;
  }
  const n = Number(t);
  return Number.isFinite(n) ? n : undefined;
}

type Draft = {
  /** 改的是哪一条；新建时为 null。**同一份草稿两种用途**——两套表单会漂移 */
  id: string | null;
  name: string;
  description: string;
  subject_type_id: string;
  conclusion: "typing" | "attribute";
  conclude_type_id: string;
  conclude_predicate_id: string;
  conclude_value: string;
  conditions: { predicate_id: string; op: string; text: string }[];
};

const emptyDraft = (
  classes: EntityTypeView[],
  attrs: RelationTypeView[],
): Draft => ({
  id: null,
  name: "",
  description: "",
  subject_type_id: classes[0]?.id ?? "",
  conclusion: "typing",
  conclude_type_id: classes[0]?.id ?? "",
  conclude_predicate_id: attrs[0]?.id ?? "",
  conclude_value: "",
  conditions: attrs[0]
    ? [{ predicate_id: attrs[0].id, op: "gt", text: "" }]
    : [],
});

/** 已有规则 → 草稿。**回读要与写入是同一套形状**，否则编辑一次就变形。 */
function draftOf(r: BusinessRule): Draft {
  return {
    id: r.id,
    name: r.name,
    description: r.description ?? "",
    subject_type_id: r.subject_type_id,
    conclusion: r.conclusion,
    conclude_type_id: r.conclude_type_id ?? "",
    conclude_predicate_id: r.conclude_predicate_id ?? "",
    conclude_value:
      typeof r.conclude_value === "string"
        ? r.conclude_value
        : r.conclude_value === undefined || r.conclude_value === null
          ? ""
          : String(r.conclude_value),
    conditions: r.conditions.map((c) => ({
      predicate_id: c.predicate_id,
      op: c.op,
      text: operandText(c.op, c.operand),
    })),
  };
}

/** 一条规则此刻标住了谁，以及凭什么。计数点开就是它。 */
function Matches({ kbId, ruleId }: { kbId: string; ruleId: string }) {
  const q = useQuery({
    queryKey: ["ruleMatches", kbId, ruleId],
    queryFn: () => api.ruleMatches(kbId, ruleId),
  });
  const rows = q.data?.matches ?? [];
  const total = q.data?.total ?? 0;
  if (!rows.length) {
    return <p className="text-small text-ink-2">{S.ontology.ruleMatchesEmpty}</p>;
  }
  return (
    <div className="space-y-2">
      {rows.map((m) => (
        <div key={m.derived_id} className="space-y-1">
          <div className="flex flex-wrap items-baseline gap-2">
            <span className="text-small text-ink">{m.entity}</span>
            <span className="text-fine text-ink-2">→ {m.concluded}</span>
            {/* 同一个实体会因为不同时段的读数出现好几次，写出这一段才不像重复 */}
            {m.valid_from && (
              <span className="u-num text-fine text-ink-2">
                {S.ontology.ruleMatchSpan(
                  m.valid_from.slice(0, 10),
                  m.valid_to ? m.valid_to.slice(0, 10) : null,
                )}
              </span>
            )}
          </div>
          {/* 前提就是「凭什么」——列表没有它就跟一串凭空的判断没区别 */}
          {m.premises.length > 0 && (
            <p className="text-fine text-ink-2">
              {S.ontology.ruleMatchBecause(m.premises.join(", "))}
            </p>
          )}
        </div>
      ))}
      {total > rows.length && (
        <p className="u-num text-fine text-ink-2">
          {S.ontology.ruleMatchesMore(rows.length, total)}
        </p>
      )}
    </div>
  );
}

export function RulesPanel({
  kbId,
  classes,
  attributes,
  onError,
}: {
  kbId: string;
  classes: EntityTypeView[];
  /** kind='attribute' 的谓词——规则只读实体自己的字面值 */
  attributes: RelationTypeView[];
  onError: (e: unknown) => void;
}) {
  const qc = useQueryClient();
  const rules = useQuery({
    queryKey: ["rules", kbId],
    queryFn: () => api.rules(kbId),
  });
  const [draft, setDraft] = useState<Draft | null>(null);
  /** 待确认删除的那一条。删规则会带走它推出的全部结论，值得停一下 */
  const [doomed, setDoomed] = useState<BusinessRule | null>(null);
  /** 展开了哪条规则的命中列表。一次只展开一条——两份长列表并排读不了 */
  const [opened, setOpened] = useState<string | null>(null);
  const [filter, setFilter] = useState("");

  const invalidate = () => {
    qc.invalidateQueries({ queryKey: ["rules", kbId] });
    qc.invalidateQueries({ queryKey: ["graph"] });
    qc.invalidateQueries({ queryKey: ["entity", kbId] });
  };

  const save = useMutation({
    mutationFn: async () => {
      const d = draft!;
      if (!d.conditions.length) throw new Error(S.ontology.ruleNeedsCondition);
      const conditions: RuleCondition[] = d.conditions.map((c) => {
        const operand = parseOperand(c.op, c.text);
        if (operandKind(c.op) !== "none" && operand === undefined) {
          throw new Error(S.ontology.ruleNeedsCondition);
        }
        return { predicate_id: c.predicate_id, op: c.op, operand };
      });
      const conclusion = {
        conclusion: d.conclusion,
        conclude_type_id:
          d.conclusion === "typing" ? d.conclude_type_id : undefined,
        conclude_predicate_id:
          d.conclusion === "attribute" ? d.conclude_predicate_id : undefined,
        conclude_value:
          d.conclusion === "attribute" ? d.conclude_value : undefined,
      };
      // 改一条已有的规则走 PATCH，主类不动——换主类等于换一条规则，
      // 那时候删了重写比原地改诚实
      await (d.id
        ? api.updateRule(kbId, d.id, {
            name: d.name,
            description: d.description,
            conditions,
            ...conclusion,
          })
        : api.createRule(kbId, {
            name: d.name,
            description: d.description,
            subject_type_id: d.subject_type_id,
            conditions,
            ...conclusion,
          }));
    },
    onSuccess: () => {
      toast.success(S.ontology.ruleSaved);
      setDraft(null);
      invalidate();
    },
    onError: (e: Error) => toast.error(e.message),
  });

  const run = useMutation({
    mutationFn: () => api.runRules(kbId),
    onSuccess: (r) => {
      toast.success(S.ontology.ruleRunDone(r.hits, r.inserted, r.invalidated));
      // 展开不全要单独说：少推几条与「不满足」在结果里长得一样
      if (r.capped) toast.error(S.ontology.ruleRunCapped(r.capped));
      invalidate();
    },
    onError,
  });

  const toggle = useMutation({
    mutationFn: (r: BusinessRule) =>
      api.updateRule(kbId, r.id, { enabled: !r.enabled }),
    onSuccess: invalidate,
    onError,
  });

  const remove = useMutation({
    mutationFn: (id: string) => api.deleteRule(kbId, id),
    onSuccess: () => {
      toast.success(S.ontology.ruleDeleted);
      invalidate();
    },
    onError,
  });

  const all = rules.data?.rules ?? [];
  const needle = filter.trim().toLowerCase();
  const list = needle
    ? all.filter((r) => searchText(r).includes(needle))
    : all;
  /** 命中列表看的是哪一条。一次一条——两份长列表并排读不了 */
  const opening = list.find((r) => r.id === opened) ?? null;

  return (
    <div className="space-y-4">
      {doomed && (
        <DangerConfirm
          title={S.ontology.ruleDelete}
          hint={S.ontology.ruleDeleteConfirm(doomed.name)}
          confirmLabel={S.ontology.ruleDelete}
          cancelLabel={S.graph.editCancel}
          busy={remove.isPending}
          onConfirm={() => {
            remove.mutate(doomed.id);
            setDoomed(null);
          }}
          onCancel={() => setDoomed(null)}
        />
      )}

      <PageHeader
        className="mb-2"
        title={S.ontology.rulesTitle}
        sub={S.ontology.rulesHint}
      />

      {/* 搜索与两个动作同一行，都在表格外面（DESIGN.md 6）：筛空了这一行还在，
          否则改筛选的唯一入口跟着列表一起消失。输入框不进 PageHeader 的
          actions——那一排是贴着标题基线排的，塞个控件进去基线就断了 */}
      <div className="flex flex-wrap items-center gap-2">
        <Input
          icon={<Search size={13} />}
          className="w-64"
          placeholder={S.ontology.ruleSearch}
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
        <div className="ml-auto flex items-center gap-2">
          <Button
            size="sm"
            variant="ghost"
            onClick={() => run.mutate()}
            disabled={run.isPending || !all.length}
          >
            <Play size={12} />
            {run.isPending ? S.ontology.ruleRunning : S.ontology.ruleRun}
          </Button>
          {/* 入口不设门槛：缺属性时表单自己会在缺的那一处说 */}
          <Button
            size="sm"
            variant="primary"
            onClick={() => setDraft(emptyDraft(classes, attributes))}
          >
            <Plus size={12} />
            {S.ontology.ruleNew}
          </Button>
        </div>
      </div>

      {!list.length ? (
        <p className="text-small text-ink-2">
          {/* 一条都没有，与「筛掉了」是两回事：前者该去建一条，后者该改筛选 */}
          {needle ? S.ontology.rulesNoMatch : S.ontology.rulesEmpty}
        </p>
      ) : (
        /* 一张表，不是一条一张卡片（DESIGN.md 6）。**第一列仍然是那句话**——
           规则的全部语义就在那句话里，收进详情等于把规则本身藏起来；其余几列
           是扫一眼要的答案：推出什么、此刻成立几条、开着没有 */
        <div className="glass overflow-hidden rounded-panel">
          <Table>
            <THead>
              <Tr>
                <Th>{S.ontology.ruleColRule}</Th>
                <Th>{S.ontology.ruleConcludes}</Th>
                <Th>{S.ontology.ruleColDerived}</Th>
                <Th>{S.ontology.ruleColStatus}</Th>
                <Th />
              </Tr>
            </THead>
            <TBody>
              {list.map((r) => (
                <Tr key={r.id} className={r.enabled ? undefined : "opacity-55"}>
                  <Td>
                    <div className="text-body text-ink">{r.name}</div>
                    <RuleSentence rule={r} />
                    {r.description && (
                      <div className="mt-1 text-fine text-ink-2">
                        {r.description}
                      </div>
                    )}
                  </Td>
                  <Td className="text-small text-ink">
                    {r.conclusion === "typing"
                      ? r.conclude_type_label
                      : `${r.conclude_predicate_label} = ${JSON.stringify(r.conclude_value)}`}
                  </Td>
                  <Td className="whitespace-nowrap">
                    {/* 此刻凭它成立的结论条数。**点得动**——二十个实体还能一个个
                        点开看，两百个就只能靠这份列表 */}
                    {r.derived_count > 0 ? (
                      <LinkButton
                        className="u-num"
                        onClick={() => setOpened(r.id)}
                      >
                        {S.ontology.ruleDerivedCount(r.derived_count)}
                      </LinkButton>
                    ) : (
                      <span className="text-small text-ink-2">—</span>
                    )}
                    {/* 展不完的组合：常驻，不只在跑完那一刻的提示里。少推几条与
                        「不满足」在结果里长得一样 */}
                    {r.capped > 0 && (
                      <Chip
                        tone="warn"
                        className="ml-2"
                        title={S.ontology.ruleCappedHint}
                      >
                        {S.ontology.ruleCappedChip}
                      </Chip>
                    )}
                  </Td>
                  <Td>
                    {/* 开关是个动作，所以是 Button；启用与否用 variant 区分，
                        而不是拿 Chip 当按钮——Chip 是状态标签，不接受点击 */}
                    <Button
                      size="sm"
                      variant={r.enabled ? "secondary" : "ghost"}
                      onClick={() => toggle.mutate(r)}
                      aria-pressed={r.enabled}
                    >
                      {r.enabled ? S.ontology.ruleEnabled : S.ontology.ruleDisabled}
                    </Button>
                  </Td>
                  <Td className="whitespace-nowrap text-right">
                    <IconButton
                      label={S.ontology.ruleEdit}
                      size="sm"
                      onClick={() => setDraft(draftOf(r))}
                    >
                      <Pencil size={12} />
                    </IconButton>
                    <IconButton
                      label={S.ontology.ruleDelete}
                      size="sm"
                      className="ml-1"
                      onClick={() => setDoomed(r)}
                    >
                      <Trash2 size={12} />
                    </IconButton>
                  </Td>
                </Tr>
              ))}
            </TBody>
          </Table>
        </div>
      )}

      {/* 命中：这一条此刻推出了哪些结论 */}
      <Dialog
        open={!!opening}
        onOpenChange={(o) => !o && setOpened(null)}
        closeLabel={S.ui.close}
        title={opening?.name ?? ""}
        description={S.ontology.ruleMatchesTitle}
      >
        {opening && <Matches kbId={kbId} ruleId={opening.id} />}
      </Dialog>

      {draft && (
        <RuleDialog
          draft={draft}
          setDraft={setDraft}
          classes={classes}
          attributes={attributes}
          busy={save.isPending}
          onSave={() => save.mutate()}
        />
      )}
    </div>
  );
}

/** 规则读成一句话。**这一段就是它的全部语义**，没有别处再藏着条件。
    条件之间是合取——所以中间写的是「并且」，不是一个点号：符号读不出
    「全都要成立」，而这正是规则最容易被误读的地方。 */
function RuleSentence({ rule }: { rule: BusinessRule }) {
  return (
    <p className="text-small leading-relaxed text-ink-2">
      <span>{rule.subject_label}</span>
      <span> {S.ontology.ruleWhere} </span>
      {rule.conditions.map((c, i) => (
        <span key={i}>
          {i > 0 && (
            <span className="text-ink-2"> {S.ontology.ruleAnd} </span>
          )}
          <span className="text-ink">{c.predicate_label}</span>{" "}
          <span>{OPS.find((o) => o.value === c.op)?.label() ?? c.op}</span>{" "}
          <span className="u-num text-ink">{operandText(c.op, c.operand)}</span>
        </span>
      ))}
    </p>
  );
}

/** 写一条规则。**与全站其他表单同一副样子**：弹窗、Field 标签、底栏两个按钮。
    从前它是页面里长出来的一张卡，与新建令牌、登记数据源各说各的。 */
function RuleDialog({
  draft,
  setDraft,
  classes,
  attributes,
  busy,
  onSave,
}: {
  draft: Draft;
  setDraft: (d: Draft | null) => void;
  classes: EntityTypeView[];
  attributes: RelationTypeView[];
  busy: boolean;
  onSave: () => void;
}) {
  const ready =
    !!draft.name.trim() && !!draft.subject_type_id && draft.conditions.length > 0;
  return (
    <Dialog
      open
      onOpenChange={(o) => !o && setDraft(null)}
      width="lg"
      closeLabel={S.ui.close}
      title={draft.id ? S.ontology.ruleEditing : S.ontology.ruleNew}
      footer={
        <>
          <Button size="sm" variant="secondary" onClick={() => setDraft(null)}>
            {S.graph.editCancel}
          </Button>
          {/* 保存的门槛与服务端同一条：名字、主类、至少一个条件。空合取恒真，
              会把整个类归进去（business_rules.rs 也是这么挡的） */}
          <Button size="sm" variant="primary" onClick={onSave} disabled={busy || !ready}>
            {S.ontology.ruleSave}
          </Button>
        </>
      }
    >
      <div className="space-y-4">
        <div className="grid grid-cols-2 gap-3">
          <Field label={S.ontology.ruleName} className="mb-0">
            <Input
              autoFocus
              className="w-full"
              placeholder={S.ontology.ruleNamePlaceholder}
              value={draft.name}
              onChange={(e) => setDraft({ ...draft, name: e.target.value })}
            />
          </Field>
          <Field
            label={S.ontology.ruleSubject}
            hint={S.ontology.ruleSubjectHint}
            className="mb-0"
          >
            {draft.id ? (
              // 改一条已有规则时主类固定：换主类等于换一条规则，
              // 而它推出来的结论全挂在旧主类上
              <p className="py-1 text-body text-ink">
                {classes.find((c) => c.id === draft.subject_type_id)?.label}
              </p>
            ) : (
              <Dropdown
                className="w-full"
                value={draft.subject_type_id}
                onChange={(v) => setDraft({ ...draft, subject_type_id: v })}
                options={classes.map((c) => ({ value: c.id, label: c.label }))}
              />
            )}
          </Field>
        </div>

        <Field label={S.ontology.ruleDescription} className="mb-0">
          <Input
            className="w-full"
            value={draft.description}
            onChange={(e) => setDraft({ ...draft, description: e.target.value })}
          />
        </Field>

        <Field label={S.ontology.ruleConditions} className="mb-0">
          <div className="space-y-2">
            {draft.conditions.map((c, i) => (
              <div key={i} className="flex items-center gap-2">
                {/* 合取写在行首而不是行尾：读的人先知道「还要同时成立」，
                    再读这一行说了什么 */}
                <span className="w-10 shrink-0 text-right text-fine text-ink-2">
                  {i === 0 ? "" : S.ontology.ruleAnd}
                </span>
                <Dropdown
                  className="flex-1"
                  value={c.predicate_id}
                  onChange={(v) =>
                    setDraft({
                      ...draft,
                      conditions: draft.conditions.map((x, j) =>
                        j === i ? { ...x, predicate_id: v } : x,
                      ),
                    })
                  }
                  options={attributes.map((a) => ({ value: a.id, label: a.label }))}
                />
                <Dropdown
                  className="w-32"
                  value={c.op}
                  onChange={(v) =>
                    setDraft({
                      ...draft,
                      conditions: draft.conditions.map((x, j) =>
                        j === i ? { ...x, op: v, text: "" } : x,
                      ),
                    })
                  }
                  options={OPS.map((o) => ({ value: o.value, label: o.label() }))}
                />
                {operandKind(c.op) !== "none" && (
                  <Input
                    className="w-40"
                    placeholder={S.ontology.ruleOperandPlaceholder(operandKind(c.op))}
                    value={c.text}
                    onChange={(e) =>
                      setDraft({
                        ...draft,
                        conditions: draft.conditions.map((x, j) =>
                          j === i ? { ...x, text: e.target.value } : x,
                        ),
                      })
                    }
                  />
                )}
                <IconButton
                  label={S.ontology.ruleDropCondition}
                  size="sm"
                  onClick={() =>
                    setDraft({
                      ...draft,
                      conditions: draft.conditions.filter((_, j) => j !== i),
                    })
                  }
                >
                  <Trash2 size={11} />
                </IconButton>
              </div>
            ))}
            {/* 一个条件判的是属性的值。没有属性时，「加一个条件」只会加出一行
                选不了东西的空条件——所以这里说清缺的是什么，按钮不摆 */}
            {attributes.length === 0 ? (
              <p className="text-small text-ink-2">
                {S.ontology.ruleNeedsAttribute}
              </p>
            ) : (
              <Button
                size="sm"
                variant="ghost"
                onClick={() =>
                  setDraft({
                    ...draft,
                    conditions: [
                      ...draft.conditions,
                      { predicate_id: attributes[0].id, op: "gt", text: "" },
                    ],
                  })
                }
              >
                <Plus size={11} />
                {S.ontology.ruleAddCondition}
              </Button>
            )}
          </div>
        </Field>

        <Field label={S.ontology.ruleConcludes} className="mb-0">
          <div className="flex flex-wrap items-center gap-2">
            <Dropdown
              className="w-40"
              value={draft.conclusion}
              onChange={(v) =>
                setDraft({ ...draft, conclusion: v as "typing" | "attribute" })
              }
              options={[
                { value: "typing", label: S.ontology.ruleConcludesTyping },
                { value: "attribute", label: S.ontology.ruleConcludesAttribute },
              ]}
            />
            {draft.conclusion === "typing" ? (
              <Dropdown
                className="w-48"
                value={draft.conclude_type_id}
                onChange={(v) => setDraft({ ...draft, conclude_type_id: v })}
                options={classes.map((c) => ({ value: c.id, label: c.label }))}
              />
            ) : (
              <>
                <Dropdown
                  className="w-48"
                  value={draft.conclude_predicate_id}
                  onChange={(v) => setDraft({ ...draft, conclude_predicate_id: v })}
                  options={attributes.map((a) => ({ value: a.id, label: a.label }))}
                />
                <Input
                  className="w-40"
                  value={draft.conclude_value}
                  onChange={(e) =>
                    setDraft({ ...draft, conclude_value: e.target.value })
                  }
                />
              </>
            )}
          </div>
        </Field>
      </div>
    </Dialog>
  );
}

// 图谱页的编辑弹窗：实体（名字、类型）与一条事实的有效区间。
// 实体面板只展示；从前改名的两格嵌在面板头下面、改区间的表单嵌在事实行里，
// 现在各开一扇窗。字段与提交逻辑原样搬过来。
import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, type EntityFact, type GraphNode } from "../api";
import { S } from "../i18n";
import { toast } from "../toast";
import { fmtTime, parseDateInput } from "../time";
import { Field, FormDialog, Input, RadioGroup, SearchSelect } from "../ui";

/* ---------- 实体：抽取给的是初判，判错此前只能整库重抽 ---------- */

export function EntityDialog({
  kbId,
  entityId,
  entity,
  onClose,
  onSaved,
}: {
  kbId: string;
  entityId: string;
  entity: GraphNode;
  onClose: () => void;
  onSaved: () => void;
}) {
  const qc = useQueryClient();
  const [name, setName] = useState(entity.name);
  const [typeId, setTypeId] = useState("");
  // 类型下拉要的是全量本体，不是当前视图里出现过的那几个
  const ontology = useQuery({
    queryKey: ["ontology", kbId],
    queryFn: () => api.ontology(kbId),
  });
  const types = ontology.data?.entity_types ?? [];
  const currentId = types.find((t) => t.key === entity.type_key)?.id ?? "";
  // 本体是异步来的：它到齐时把类型下拉对到当前类型上
  useEffect(() => {
    if (!typeId && currentId) setTypeId(currentId);
  }, [typeId, currentId]);

  const dirty = name.trim() !== entity.name || (!!typeId && typeId !== currentId);
  const save = useMutation({
    mutationFn: () => {
      const body: { type_id?: string; canonical_name?: string } = {};
      if (name.trim() && name.trim() !== entity.name) body.canonical_name = name.trim();
      if (typeId && typeId !== currentId) body.type_id = typeId;
      return api.updateEntity(kbId, entityId, body);
    },
    onSuccess: (r) => {
      toast.success(S.graph.editSaved);
      // 改了类型/名字，图谱节点与本体计数都要跟着动
      qc.invalidateQueries({ queryKey: ["entity", kbId, entityId] });
      qc.invalidateQueries({ queryKey: ["graph", kbId] });
      qc.invalidateQueries({ queryKey: ["ontology", kbId] });
      onSaved();
    },
    onError: (err: Error) => toast.error(err.message),
  });

  return (
    <FormDialog
      title={S.graph.editTitle}
      closeLabel={S.graph.close}
      saveLabel={S.graph.editSave}
      cancelLabel={S.graph.editCancel}
      canSave={dirty && !!name.trim()}
      busy={save.isPending}
      onSave={() => save.mutate()}
      onCancel={onClose}
    >
      <Field
        label={S.graph.editName}
        error={!name.trim() ? S.graph.editEmptyName : undefined}
      >
        <Input
          autoFocus
          className="w-full"
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
      </Field>
      {/* 类可能上千个（schema.org 一装就是 1010 个）：这一格要能打字过滤，
          所以是 SearchSelect 而不是下拉——下拉是给小而有界的枚举的 */}
      <Field label={S.graph.editType} className="mb-0">
        <SearchSelect
          className="w-full"
          value={typeId}
          onChange={setTypeId}
          options={types.map((t) => ({ value: t.id, label: t.label, hint: t.key }))}
        />
      </Field>
    </FormDialog>
  );
}

/* ---------- 一条事实的有效区间（302） ----------
   两端一起提交而不是逐端改：区间的两端互相定义，「清空结束端」与「这次不动
   结束端」得能分辨。结束端的三个选项与账本里的三种写法一一对应，所以这里
   没有「留空即至今」这种隐含约定——那正是 valid_to IS NULL 一度承载两个意思
   的老毛病。 */

export function FactTimeDialog({
  kbId,
  fact,
  onClose,
}: {
  kbId: string;
  fact: EntityFact;
  onClose: () => void;
}) {
  const qc = useQueryClient();
  const [from, setFrom] = useState(
    fmtTime(fact.valid_from, fact.valid_from_precision) ?? "",
  );
  const [to, setTo] = useState(
    fmtTime(fact.valid_to, fact.valid_to_precision) ?? "",
  );
  const [endMode, setEndMode] = useState<"open" | "unknown" | "date">(
    fact.valid_to
      ? "date"
      : fact.valid_to_precision === "unknown"
        ? "unknown"
        : "open",
  );
  const [note, setNote] = useState("");

  const save = useMutation({
    mutationFn: () => {
      const f = from.trim() ? parseDateInput(from) : null;
      if (from.trim() && !f) throw new Error(S.graph.timeBadDate);
      const t = endMode === "date" ? parseDateInput(to) : null;
      if (endMode === "date" && !t) throw new Error(S.graph.timeBadDate);
      return api.updateFactTime(kbId, fact.id, {
        valid_from: f?.iso ?? null,
        valid_from_precision: f?.precision ?? null,
        valid_to: t?.iso ?? null,
        valid_to_precision:
          endMode === "date"
            ? (t?.precision ?? null)
            : endMode === "unknown"
              ? "unknown"
              : null,
        note: note.trim() || undefined,
      });
    },
    onSuccess: (r) => {
      // 对账的后果要说出来：改了起点可能顺手闭合了继任者的开放区间，
      // 也可能撞出一条需要人裁的冲突。不说的话图会自己变而没人知道为什么
      if (r.conflicts) toast.success(S.graph.timeSavedConflicts(r.conflicts));
      else if (r.closed) toast.success(S.graph.timeSavedClosed(r.closed));
      else toast.success(S.graph.timeSaved);
      qc.invalidateQueries({ queryKey: ["entity", kbId] });
      qc.invalidateQueries({ queryKey: ["graph"] });
      qc.invalidateQueries({ queryKey: ["review", kbId] });
      onClose();
    },
    onError: (e: Error) => toast.error(e.message),
  });

  const what = `${fact.predicate_label ?? S.graph.unknownPredicate} → ${
    fact.other_name ?? "?"
  }`;
  return (
    <FormDialog
      width="sm"
      title={S.graph.editTime}
      description={what}
      closeLabel={S.graph.close}
      saveLabel={S.graph.timeSave}
      cancelLabel={S.graph.timeCancel}
      busy={save.isPending}
      onSave={() => save.mutate()}
      onCancel={onClose}
    >
      <Field label={S.graph.timeStart} hint={S.graph.timeFormat}>
        <Input
          autoFocus
          value={from}
          onChange={(e) => setFrom(e.target.value)}
          placeholder={S.graph.timeFormat}
          className="u-num w-full"
        />
      </Field>
      <Field label={S.graph.timeEnd}>
        <RadioGroup
          name={`end-${fact.id}`}
          value={endMode}
          onChange={(v) => setEndMode(v)}
          options={[
            { value: "open" as const, label: S.graph.timeEndOpen },
            { value: "unknown" as const, label: S.graph.timeEndUnknown },
            {
              value: "date" as const,
              label: S.graph.timeEndDate,
              children: (
                <Input
                  size="sm"
                  value={to}
                  onChange={(e) => setTo(e.target.value)}
                  placeholder={S.graph.timeFormat}
                  className="u-num ml-1 flex-1"
                />
              ),
            },
          ]}
        />
      </Field>
      <Field label={S.graph.timeNote} className="mb-0">
        <Input
          value={note}
          onChange={(e) => setNote(e.target.value)}
          placeholder={S.graph.timeNotePlaceholder}
          className="w-full"
        />
      </Field>
    </FormDialog>
  );
}

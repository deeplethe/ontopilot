/* 账户层"我的知识库"：可访问的库 + 我的角色 + 加入信息 + 概览统计，**以及建库**。
   从前建库的入口在 Administration 里，而那一节只有系统管理员看得见——可建库要的
   是工作区 Admin+，于是一个工作区管理员在界面上根本没有建库的门。两处列的又是
   同一份数据（都读 `my-kbs`），所以并成这一页。 */
import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { Lock, Plus, Search } from "lucide-react";
import { api, DEFAULT_ONTOLOGY_PACKS, type MyKb } from "../api";
import { S } from "../i18n";
import { useKb } from "../kb";
import {
  Button,
  Checkbox,
  Chip,
  Dialog,
  Field,
  Input,
  Loading,
  MultiSearchSelect,
  PageHeader,
} from "../ui";

const ymd = (iso: string) => iso.slice(0, 10);

function joinInfo(row: MyKb): string {
  if (row.my_role === "owner") return S.account.deploymentAdmin;
  if (row.joined_at) {
    return row.added_by_name
      ? S.account.addedBy(row.added_by_name, ymd(row.joined_at))
      : S.account.joinedOn(ymd(row.joined_at));
  }
  return S.account.openToEveryone;
}

export function MyKbs() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { workspace, setKb } = useKb();
  // 库切换器最后一行带 ?create=true 过来：落地就开表单
  const { create: deepLink } = useSearch({ from: "/account/account/kbs" });
  const [creating, setCreating] = useState(!!deepLink);
  const [filter, setFilter] = useState("");

  const mine = useQuery({
    queryKey: ["myKbs", workspace?.id],
    queryFn: () => api.myKbs(workspace!.id),
    enabled: !!workspace,
  });
  const me = useQuery({ queryKey: ["me"], queryFn: api.me });
  /* 能不能建库：系统管理员，或者在这个工作区里是 Admin+（api/kbs.rs 的 create
     就是这么判的）。**editor 不能**——它管的是库里的内容，不是有几个库 */
  const wsRole = useQuery({
    queryKey: ["workspaceRole", workspace?.id],
    queryFn: () => api.workspaceRole(workspace!.id),
    enabled: !!workspace,
  });
  const canCreate =
    !!me.data?.is_admin ||
    wsRole.data?.role === "admin" ||
    wsRole.data?.role === "owner";

  if (!workspace || mine.isPending) return <Loading>{S.nav.loading}</Loading>;

  const q = filter.trim().toLowerCase();
  const rows = (mine.data?.kbs ?? []).filter(
    (r) =>
      !q ||
      r.kb.name.toLowerCase().includes(q) ||
      (r.kb.description ?? "").toLowerCase().includes(q),
  );
  const openKb = (id: string) => {
    setKb(id);
    navigate({ to: "/kb/$kbId/graph", params: { kbId: id } });
  };

  return (
    <div className="px-8 py-6">
      {/* 与管理页同一副身材：内容居中限宽，行长不随窗口拉长 */}
      <div className="mx-auto w-full max-w-4xl">
        <PageHeader
          title={S.account.kbsTitle}
          actions={
            canCreate && (
              <Button variant="primary" size="sm" onClick={() => setCreating(true)}>
                <Plus size={12} />
                {S.settings.kbs.newKb}
              </Button>
            )
          }
        />

        {/* 筛这份名单的东西在面板外面（DESIGN.md 6） */}
        <div className="mb-4">
          <Input
            icon={<Search size={13} />}
            className="w-72"
            placeholder={S.account.kbsFilter}
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
          />
        </div>

        {/* 一个面板装多行，不是一行一张卡片（DESIGN.md 6）。一行说清三件事：
            这是哪个库、里面有多少东西、我在里面是什么身份 */}
        <div className="glass rounded-panel divide-y divide-line">
          {rows.map((row) => {
            const canManage = row.my_role === "admin" || row.my_role === "owner";
            return (
              <div key={row.kb.id} className="flex items-center gap-3 px-4 py-3">
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-body text-ink">{row.kb.name}</span>
                    {row.kb.is_default && (
                      <Chip tone="neutral">{S.settings.kbs.defaultChip}</Chip>
                    )}
                    {row.kb.visibility === "restricted" && (
                      <span className="flex items-center gap-1 text-fine text-ink-2">
                        <Lock size={10} />
                        {S.account.kbRestricted}
                      </span>
                    )}
                    {row.my_role && (
                      <Chip tone={canManage ? "info" : "neutral"}>
                        {S.account.roleNames[row.my_role] ?? row.my_role}
                      </Chip>
                    )}
                  </div>
                  <div className="mt-1 truncate text-small text-ink-2">
                    <span className="u-num">
                      {S.account.kbStats(row.doc_count, row.member_count)}
                    </span>
                    <span className="mx-2">·</span>
                    {joinInfo(row)}
                  </div>
                </div>
                {/* 设置在前、打开在后：不是每一行都有设置（要 admin），把总是在的
                    那个放右端，一列按钮的右缘才不会一行一个样 */}
                <div className="flex shrink-0 items-center gap-2">
                  {canManage && (
                    <Button variant="secondary" size="sm"
                      onClick={() => {
                        setKb(row.kb.id);
                        navigate({ to: "/kb/$kbId/settings", params: { kbId: row.kb.id } });
                      }}
                    >
                      {S.account.kbSettingsBtn}
                    </Button>
                  )}
                  <Button variant="secondary" size="sm" onClick={() => openKb(row.kb.id)}>
                    {S.account.openKb}
                  </Button>
                </div>
              </div>
            );
          })}
          {rows.length === 0 && (
            <p className="px-4 py-6 text-body text-ink-2">{S.ui.noMatches}</p>
          )}
        </div>

        {creating && (
          <NewKbModal
            workspaceId={workspace.id}
            onDone={(id) => {
              setCreating(false);
              queryClient.invalidateQueries({ queryKey: ["myKbs", workspace.id] });
              queryClient.invalidateQueries({ queryKey: ["kbs", workspace.id] });
              // 建完直达库设置：下一步几乎总是邀人/配置
              if (id) {
                setKb(id);
                navigate({ to: "/kb/$kbId/settings", params: { kbId: id } });
              }
            }}
          />
        )}
      </div>
    </div>
  );
}

/** 新建知识库弹窗：缺省 restricted，不污染全员切换器。
    **建库不是系统管理员专属**——工作区 Admin 起就能建（见 api/kbs.rs 的
    create），所以它住在人人都会来的这一页，不在 Administration 里 */
function NewKbModal({
  workspaceId,
  onDone,
}: {
  workspaceId: string;
  onDone: (id?: string) => void;
}) {
  const [name, setName] = useState("");
  const [desc, setDesc] = useState("");
  const [restricted, setRestricted] = useState(true);
  // schema.org 默认勾选，可反选（0009）。删掉内置类之后不选任何包的库是真的空，
  // 而空库仍然能用——但绝大多数人要的是一个已经能认出人、组织、产品的起点。
  // 一秒装完（0008 的批量插入），所以默认装得起
  const [packs, setPacks] = useState<string[]>([...DEFAULT_ONTOLOGY_PACKS]);

  const available = useQuery({
    queryKey: ["ontologyPacks"],
    queryFn: api.ontologyPacks,
  });

  // 勾选顺序即安装顺序：第一个包的类会认领同名的种子类，
  // 后面的撞名才查得到对齐表。所以取消再勾会排到末尾——这是对的
  const toggle = (id: string) =>
    setPacks((prev) =>
      prev.includes(id) ? prev.filter((p) => p !== id) : [...prev, id],
    );

  const create = useMutation({
    mutationFn: () =>
      api.createKb(workspaceId, {
        name: name.trim(),
        description: desc.trim() || null,
        visibility: restricted ? "restricted" : "open",
        ontology_packs: packs,
      }),
    onSuccess: (kb) => onDone(kb.id),
  });

  return (
    <Dialog
      open
      onOpenChange={(o) => !o && onDone()}
      title={S.settings.kbs.newKb}
      closeLabel={S.ui.close}
      width="sm"
      footer={
        <>
          <Button variant="secondary" size="sm" onClick={() => onDone()}>
            {S.library.cancel}
          </Button>
          <Button
            variant="primary"
            size="sm"
            disabled={!name.trim() || create.isPending}
            onClick={() => create.mutate()}
          >
            {S.settings.kbs.create}
          </Button>
        </>
      }
    >
      <div>
        <Input className="w-full mb-2"
          autoFocus
          placeholder={S.settings.kbs.name}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <Input className="w-full mb-3"
          placeholder={S.settings.kbs.description}
          value={desc}
          onChange={(e) => setDesc(e.target.value)}
        />
        <Checkbox
          className="mb-4"
          checked={restricted}
          onChange={(e) => setRestricted(e.target.checked)}
          label={S.settings.kbs.visRestricted}
        />

        {/* 包是可选的，而且**多半只装一个**：五张卡片铺开占了这张弹窗一多半，
            换成搜着选——说明与规模挪进下拉里的次要文案，选中的堆在框上面 */}
        <Field label={S.settings.kbs.packsLabel} hint={S.settings.kbs.packsHint}>
          <MultiSearchSelect
            className="w-full"
            values={packs}
            options={(available.data?.packs ?? []).map((p) => ({
              value: p.id,
              label: p.name,
              hint: `${p.summary} · ${S.settings.kbs.packsCount(p.classes, p.properties)}`,
            }))}
            placeholder={S.settings.kbs.packsPick}
            emptyHint={S.settings.kbs.packsNone}
            onToggle={toggle}
          />
        </Field>
        {create.isError && (
          <p className="text-small text-danger mb-2">
            {(create.error as Error).message}
          </p>
        )}
      </div>
    </Dialog>
  );
}


import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useSearch } from "@tanstack/react-router";
import { X } from "lucide-react";
import { api } from "../api";
import { LANG_NAMES, S } from "../i18n";
import { useKb } from "../kb";
import { toast } from "../toast";
import {
  Button,
  Checkbox,
  IconButton,
  Input,
  LinkButton,
  Pill,
  SearchSelect,
  Segmented,
  SettingsCard,
  PageHeader,
} from "../ui";
import { Members } from "./Members";

/** 管理页的五节。**它们是左栏的第二层，不是正文顶上的一条 tab 带**——
    与账户栏那四项是同一种东西（去哪儿），只是矮一级；地址里是 `?tab=`，
    刷新、回退、分享链接都落回同一节。左栏（AccountShell）与这一页的标题
    读的是同一份，名字只有一处。**label 是函数**：界面语言在运行时可切，
    模块顶上取值会把第一次加载时的那门语言焊死 */
export const ADMIN_TABS = [
  { key: "models", label: () => S.settings.tabModels },
  { key: "members", label: () => S.settings.tabMembers },
  { key: "datasources", label: () => S.settings.datasources.tab },
  { key: "deployment", label: () => S.settings.tabDeployment },
] as const;

/** 一个源授权给了哪些工作区（0014）。
 *
 * **授权与挂载是两层**：这里说「这个源可以给谁用」，KB 管理员再在授权过的
 * 集合里挑挂不挂。从前没有这一层——可挂载列表返回全部署每一个源，于是任何
 * 库的管理员都能把任意生产库挂进自己库。
 *
 * 两层都是多对多：一个源可授权给多个工作区，一个工作区可拿到多个源。 */
function SourceGrants({ sourceId }: { sourceId: string }) {
  const queryClient = useQueryClient();
  const [picked, setPicked] = useState("");
  const [notice, setNotice] = useState<string | null>(null);

  const grants = useQuery({
    queryKey: ["dataSourceGrants", sourceId],
    queryFn: () => api.dataSourceGrants(sourceId),
  });
  const workspaces = useQuery({
    queryKey: ["workspaces"],
    queryFn: api.workspaces,
  });
  const invalidate = () =>
    queryClient.invalidateQueries({ queryKey: ["dataSourceGrants", sourceId] });

  const grant = useMutation({
    mutationFn: (wsId: string) => api.grantDataSource(sourceId, wsId),
    onSuccess: () => {
      setPicked("");
      setNotice(null);
      invalidate();
    },
    onError: (e: unknown) => toast.error((e as Error).message),
  });
  const revoke = useMutation({
    mutationFn: (wsId: string) => api.revokeDataSource(sourceId, wsId),
    // 卸了几个要说出来：收回授权会顺带断掉正在用的挂载，
    // 悄悄断比断本身更糟
    onSuccess: (r) => {
      setNotice(S.settings.datasources.grantRevoked(r.unmounted));
      invalidate();
    },
    onError: (e: unknown) => toast.error((e as Error).message),
  });

  const granted = grants.data?.workspaces ?? [];
  const grantedIds = new Set(granted.map((w) => w.id));
  const grantable = (workspaces.data ?? []).filter(
    (w) => !grantedIds.has(w.id),
  );

  return (
    <div className="border-t border-line pt-3 space-y-2">
      <div className="flex items-baseline gap-2">
        <span className="text-fine text-ink-2">
          {S.settings.datasources.grants}
        </span>
        {granted.length === 0 ? (
          <span className="text-fine text-ink-2">
            {S.settings.datasources.grantsNone}
          </span>
        ) : (
          <div className="flex flex-wrap gap-2">
            {granted.map((w) => (
              <span
                key={w.id}
                className="u-chip u-chip-neutral text-fine flex items-center gap-1"
              >
                {w.name}
                <IconButton size="sm" label={S.settings.datasources.grantRevoke}
                  disabled={revoke.isPending}
                  onClick={() => revoke.mutate(w.id)}
                >
                  <X size={10} />
                </IconButton>
              </span>
            ))}
          </div>
        )}
      </div>
      {grantable.length > 0 && (
        <div className="flex items-center gap-2">
          <SearchSelect
            className="flex-1"
            value={picked}
            options={grantable.map((w) => ({ value: w.id, label: w.name }))}
            onChange={(v) => {
              setPicked(v);
              if (v) grant.mutate(v);
            }}
            placeholder={S.settings.datasources.grantAdd}
          />
        </div>
      )}
      {notice && <p className="text-fine text-ink-2">{notice}</p>}
    </div>
  );
}

/** 部署级配置：注册开关 + worker 并发。 */
function DeploymentAdmin() {
  const queryClient = useQueryClient();
  const dep = useQuery({
    queryKey: ["deployment"],
    queryFn: api.adminDeployment,
  });
  const [workers, setWorkers] = useState<number | null>(null);
  const shown = workers ?? dep.data?.worker_concurrency ?? 32;
  // 按模型的并发：缺省值 + 每个在用模型的覆盖
  const [modelDefault, setModelDefault] = useState<number | null>(null);
  const shownDefault =
    modelDefault ?? dep.data?.default_model_concurrency ?? 10;
  const [perModel, setPerModel] = useState<Record<string, number>>({});
  const save = useMutation({
    mutationFn: (v: {
      open: boolean;
      workers?: number;
      defaultModel?: number;
      modelLimit?: {
        base_url: string;
        model: string;
        max_concurrent: number | null;
      };
      ontologyLang?: "en" | "zh";
    }) =>
      api.saveAdminDeployment(
        v.open,
        v.workers,
        v.defaultModel,
        v.modelLimit,
        v.ontologyLang,
      ),
    onSuccess: () => {
      setPerModel({});
      queryClient.invalidateQueries({ queryKey: ["deployment"] });
    },
  });
  const open = dep.data?.open_registration ?? true;

  return (
    /* 一件事一张卡，与库设置同一副排法。改即生效的（注册开关、默认本体语言）
       没有底栏——它们没有"保存"这一步；数字要按一下才算数的，按钮在底栏 */
    <div className="space-y-4">
      <SettingsCard title={S.settings.cardAccounts}>
        <Checkbox
          checked={open}
          disabled={dep.isPending || save.isPending}
          onChange={(e) => save.mutate({ open: e.target.checked })}
          label={S.settings.deployment.openReg}
          hint={S.settings.deployment.openRegHint}
        />
      </SettingsCard>

      {/* 新建库的本体语言。**不是界面语言**——界面语言是每个人自己在账户菜单里选的，
          根本不经过后端（docs/decisions/0004）。说明里必须把这句讲出来 */}
      <SettingsCard
        title={S.settings.deployment.ontologyLang}
        hint={S.settings.deployment.ontologyLangHint}
      >
        <Segmented
          size="sm"
          className="w-fit"
          disabled={dep.isPending || save.isPending}
          value={dep.data?.default_ontology_lang ?? "en"}
          onChange={(l) => save.mutate({ open, ontologyLang: l })}
          options={(["en", "zh"] as const).map((l) => ({
            value: l,
            label: LANG_NAMES[l],
          }))}
        />
      </SettingsCard>

      <SettingsCard
        title={S.settings.deployment.workers}
        hint={S.settings.deployment.workersHint}
        action={
          <>
            <Input size="sm" className="u-input-plain w-16 u-num text-center"
              type="number"
              min={1}
              max={32}
              value={shown}
              disabled={dep.isPending}
              onChange={(e) =>
                setWorkers(Math.max(1, Math.min(32, Number(e.target.value) || 1)))
              }
            />
            <Button variant="secondary" size="sm"
              disabled={
                save.isPending ||
                workers === null ||
                workers === dep.data?.worker_concurrency
              }
              onClick={() => save.mutate({ open, workers: shown })}
            >
              {S.settings.deployment.workersApply}
            </Button>
          </>
        }
      />

      {/* 按模型的并发才是真正的节流：约束来自供应商的速率限制，而那是按模型算的。
          上面那个 worker 并发只是外层兜底，防任务无限堆积 */}
      <SettingsCard
        title={S.settings.deployment.modelConcurrency}
        hint={S.settings.deployment.modelConcurrencyHint}
        note={S.settings.deployment.modelDefault}
        action={
          <>
            <Input size="sm" className="u-input-plain w-16 u-num text-center"
              type="number"
              min={1}
              max={256}
              value={shownDefault}
              disabled={dep.isPending}
              onChange={(e) =>
                setModelDefault(
                  Math.max(1, Math.min(256, Number(e.target.value) || 1)),
                )
              }
            />
            <Button variant="secondary" size="sm"
              disabled={
                save.isPending ||
                modelDefault === null ||
                modelDefault === dep.data?.default_model_concurrency
              }
              onClick={() => save.mutate({ open, defaultModel: shownDefault })}
            >
              {S.settings.deployment.workersApply}
            </Button>
          </>
        }
      >
        {/* 在用的模型各自一行：这一行的数字与按钮是这一行的事，不归底栏 */}
        {!!dep.data?.models_in_use?.length && (
          <div className="divide-y divide-line">
            {dep.data.models_in_use.map((m) => {
              const cur =
                dep.data?.model_limits?.find(
                  (l) => l.base_url === m.base_url && l.model === m.model,
                )?.max_concurrent ?? null;
              const key = `${m.base_url}|${m.model}`;
              const val = perModel[key] ?? cur ?? shownDefault;
              return (
                <div
                  key={key}
                  className="flex items-center gap-2 py-3 text-small first:pt-0"
                >
                  <span className="u-chip u-chip-neutral !text-fine !px-2 shrink-0">
                    {m.kind}
                  </span>
                  <span className="font-mono text-ink-2 truncate">{m.model}</span>
                  <span className="text-ink-2 truncate hidden sm:inline">
                    {m.base_url}
                  </span>
                  {/* 数字框与按钮包成一组，**和底栏那一组同一副几何**
                      （w-16、gap-3）：右缘都顶到卡的内距，间距一差 4px，
                      同一列上下两个框的左缘就错开，看着就是内距没对齐 */}
                  <div className="ml-auto flex shrink-0 items-center gap-3">
                    <Input size="sm" className="u-input-plain w-16 u-num text-center"
                      type="number"
                      min={1}
                      max={256}
                      value={val}
                      onChange={(e) =>
                        setPerModel({
                          ...perModel,
                          [key]: Math.max(1, Math.min(256, Number(e.target.value) || 1)),
                        })
                      }
                    />
                    <Button variant="secondary" size="sm"
                      disabled={save.isPending || perModel[key] === undefined}
                      onClick={() =>
                        save.mutate({
                          open,
                          modelLimit: {
                            base_url: m.base_url,
                            model: m.model,
                            max_concurrent: val,
                          },
                        })
                      }
                    >
                      {S.settings.deployment.workersApply}
                    </Button>
                    {cur !== null && (
                      <Button variant="secondary" size="sm"
                        disabled={save.isPending}
                        title={S.settings.deployment.modelResetHint}
                        onClick={() =>
                          save.mutate({
                            open,
                            modelLimit: {
                              base_url: m.base_url,
                              model: m.model,
                              max_concurrent: null,
                            },
                          })
                        }
                      >
                        {S.settings.deployment.modelReset}
                      </Button>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </SettingsCard>

      {save.isError && (
        <p className="text-small text-danger">{(save.error as Error).message}</p>
      )}
    </div>
  );
}

/** 知识库管理（部署层）：全部库总览 + 新建（建库是管理动作，切换器只切换）。 */

/** 系统层数据源注册（问数）：凭据只进不出，列表只显示 host:port/db 摘要。 */
function DataSourcesAdmin() {
  const queryClient = useQueryClient();
  const list = useQuery({
    queryKey: ["dataSources"],
    queryFn: api.adminDataSources,
  });
  const [name, setName] = useState("");
  const [conn, setConn] = useState("");
  const invalidate = () =>
    queryClient.invalidateQueries({ queryKey: ["dataSources"] });

  const create = useMutation({
    mutationFn: () =>
      api.adminCreateDataSource({
        name: name.trim(),
        conn_string: conn.trim(),
      }),
    onSuccess: () => {
      setName("");
      setConn("");
      invalidate();
    },
  });
  const remove = useMutation({
    mutationFn: (id: string) => api.adminDeleteDataSource(id),
    onSettled: invalidate,
  });
  const test = useMutation({
    mutationFn: (id: string) => api.adminTestDataSource(id),
    onSettled: invalidate,
  });

  return (
    <div className="space-y-4">
      <p className="text-small text-ink-2">{S.settings.datasources.hint}</p>

      <div className="glass rounded-panel divide-y divide-line">
        {(list.data?.data_sources ?? []).map((d) => (
          <div key={d.id} className="px-4 py-3 space-y-3">
            <div className="flex items-center gap-3">
              <div className="min-w-0 flex-1">
                <div className="text-body text-ink">
                  {d.name}
                  <span className="ml-2 text-fine text-ink-2">
                    {d.engine}
                  </span>
                </div>
                <div className="text-small text-ink-2 font-mono truncate">
                  {d.summary}
                </div>
              </div>
              <span
                className={`u-chip shrink-0 ${
                  d.last_test_ok === true
                    ? "u-chip-success"
                    : d.last_test_ok === false
                      ? "u-chip-danger"
                      : "u-chip-neutral"
                }`}
              >
                {d.last_test_ok === true
                  ? S.settings.datasources.testOk
                  : d.last_test_ok === false
                    ? S.settings.datasources.testFail
                    : S.settings.datasources.neverTested}
              </span>
              <Button variant="secondary" size="sm" className="shrink-0"
                disabled={test.isPending}
                onClick={() => test.mutate(d.id)}
              >
                {S.settings.datasources.test}
              </Button>
              <LinkButton
                tone="danger"
                className="shrink-0"
                disabled={remove.isPending}
                onClick={() => remove.mutate(d.id)}
              >
                {S.settings.datasources.remove}
              </LinkButton>
            </div>
            <SourceGrants sourceId={d.id} />
          </div>
        ))}
        {list.data?.data_sources.length === 0 && (
          <p className="px-4 py-6 text-body text-ink-2">
            {S.settings.datasources.empty}
          </p>
        )}
      </div>

      <div className="glass rounded-panel p-4">
        <div className="max-w-xl space-y-2">
          <Input className="w-full"
            placeholder={S.settings.datasources.name}
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
          <Input className="w-full font-mono u-placeholder-sans"
            placeholder={S.settings.datasources.connString}
            value={conn}
            onChange={(e) => setConn(e.target.value)}
          />
          <p className="text-fine leading-5 text-ink-2 font-mono whitespace-pre-line">
            {S.settings.datasources.connSchemes}
          </p>
          <div className="flex items-center gap-3">
            <Button variant="primary" size="sm"
              disabled={!name.trim() || !conn.trim() || create.isPending}
              onClick={() => create.mutate()}
            >
              {S.settings.datasources.add}
            </Button>
            {create.isError && (
              <span className="text-small text-danger">
                {(create.error as Error).message}
              </span>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

const PRESETS: Record<
  string,
  { chat: string; embed: string; chatModel: string; embedModel: string }
> = {
  DeepSeek: {
    chat: "https://api.deepseek.com/v1",
    chatModel: "deepseek-chat",
    embed: "",
    embedModel: "",
  },
  SiliconFlow: {
    chat: "https://api.siliconflow.cn/v1",
    chatModel: "deepseek-ai/DeepSeek-V3",
    embed: "https://api.siliconflow.cn/v1",
    embedModel: "BAAI/bge-m3",
  },
  "Qwen (DashScope)": {
    chat: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    chatModel: "qwen-plus",
    embed: "https://dashscope.aliyuncs.com/compatible-mode/v1",
    embedModel: "text-embedding-v3",
  },
  Ollama: {
    chat: "http://localhost:11434/v1",
    chatModel: "qwen2.5:7b",
    embed: "http://localhost:11434/v1",
    embedModel: "bge-m3",
  },
  OpenAI: {
    chat: "https://api.openai.com/v1",
    chatModel: "gpt-4o-mini",
    embed: "https://api.openai.com/v1",
    embedModel: "text-embedding-3-small",
  },
};

export function Settings() {
  const { workspace } = useKb();
  /* 人在哪一节，**地址说了算**：左栏第二层是一组 Link（见 AccountShell），
     刷新、回退、把链接发给同事都落回同一节。从前这里另存一份 state，
     于是地址与页面各说各的 */
  const { tab: tabParam } = useSearch({ from: "/account/admin" });
  const tab = tabParam ?? "models";
  const queryClient = useQueryClient();
  const settings = useQuery({
    queryKey: ["settings", workspace?.id],
    queryFn: () => api.settings(workspace!.id),
    enabled: !!workspace,
  });

  const [form, setForm] = useState({
    chat_base_url: "",
    chat_api_key: "",
    chat_model: "",
    embed_base_url: "",
    embed_api_key: "",
    embed_model: "",
  });

  useEffect(() => {
    if (settings.data) {
      setForm((f) => ({
        ...f,
        chat_base_url: settings.data.chat_base_url ?? "",
        chat_model: settings.data.chat_model ?? "",
        embed_base_url: settings.data.embed_base_url ?? "",
        embed_model: settings.data.embed_model ?? "",
      }));
    }
  }, [settings.data]);

  /** 一张卡一个保存。**PUT 是整体替换**（`llm_settings` 的 upsert 只对两个
      密钥做 COALESCE，其余列直接取 EXCLUDED），所以不能只送这张卡的三项——
      那会把另一半清成空。底子取服务端那一份、再把这张卡的字段盖上去：
      既不会清空邻居，也不会把邻居那张卡还没保存的编辑一起交上去。
      密钥不在底子里：留空 = 保留旧密钥，这是 COALESCE 那两列的用法 */
  const withSaved = (over: Record<string, unknown>) => ({
    chat_base_url: settings.data?.chat_base_url ?? "",
    chat_model: settings.data?.chat_model ?? "",
    embed_base_url: settings.data?.embed_base_url ?? "",
    embed_model: settings.data?.embed_model ?? "",
    ...over,
  });
  const usePatch = () =>
    // eslint-disable-next-line react-hooks/rules-of-hooks
    useMutation({
      mutationFn: (patch: Record<string, unknown>) =>
        api.saveSettings(workspace!.id, patch),
      onSuccess: () =>
        queryClient.invalidateQueries({ queryKey: ["settings", workspace?.id] }),
    });
  const saveChat = usePatch();
  const saveEmbed = usePatch();

  const test = useMutation({
    mutationFn: () => api.testSettings(workspace!.id),
  });

  if (!workspace)
    return <div className="p-8 text-body text-ink-2">{S.nav.loading}</div>;

  const set =
    (k: keyof typeof form) => (e: React.ChangeEvent<HTMLInputElement>) =>
      setForm({ ...form, [k]: e.target.value });

  const label = "block text-small font-medium text-ink-2 mb-1";

  return (
    <div className="h-full overflow-y-auto u-scroll px-8 py-6">
      {/* 同 KbSettings：设置是读一列字段，居中限宽，行长不随窗口拉长 */}
      <div className="mx-auto w-full max-w-4xl">
        {/* 标题是这一节的名字，不是「Administration」——左栏已经说了人在
            管理区，页顶再说一遍等于每一节的标题都一样 */}
        <PageHeader
          title={ADMIN_TABS.find((t) => t.key === tab)?.label() ?? S.settings.title}
        />

        {tab === "members" && <Members workspaceId={workspace.id} />}
        {tab === "datasources" && <DataSourcesAdmin />}
        {tab === "deployment" && <DeploymentAdmin />}

        {tab === "models" && (
          /* 两张卡，两个保存：聊天模型与嵌入模型是两套凭据，改一套不该把另一套
             一起送上去。服务端对缺席与空串都当"这项不改"，所以每张卡只送自己
             那三项就够了 */
          <div className="space-y-4">
            <p className="text-body text-ink-2">{S.settings.modelsIntro}</p>

            {/* 预设一按填满两张卡的字段：它不是设置本身，所以在卡外面 */}
            <div className="flex flex-wrap gap-2">
              {Object.entries(PRESETS).map(([name, p]) => (
                <Pill
                  key={name}
                  onClick={() =>
                    setForm({
                      ...form,
                      chat_base_url: p.chat,
                      chat_model: p.chatModel,
                      embed_base_url: p.embed,
                      embed_model: p.embedModel,
                    })
                  }
                >
                  {name}
                </Pill>
              ))}
            </div>

            <SettingsCard
              title={S.settings.chatModel}
              note={
                test.data ? (
                  <span className={test.data.chat.ok ? "text-accent" : "text-danger"}>
                    {test.data.chat.ok
                      ? S.settings.ok(test.data.chat.reply ?? "OK")
                      : test.data.chat.error}
                  </span>
                ) : saveChat.isError ? (
                  <span className="text-danger">{(saveChat.error as Error).message}</span>
                ) : saveChat.isSuccess ? (
                  S.settings.saved
                ) : undefined
              }
              action={
                <>
                  {/* 试一次连的是两套模型（一个接口），结果各自回到各自那张卡 */}
                  <Button variant="secondary" size="sm"
                    onClick={() => test.mutate()}
                    disabled={test.isPending}
                  >
                    {test.isPending ? S.settings.testing : S.settings.test}
                  </Button>
                  <Button variant="secondary" size="sm"
                    onClick={() =>
                      saveChat.mutate(
                        withSaved({
                          chat_base_url: form.chat_base_url,
                          chat_model: form.chat_model,
                          chat_api_key: form.chat_api_key,
                        }),
                      )
                    }
                    disabled={saveChat.isPending}
                  >
                    {saveChat.isPending ? S.settings.saving : S.settings.save}
                  </Button>
                </>
              }
            >
              <div className="space-y-3">
                <div>
                  <label className={label}>{S.settings.baseUrl}</label>
                  <Input
                    className="w-full"
                    placeholder="https://api.deepseek.com/v1"
                    value={form.chat_base_url}
                    onChange={set("chat_base_url")}
                  />
                </div>
                <div className="grid grid-cols-2 gap-3">
                  <div>
                    <label className={label}>{S.settings.model}</label>
                    <Input
                      className="w-full"
                      placeholder="deepseek-chat"
                      value={form.chat_model}
                      onChange={set("chat_model")}
                    />
                  </div>
                  <div>
                    <label className={label}>
                      {S.settings.apiKey}{" "}
                      {settings.data?.has_chat_key && (
                        <span className="text-accent">{S.settings.keyConfigured}</span>
                      )}
                    </label>
                    <Input
                      className="w-full"
                      type="password"
                      placeholder="sk-…"
                      value={form.chat_api_key}
                      onChange={set("chat_api_key")}
                    />
                  </div>
                </div>
              </div>
            </SettingsCard>

            <SettingsCard
              title={S.settings.embedModel}
              note={
                test.data ? (
                  <span className={test.data.embed.ok ? "text-accent" : "text-ink-2"}>
                    {test.data.embed.ok
                      ? S.settings.okDim(test.data.embed.dim ?? 0)
                      : test.data.embed.error}
                  </span>
                ) : saveEmbed.isError ? (
                  <span className="text-danger">{(saveEmbed.error as Error).message}</span>
                ) : saveEmbed.isSuccess ? (
                  S.settings.saved
                ) : undefined
              }
              action={
                <>
                  <Button variant="secondary" size="sm"
                    onClick={() => test.mutate()}
                    disabled={test.isPending}
                  >
                    {test.isPending ? S.settings.testing : S.settings.test}
                  </Button>
                  <Button variant="secondary" size="sm"
                    onClick={() =>
                      saveEmbed.mutate(
                        withSaved({
                          embed_base_url: form.embed_base_url,
                          embed_model: form.embed_model,
                          embed_api_key: form.embed_api_key,
                        }),
                      )
                    }
                    disabled={saveEmbed.isPending}
                  >
                    {saveEmbed.isPending ? S.settings.saving : S.settings.save}
                  </Button>
                </>
              }
            >
              <div className="space-y-3">
                <div>
                  <label className={label}>{S.settings.baseUrl}</label>
                  <Input
                    className="w-full"
                    placeholder="http://localhost:11434/v1"
                    value={form.embed_base_url}
                    onChange={set("embed_base_url")}
                  />
                </div>
                <div className="grid grid-cols-2 gap-3">
                  <div>
                    <label className={label}>{S.settings.model}</label>
                    <Input
                      className="w-full"
                      placeholder="bge-m3"
                      value={form.embed_model}
                      onChange={set("embed_model")}
                    />
                  </div>
                  <div>
                    <label className={label}>
                      {S.settings.apiKey}{" "}
                      {settings.data?.has_embed_key && (
                        <span className="text-accent">{S.settings.keyConfigured}</span>
                      )}
                    </label>
                    <Input
                      className="w-full"
                      type="password"
                      value={form.embed_api_key}
                      onChange={set("embed_api_key")}
                    />
                  </div>
                </div>
              </div>
            </SettingsCard>
          </div>
        )}
      </div>
    </div>
  );
}

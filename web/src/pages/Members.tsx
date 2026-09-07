import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "../api";
import { S } from "../i18n";
import { toast } from "../toast";
import {
  Button,
  Chip,
  DangerConfirm,
  Dropdown,
  Input,
  LinkButton,
  Pager,
  pageSlice,
  SearchSelect,
  SettingsCard,
} from "../ui";

const ROLES = ["owner", "admin", "editor", "viewer"] as const;
const ROLE_OPTIONS = ROLES.map((r) => ({ value: r, label: S.members.roles[r] }));
const MEMBER_PAGE = 10;

export function Members({ workspaceId }: { workspaceId: string }) {
  const queryClient = useQueryClient();
  const [addUserId, setAddUserId] = useState("");
  const [addRole, setAddRole] = useState("viewer");
  const [memberPage, setMemberPage] = useState(0);
  const [filter, setFilter] = useState("");
  const me = useQuery({ queryKey: ["me"], queryFn: api.me });
  const [error, setError] = useState<string | null>(null);

  const members = useQuery({
    queryKey: ["members", workspaceId],
    queryFn: () => api.members(workspaceId),
  });
  const orgUsers = useQuery({ queryKey: ["orgUsers"], queryFn: api.orgUsers });

  const refresh = () => {
    setError(null);
    queryClient.invalidateQueries({ queryKey: ["members", workspaceId] });
  };
  const onError = (e: unknown) => setError((e as Error).message);

  const setRole = useMutation({
    mutationFn: ({ userId, role }: { userId: string; role: string }) =>
      api.setMemberRole(workspaceId, userId, role),
    onSuccess: refresh,
    onError,
  });
  const remove = useMutation({
    mutationFn: (userId: string) => api.removeMember(workspaceId, userId),
    onSuccess: refresh,
    onError,
  });
  // 停用先问一句——用全站的对话框而不是浏览器原生 confirm()
  const [deactivating, setDeactivating] = useState<{ id: string; name: string } | null>(
    null,
  );
  const deactivate = useMutation({
    mutationFn: (userId: string) => api.adminDeactivateUser(userId),
    onSuccess: refresh,
    onError,
  });

  const memberIds = new Set(members.data?.map((m) => m.user_id));
  const addable = orgUsers.data?.filter((u) => !memberIds.has(u.id)) ?? [];
  const q = filter.trim().toLowerCase();
  const memberList = (members.data ?? []).filter(
    (m) =>
      !q || m.display_name.toLowerCase().includes(q) || m.email.toLowerCase().includes(q),
  );
  const { rows: pagedMembers, safe: safeMemberPage } = pageSlice(memberList, memberPage, MEMBER_PAGE);

  return (
    /* 三张卡：名单、开账号、停用过的账号。从前是一张大面板把三件事装在一起，
       于是「移出工作区」「停用账号」「新建用户」读起来像同一件事的三个步骤 */
    <div className="space-y-4">
      {/* 过滤这个名单的东西在卡外面（DESIGN.md 6）：它不是名单的内容，
          而且筛空了的时候，那张卡要能变成空态，不能把改筛选的唯一办法一起带走 */}
      <div className="flex items-center justify-end">
        <Input size="sm" className="w-56"
          placeholder={S.settings.searchUsers}
          value={filter}
          onChange={(e) => {
            setFilter(e.target.value);
            setMemberPage(0);
          }}
        />
      </div>

      {error && <p className="text-body text-danger">{error}</p>}

      <SettingsCard
        title={S.members.title}
        note={
          /* picker 常驻。理由同 KbSettings 里那段：控件消失读作"坏了"，
             而不是"没人可加"；空列表 SearchSelect 自己会说 */
          <SearchSelect
            className="w-full max-w-sm"
            value={addUserId}
            onChange={setAddUserId}
            placeholder={S.members.pickUser}
            options={addable.map((u) => ({
              value: u.id,
              label: u.display_name,
              hint: u.email,
            }))}
          />
        }
        action={
          <>
            <Dropdown
              className="w-28"
              value={addRole}
              onChange={setAddRole}
              options={ROLE_OPTIONS}
            />
            <Button variant="primary" size="sm"
              onClick={() => addUserId && setRole.mutate({ userId: addUserId, role: addRole })}
              disabled={!addUserId}
            >
              {S.members.add}
            </Button>
          </>
        }
      >
        <div className="divide-y divide-line">
          {pagedMembers.map((m) => (
            <div key={m.user_id} className="flex items-center gap-3 py-3 first:pt-0">
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className="truncate text-body text-ink">{m.display_name}</span>
                  {m.is_admin && <Chip tone="info">{S.members.systemAdmin}</Chip>}
                </div>
                <div className="truncate text-small text-ink-2">{m.email}</div>
              </div>
              <Dropdown
                size="sm"
                className="w-24"
                value={m.role}
                onChange={(role) => setRole.mutate({ userId: m.user_id, role })}
                options={ROLE_OPTIONS}
              />
              <div className="flex shrink-0 items-center gap-3 whitespace-nowrap">
                <LinkButton tone="danger" onClick={() => remove.mutate(m.user_id)}>
                  {S.members.remove}
                </LinkButton>
                {/* 停用账号跟「移出工作区」是两件事：前者断掉整个系统的访问，
                    后者只是这个工作区不再有他。所以分开两个按钮，而且停用
                    只给管理员看——它的影响面大得多 */}
                {me.data?.is_admin && me.data.id !== m.user_id && (
                  <LinkButton
                    tone="danger"
                    onClick={() =>
                      setDeactivating({ id: m.user_id, name: m.display_name })
                    }
                    title={S.members.deactivateHint}
                  >
                    {S.members.deactivate}
                  </LinkButton>
                )}
              </div>
            </div>
          ))}
        </div>
        <Pager
          total={memberList.length}
          pageSize={MEMBER_PAGE}
          page={safeMemberPage}
          onPage={setMemberPage}
        />
      </SettingsCard>

      {me.data?.is_admin && <CreateUser onCreated={refresh} />}
      {me.data?.is_admin && <DeactivatedUsers onChanged={refresh} />}
      {deactivating && (
        <DangerConfirm
          title={S.members.deactivate}
          hint={S.members.deactivateConfirm(deactivating.name)}
          confirmLabel={S.members.deactivate}
          cancelLabel={S.members.cancel}
          busy={deactivate.isPending}
          onConfirm={() => {
            deactivate.mutate(deactivating.id);
            setDeactivating(null);
          }}
          onCancel={() => setDeactivating(null)}
        />
      )}
    </div>
  );
}

/** 已停用的账号，以及恢复它们。
 *
 * **这一块存在的理由是「否则恢复够不着」**：停用之后那个人从成员表、选人器、
 * 每一个列表里消失，管理员拿不到他的 id，而恢复接口要的正是那个 id。
 *
 * 一个都没有时整块不出现——没有停用过的部署不该看到一个永远空的区块。
 */
function DeactivatedUsers({ onChanged }: { onChanged: () => void }) {
  const list = useQuery({
    queryKey: ["deactivatedUsers"],
    queryFn: api.deactivatedUsers,
  });
  const queryClient = useQueryClient();
  const revive = useMutation({
    mutationFn: (userId: string) => api.adminReactivateUser(userId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["deactivatedUsers"] });
      onChanged();
    },
    onError: (e: Error) => toast.error(e.message),
  });

  const users = list.data ?? [];
  if (users.length === 0) return null;

  return (
    <SettingsCard
      title={S.members.deactivatedTitle}
      hint={S.members.deactivatedHint}
    >
      {/* 一人一行：恢复是每一行自己的动作，所以按钮在行里，不在底栏 */}
      <div className="divide-y divide-line">
        {users.map((u) => (
          <div key={u.id} className="flex items-center gap-3 py-3 first:pt-0">
            <div className="min-w-0 flex-1">
              <div className="truncate text-body text-ink-2">{u.display_name}</div>
              <div className="truncate text-small text-ink-2">{u.email}</div>
            </div>
            <Button variant="secondary" size="sm"
              disabled={revive.isPending}
              onClick={() => revive.mutate(u.id)}
            >
              {S.members.reactivate}
            </Button>
          </div>
        ))}
      </div>
    </SettingsCard>
  );
}
/** 管理员代开账号（注册关闭后的唯一入口）。 */
function CreateUser({ onCreated }: { onCreated: () => void }) {
  const queryClient = useQueryClient();
  const [email, setEmail] = useState("");
  const [name, setName] = useState("");
  const [password, setPassword] = useState("");
  const [role, setRole] = useState("editor");
  const [error, setError] = useState<string | null>(null);

  const create = useMutation({
    mutationFn: () =>
      api.adminCreateUser({ email: email.trim(), display_name: name.trim(), password, role }),
    onSuccess: () => {
      setEmail("");
      setName("");
      setPassword("");
      setError(null);
      queryClient.invalidateQueries({ queryKey: ["orgUsers"] });
      onCreated();
    },
    onError: (e) => setError((e as Error).message),
  });

  const valid = email.includes("@") && name.trim() && password.length >= 8;

  return (
    <SettingsCard
      title={S.settings.newUser}
      note={error && <span className="text-danger">{error}</span>}
      action={
        <Button variant="secondary" size="sm"
          disabled={!valid || create.isPending}
          onClick={() => create.mutate()}
        >
          {S.settings.createUserBtn}
        </Button>
      }
    >
      <div className="grid grid-cols-2 gap-2">
        <Input
          placeholder={S.login.email}
          value={email}
          onChange={(e) => setEmail(e.target.value)}
        />
        <Input
          placeholder={S.login.displayName}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <Input
          type="password"
          placeholder={S.settings.initialPassword}
          value={password}
          onChange={(e) => setPassword(e.target.value)}
        />
        <Dropdown
          value={role}
          onChange={setRole}
          options={[
            { value: "admin", label: S.members.roles.admin },
            { value: "editor", label: S.members.roles.editor },
            { value: "viewer", label: S.members.roles.viewer },
          ]}
        />
      </div>
    </SettingsCard>
  );
}

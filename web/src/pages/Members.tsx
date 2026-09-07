import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus } from "lucide-react";
import { api } from "../api";
import { S } from "../i18n";
import {
  Button,
  Chip,
  DangerConfirm,
  Dialog,
  Dropdown,
  Field,
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

/** 名单里的一行。停用的账号也是一行——它没有工作区角色，别的都一样 */
type Person = {
  user_id: string;
  display_name: string;
  email: string;
  role: string;
  is_admin: boolean;
  deactivated: boolean;
};

export function Members({ workspaceId }: { workspaceId: string }) {
  const queryClient = useQueryClient();
  const [addUserId, setAddUserId] = useState("");
  const [addRole, setAddRole] = useState("viewer");
  const [memberPage, setMemberPage] = useState(0);
  const [filter, setFilter] = useState("");
  // 看在用的、看停用的，还是都看。缺省只看在用的：那是这一页平时的问题
  const [status, setStatus] = useState<"all" | "active" | "deactivated">("active");
  const [creating, setCreating] = useState(false);
  const me = useQuery({ queryKey: ["me"], queryFn: api.me });
  const [error, setError] = useState<string | null>(null);

  const members = useQuery({
    queryKey: ["members", workspaceId],
    queryFn: () => api.members(workspaceId),
  });
  const orgUsers = useQuery({ queryKey: ["orgUsers"], queryFn: api.orgUsers });
  /* 停用的账号是部署级的，成员表里查不到（停用之后那个人从成员表、选人器、
     每一个列表里消失）——所以另取一次，再并进同一份名单。**只有管理员看得到**：
     恢复也只有他能做 */
  const deactivated = useQuery({
    queryKey: ["deactivatedUsers"],
    queryFn: api.deactivatedUsers,
    enabled: !!me.data?.is_admin,
  });

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
    onSuccess: () => {
      refresh();
      queryClient.invalidateQueries({ queryKey: ["deactivatedUsers"] });
    },
    onError,
  });
  const revive = useMutation({
    mutationFn: (userId: string) => api.adminReactivateUser(userId),
    onSuccess: () => {
      refresh();
      queryClient.invalidateQueries({ queryKey: ["deactivatedUsers"] });
      queryClient.invalidateQueries({ queryKey: ["orgUsers"] });
    },
    onError,
  });

  const memberIds = new Set(members.data?.map((m) => m.user_id));
  const addable = orgUsers.data?.filter((u) => !memberIds.has(u.id)) ?? [];

  /* **停用不是另一张表，是这份名单里的一种状态。**
     从前它单独一块挂在页尾：同一个人在两个地方各出现一次，而"这个账号还在不在"
     恰恰是看名单时最先要问的一件事。合成一份之后，它变成一个可筛的状态 */
  const people: Person[] = [
    ...(members.data ?? []).map((m) => ({ ...m, deactivated: false })),
    ...(deactivated.data ?? []).map((u) => ({
      user_id: u.id,
      display_name: u.display_name,
      email: u.email,
      role: "",
      is_admin: u.is_admin,
      deactivated: true,
    })),
  ];
  const q = filter.trim().toLowerCase();
  const memberList = people
    .filter((p) => status === "all" || (status === "deactivated") === p.deactivated)
    .filter(
      (p) =>
        !q ||
        p.display_name.toLowerCase().includes(q) ||
        p.email.toLowerCase().includes(q),
    )
    // 停用的沉底：他们仍然列着，但不该排在还在用的人前面
    .sort((a, b) => Number(a.deactivated) - Number(b.deactivated));
  const { rows: pagedMembers, safe: safeMemberPage } = pageSlice(memberList, memberPage, MEMBER_PAGE);

  return (
    <div className="space-y-4">
      {/* 筛这份名单的东西在卡外面（DESIGN.md 6）：它们不是名单的内容，
          而且筛空了的时候，那张卡要能变成空态，不能把改筛选的唯一办法一起带走 */}
      <div className="flex items-center gap-2">
        <Input size="sm" className="w-56"
          placeholder={S.settings.searchUsers}
          value={filter}
          onChange={(e) => {
            setFilter(e.target.value);
            setMemberPage(0);
          }}
        />
        {me.data?.is_admin && (
          <Dropdown
            className="w-36"
            value={status}
            onChange={(v) => {
              setStatus(v as typeof status);
              setMemberPage(0);
            }}
            options={[
              { value: "active", label: S.members.filterActive },
              { value: "deactivated", label: S.members.filterDeactivated },
              { value: "all", label: S.members.filterAll },
            ]}
          />
        )}
        {me.data?.is_admin && (
          <Button variant="primary" size="sm" className="ml-auto"
            onClick={() => setCreating(true)}
          >
            <Plus size={12} />
            {S.settings.newUser}
          </Button>
        )}
      </div>

      {error && <p className="text-body text-danger">{error}</p>}

      <SettingsCard
        title={S.members.title}
        // 为什么停用的账号还留着：只在看它们的时候说
        hint={status === "deactivated" ? S.members.deactivatedHint : undefined}
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
        {memberList.length === 0 ? (
          <p className="text-small text-ink-2">{S.ui.noMatches}</p>
        ) : (
          <div className="divide-y divide-line">
            {pagedMembers.map((m) => (
              <div
                key={m.user_id}
                className={`flex items-center gap-3 py-3 first:pt-0 ${m.deactivated ? "opacity-55" : ""}`}
              >
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-body text-ink">{m.display_name}</span>
                    {m.is_admin && <Chip tone="info">{S.members.systemAdmin}</Chip>}
                    {/* 停用是这一行的状态，不是另一张表 */}
                    {m.deactivated && (
                      <Chip tone="danger">{S.members.filterDeactivated}</Chip>
                    )}
                  </div>
                  <div className="truncate text-small text-ink-2">{m.email}</div>
                </div>
                {!m.deactivated && (
                  <Dropdown
                    size="sm"
                    className="w-24"
                    value={m.role}
                    onChange={(role) => setRole.mutate({ userId: m.user_id, role })}
                    options={ROLE_OPTIONS}
                  />
                )}
                <div className="flex shrink-0 items-center gap-3 whitespace-nowrap">
                  {m.deactivated ? (
                    <Button variant="secondary" size="sm"
                      disabled={revive.isPending}
                      onClick={() => revive.mutate(m.user_id)}
                    >
                      {S.members.reactivate}
                    </Button>
                  ) : (
                    <>
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
                    </>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
        <Pager
          total={memberList.length}
          pageSize={MEMBER_PAGE}
          page={safeMemberPage}
          onPage={setMemberPage}
        />
      </SettingsCard>

      {me.data?.is_admin && (
        <CreateUserDialog
          open={creating}
          onOpenChange={setCreating}
          onCreated={() => {
            setCreating(false);
            refresh();
          }}
        />
      )}
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

/** 管理员代开账号（注册关闭后的唯一入口）。开账号是个动作，住在弹窗里 */
function CreateUserDialog({
  open,
  onOpenChange,
  onCreated,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onCreated: () => void;
}) {
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
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={S.settings.newUser}
      closeLabel={S.ui.close}
      footer={
        <>
          <Button variant="secondary" size="sm" onClick={() => onOpenChange(false)}>
            {S.members.cancel}
          </Button>
          <Button variant="primary" size="sm"
            disabled={!valid || create.isPending}
            onClick={() => create.mutate()}
          >
            {S.settings.createUserBtn}
          </Button>
        </>
      }
    >
      {/* 这是**给别人开账号**，不是登录：浏览器看见「邮箱 + 密码」就把当前
          登录的人填进来（管理员打开它，看到的是自己的名字和一串圆点）。
          `new-password` 让 Chrome 认出这是设新密码而不是回填旧凭据，
          上面两格一并关掉自动填充 */}
      <div className="grid grid-cols-2 gap-3">
        <Field label={S.login.email} className="mb-0">
          <Input className="w-full"
            autoFocus
            autoComplete="off"
            value={email}
            onChange={(e) => setEmail(e.target.value)}
          />
        </Field>
        <Field label={S.login.displayName} className="mb-0">
          <Input className="w-full"
            autoComplete="off"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
        </Field>
        <Field label={S.settings.initialPassword} className="mb-0">
          <Input className="w-full"
            type="password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </Field>
        <Field label={S.members.roleLabel} className="mb-0">
          <Dropdown
            className="w-full"
            value={role}
            onChange={setRole}
            options={[
              { value: "admin", label: S.members.roles.admin },
              { value: "editor", label: S.members.roles.editor },
              { value: "viewer", label: S.members.roles.viewer },
            ]}
          />
        </Field>
      </div>
      {error && <p className="mt-3 text-small text-danger">{error}</p>}
    </Dialog>
  );
}

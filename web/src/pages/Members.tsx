import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Plus, Search } from "lucide-react";
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
  Table,
  TBody,
  Td,
  Th,
  THead,
  Tr,
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
  const [adding, setAdding] = useState(false);
  // 角色平时是一行字，点了才变成下拉——同时只有一行在编辑
  const [editingRole, setEditingRole] = useState<string | null>(null);
  // 按角色筛
  const [role, setRole] = useState("all");
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

  const setRole_ = useMutation({
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
    .filter((p) => role === "all" || p.role === role)
    .filter(
      (p) =>
        !q ||
        p.display_name.toLowerCase().includes(q) ||
        p.email.toLowerCase().includes(q),
    )
    // 停用的沉底：他们仍然列着，但不该排在还在用的人前面
    .sort((a, b) => Number(a.deactivated) - Number(b.deactivated));
  const { rows: pagedMembers, safe: safeMemberPage } = pageSlice(memberList, memberPage, MEMBER_PAGE);

  const reset = (fn: () => void) => {
    fn();
    setMemberPage(0);
  };

  return (
    <div className="space-y-4">
      {/* 筛这份名单的东西在表格外面（DESIGN.md 6）：三个筛子同一副身材——
          带放大镜的中号输入框 + 两个下拉，与图谱、文库那几页的筛选条一致 */}
      <div className="flex flex-wrap items-center gap-2">
        <Input
          icon={<Search size={13} />}
          className="w-64"
          placeholder={S.settings.searchUsers}
          value={filter}
          onChange={(e) => reset(() => setFilter(e.target.value))}
        />
        <Dropdown
          className="w-32"
          value={role}
          onChange={(v) => reset(() => setRole(v))}
          options={[
            { value: "all", label: S.members.filterAllRoles },
            ...ROLE_OPTIONS,
          ]}
        />
        {me.data?.is_admin && (
          <Dropdown
            className="w-36"
            value={status}
            onChange={(v) => reset(() => setStatus(v as typeof status))}
            options={[
              { value: "active", label: S.members.filterActive },
              { value: "deactivated", label: S.members.filterDeactivated },
              { value: "all", label: S.members.filterAll },
            ]}
          />
        )}
        <div className="ml-auto flex items-center gap-2">
          <Button variant="secondary" size="sm" onClick={() => setAdding(true)}>
            {S.members.addExisting}
          </Button>
          {me.data?.is_admin && (
            <Button variant="primary" size="sm" onClick={() => setCreating(true)}>
              <Plus size={12} />
              {S.settings.newUser}
            </Button>
          )}
        </div>
      </div>

      {error && <p className="text-body text-danger">{error}</p>}

      {/* 一张表：每一列宽度定死，角色、动作都在自己那一列里，不再随名字长短漂移 */}
      <div className="glass rounded-panel overflow-hidden">
        <Table>
          <THead>
            <Tr>
              <Th>{S.members.userLabel}</Th>
              <Th>{S.members.roleLabel}</Th>
              <Th>{S.members.statusLabel}</Th>
              <Th />
            </Tr>
          </THead>
          <TBody>
            {pagedMembers.map((m) => (
              <Tr key={m.user_id} className={m.deactivated ? "opacity-55" : undefined}>
                <Td>
                  <div className="flex items-center gap-2">
                    <span className="truncate text-body text-ink">{m.display_name}</span>
                    {m.is_admin && <Chip tone="info">{S.members.systemAdmin}</Chip>}
                  </div>
                  <div className="truncate text-small text-ink-2">{m.email}</div>
                </Td>
                <Td>
                  {/* 静态文字，点一下才变成下拉：一列下拉框会把一张只读的名单
                      看成一张待填的表，而改角色是偶尔为之 */}
                  {m.deactivated ? (
                    <span className="text-small text-ink-2">—</span>
                  ) : editingRole === m.user_id ? (
                    <Dropdown
                      size="sm"
                      className="w-24"
                      value={m.role}
                      onChange={(r) => {
                        setEditingRole(null);
                        if (r !== m.role) setRole_.mutate({ userId: m.user_id, role: r });
                      }}
                      options={ROLE_OPTIONS}
                    />
                  ) : (
                    <LinkButton onClick={() => setEditingRole(m.user_id)}>
                      {S.members.roles[m.role as keyof typeof S.members.roles] ?? m.role}
                    </LinkButton>
                  )}
                </Td>
                <Td>
                  {m.deactivated ? (
                    <Chip tone="danger">{S.members.filterDeactivated}</Chip>
                  ) : (
                    <span className="text-small text-ink-2">{S.members.filterActive}</span>
                  )}
                </Td>
                <Td className="text-right whitespace-nowrap">
                  {m.deactivated ? (
                    <Button variant="secondary" size="sm"
                      disabled={revive.isPending}
                      onClick={() => revive.mutate(m.user_id)}
                    >
                      {S.members.reactivate}
                    </Button>
                  ) : (
                    <span className="flex items-center justify-end gap-3">
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
                    </span>
                  )}
                </Td>
              </Tr>
            ))}
          </TBody>
        </Table>
        {memberList.length === 0 && (
          <p className="px-4 py-6 text-body text-ink-2">{S.ui.noMatches}</p>
        )}
      </div>
      <Pager
        total={memberList.length}
        pageSize={MEMBER_PAGE}
        page={safeMemberPage}
        onPage={setMemberPage}
      />

      {/* 把一个**已有账号**加进这个工作区。与「开账号」是两件事：那个凭空造
          一个人，这个只是给已经存在的人一个角色 */}
      <Dialog
        open={adding}
        onOpenChange={setAdding}
        title={S.members.addExisting}
        closeLabel={S.ui.close}
        footer={
          <>
            <Button variant="secondary" size="sm" onClick={() => setAdding(false)}>
              {S.members.cancel}
            </Button>
            <Button variant="primary" size="sm"
              disabled={!addUserId || setRole_.isPending}
              onClick={() => {
                setRole_.mutate({ userId: addUserId, role: addRole });
                setAdding(false);
              }}
            >
              {S.members.add}
            </Button>
          </>
        }
      >
        <div className="grid grid-cols-2 gap-3">
          <Field label={S.members.userLabel} className="mb-0">
            {/* 空列表由 SearchSelect 自己说（它有 noMatches 空态） */}
            <SearchSelect
              className="w-full"
              value={addUserId}
              onChange={setAddUserId}
              placeholder={S.members.pickUser}
              options={addable.map((u) => ({
                value: u.id,
                label: u.display_name,
                hint: u.email,
              }))}
            />
          </Field>
          <Field label={S.members.roleLabel} className="mb-0">
            <Dropdown
              className="w-full"
              value={addRole}
              onChange={setAddRole}
              options={ROLE_OPTIONS}
            />
          </Field>
        </div>
      </Dialog>

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
            autoComplete="new-password"
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

/* 顶栏的知识库切换器：与用户菜单、告警面板同一套——胶囊原地长成面板（FLIP），
   面板的第一行就是胶囊本身，再点一下缩回去；下面列全部的库，当前那个带勾。
   从前它是一个下拉（Dropdown），弹出来的是另一张皮、另一种关法，挨着旁边
   两个面板一看就是外人。

   面板分三段：查找、库、新建。库上了十几个之后，切换库这件事是**先打字再挑**，
   不是滚一列名字；新建钉在最下面而不是混在列里——它不是一个库，是这张面板
   的出口。 */
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { Check, ChevronDown, Layers, Plus } from "lucide-react";
import { api, type Kb } from "../api";
import { S } from "../i18n";
import { Button, cn, Input, Row } from "../ui";
import { usePopoverFlip } from "../ui/popoverFlip";

export function KbSwitcher({
  kb,
  kbs,
  onChange,
}: {
  kb: Kb | null | undefined;
  kbs: Kb[];
  onChange: (id: string) => void;
}) {
  const { open, setOpen, close, rootRef, anchorRef, panelRef } =
    usePopoverFlip<HTMLButtonElement, HTMLDivElement>("top left");
  const navigate = useNavigate();
  const name = kb?.name ?? "…";
  const [q, setQ] = useState("");
  // 建库要系统管理员或工作区 Admin+（见 api/kbs.rs create）：没这个权限的人
  // 看不到入口，省得点进去吃一个 403
  const me = useQuery({ queryKey: ["me"], queryFn: api.me });
  const wsRole = useQuery({
    queryKey: ["workspaceRole", kb?.workspace_id],
    queryFn: () => api.workspaceRole(kb!.workspace_id),
    enabled: !!kb?.workspace_id,
  });

  // 关掉就把查找词丢掉：下次打开是从头挑，不是接着上次的筛选结果
  const dismiss = () => {
    setQ("");
    close();
  };
  const needle = q.trim().toLowerCase();
  const shown = needle
    ? kbs.filter((k) => k.name.toLowerCase().includes(needle))
    : kbs;

  return (
    <div ref={rootRef} className="relative">
      {/* 胶囊：无底无框，图标与导航标签同一档灰、同一个大小和线宽（15 / 1.8），
          名字与标签同一个字号和字重（正文、中等），颜色是正文色——它是当前库的
          名字，不是次要信息。打开时隐形，位置留给面板的第一行 */}
      <Button
        ref={anchorRef}
        variant="ghost"
        size="sm"
        aria-expanded={open}
        title={S.nav.kbLabel}
        // 与右边的用户菜单胶囊同高（36）：py-2 加一行正文。左右只留 px-2——胶囊没有边框，
        // 内距再宽就只是把图标从字标旁边推开；面板行仍是 px-4，靠面板整体左移 8 让
        // 第一行的图标落在胶囊图标的位置上
        className={cn("h-8 max-w-64 border-0 px-2", open && "invisible")}
        icon={<Layers size={15} strokeWidth={1.8} className="text-ink-2" />}
        onClick={() => (open ? dismiss() : setOpen(true))}
      >
        <span className="truncate text-body font-medium text-ink">{name}</span>
        <ChevronDown size={12} className="shrink-0 text-ink-2" />
      </Button>

      {open && (
        <div
          ref={panelRef}
          // -left-2：胶囊只有 8 的内距，面板行有 16——面板左移 8，行里的图标才与胶囊的图标同一个 x
          className="u-menu-glass absolute -left-2 top-0 z-50 w-max min-w-64 max-w-80 overflow-hidden rounded-overlay shadow-2xl"
        >
          {/* 第一行是胶囊自己：同一个图标、同一个名字，箭头翻上去；再点一下缩回 */}
          <div
            onClick={dismiss}
            className="u-row-shell flex cursor-pointer items-center gap-3 border-b border-line px-4 py-3"
          >
            <Layers size={15} strokeWidth={1.8} className="shrink-0 text-ink-2" />
            <span className="min-w-0 flex-1 truncate text-body font-medium text-ink">
              {name}
            </span>
            <ChevronDown size={12} className="shrink-0 rotate-180 text-ink-2" />
          </div>
          {/* 查找：没有自己的框（bare）——它是面板的一段，不是面板里摆的一个控件。
              打开就聚焦，一开口就能打字；Esc 由 popoverFlip 统一关面板 */}
          <div className="border-b border-line px-4 py-3">
            <Input
              bare
              autoFocus
              className="w-full text-body"
              placeholder={S.nav.findKb}
              value={q}
              onChange={(e) => setQ(e.target.value)}
            />
          </div>
          <div className="u-scroll max-h-80 overflow-y-auto">
            {shown.map((k) => (
              <Row
                key={k.id}
                density="menu"
                className="gap-3 px-4 py-2 text-body"
                trailing={
                  k.id === kb?.id ? <Check size={13} className="text-ink-2" /> : undefined
                }
                onClick={() => {
                  dismiss();
                  if (k.id !== kb?.id) onChange(k.id);
                }}
              >
                <span className="truncate">{k.name}</span>
              </Row>
            ))}
            {shown.length === 0 && (
              <p className="px-4 py-3 text-body text-ink-2">{S.nav.noKbMatch}</p>
            )}
          </div>
          {/* 新建钉在最下面，不混在列里：它不是一个库。去的是「我的知识库」，
              带上 create——落地就是表单，不用到了那一页再找一次按钮 */}
          {(me.data?.is_admin ||
            wsRole.data?.role === "admin" ||
            wsRole.data?.role === "owner") && (
            <Row
              density="menu"
              className="gap-3 border-t border-line px-4 py-3 text-body"
              icon={<Plus size={14} />}
              onClick={() => {
                dismiss();
                navigate({ to: "/account/kbs", search: { create: true } });
              }}
            >
              {S.settings.kbs.newKb}
            </Row>
          )}
        </div>
      )}
    </div>
  );
}

/* 顶栏的知识库切换器：与用户菜单、告警面板同一套——胶囊原地长成面板（FLIP），
   面板的第一行就是胶囊本身，再点一下缩回去；下面列全部的库，当前那个带勾。
   从前它是一个下拉（Dropdown），弹出来的是另一张皮、另一种关法，挨着旁边
   两个面板一看就是外人。 */
import { Check, ChevronDown, Layers } from "lucide-react";
import type { Kb } from "../api";
import { S } from "../i18n";
import { Button, cn, Row } from "../ui";
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
  const name = kb?.name ?? "…";

  return (
    <div ref={rootRef} className="relative">
      {/* 胶囊：无底无框，图标与导航标签同一档灰，名字是正文色——它是当前库的
          名字，不是次要信息。打开时隐形，位置留给面板的第一行 */}
      <Button
        ref={anchorRef}
        variant="ghost"
        size="sm"
        aria-expanded={open}
        title={S.nav.kbLabel}
        // px-4 与面板行同一个内距：面板长出来时，第一行的图标就落在胶囊图标的位置上
        className={cn("h-auto max-w-64 border-0 px-4 py-1", open && "invisible")}
        icon={<Layers size={13} className="text-ink-2" />}
        onClick={() => (open ? close() : setOpen(true))}
      >
        <span className="truncate text-body text-ink">{name}</span>
        <ChevronDown size={12} className="shrink-0 text-ink-3" />
      </Button>

      {open && (
        <div
          ref={panelRef}
          className="u-menu-glass absolute left-0 top-0 z-50 w-max min-w-64 max-w-80 overflow-hidden rounded-xl shadow-2xl"
        >
          {/* 第一行是胶囊自己：同一个图标、同一个名字，箭头翻上去；再点一下缩回 */}
          <div
            onClick={close}
            className="u-row-shell flex cursor-pointer items-center gap-3 border-b border-line px-4 py-3"
          >
            <Layers size={13} className="shrink-0 text-ink-2" />
            <span className="min-w-0 flex-1 truncate text-body font-medium text-ink">
              {name}
            </span>
            <ChevronDown size={12} className="shrink-0 rotate-180 text-ink-3" />
          </div>
          <div className="u-scroll max-h-80 overflow-y-auto">
            {kbs.map((k) => (
              <Row
                key={k.id}
                density="menu"
                className="gap-3 px-4 py-3 text-body"
                trailing={
                  k.id === kb?.id ? <Check size={13} className="text-ink-2" /> : undefined
                }
                onClick={() => {
                  close();
                  if (k.id !== kb?.id) onChange(k.id);
                }}
              >
                <span className="truncate">{k.name}</span>
              </Row>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

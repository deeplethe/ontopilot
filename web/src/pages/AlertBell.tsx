// 顶栏告警（0005）：铃铛 + 未读角标 + 弹出面板。
//
// **弹窗不是页面**：告警是"顺手瞄一眼"的东西，不是一个要专门去逛的地方。
// 做成页面会逼人离开手头的事，而离开的代价就是没人去看。
//
// 一条告警 = 一次故障，写完不再变，没有"已解决"。
// 「已读」逐人——一个人读过不代表别人也该从未读里消失。
import { type Ref, useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Bell, ChevronDown } from "lucide-react";

import { api, type AlertGroup } from "../api";
import { S } from "../i18n";
import { toast } from "../toast";
import {
  Button,
  Chip,
  type ChipTone,
  cn,
  IconButton,
  Input,
  LinkButton,
  Pager,
} from "../ui";
import { usePopoverFlip } from "../ui/popoverFlip";

const PAGE = 8;

/** 明细里给人看的那一行：对象名 — 报错原文 */
function line(d: AlertGroup["lines"][number]): string | null {
  const parts = [d.name ?? d.job, d.error].filter(Boolean);
  return parts.length ? parts.join(" — ") : null;
}

/** 哪些告警带「再跑一遍」：故障修好之后（充值、改端点）任务不会自己回来的那几种 */
const REQUEUE_KINDS = new Set(["llm.out_of_credit", "llm.unreachable"]);

/* 轻重是服务端定的（`utopia-store/src/alerts.rs` 的 severity），一组取组里最重的
   那一档。**告警不全是故障**：欠费、源同步失败是 error，限流、schema 没摄进来、
   治理跳闸是 warning，而「映射探索一条口径都没提出来」是 info——它是"你等的那件
   事没有结果"，不是坏了。从前这一栏一概不看 severity，七种告警长得一模一样。 */
const SEVERITY_TONE: Record<string, ChipTone> = {
  error: "danger",
  warning: "warn",
  info: "info",
};
/** 未读的那个点：**有没有点说的是读没读过，什么颜色说的是多重** */
const SEVERITY_DOT: Record<string, string> = {
  error: "bg-danger",
  warning: "bg-warn",
  info: "bg-accent",
};

function AlertRow({
  g,
  onRead,
  onRequeue,
  requeuing,
}: {
  g: AlertGroup;
  onRead: (g: AlertGroup) => void;
  onRequeue: (g: AlertGroup) => void;
  requeuing: boolean;
}) {
  // 没见过的 kind 也得显示得出来：新告警源上线时前端可能还没跟上，
  // 而"有条告警但我不认识它"远好过"什么都不显示"
  const worded = S.alerts.kinds[g.kind];
  const lines = g.lines.map(line).filter((l): l is string => !!l);
  /* 同一句报错重复五遍，读者第二遍就不再读了，可它照样把面板撑高一截：
     一模一样的行并成一条，右边记个次数。**并的是显示，不是计数**——
     下面「还有 N 条」用的仍是原始条数 */
  const tally = new Map<string, number>();
  for (const l of lines) tally.set(l, (tally.get(l) ?? 0) + 1);
  // count 数的是整组，lines 只带回前几条——差额是"还有 N 条"
  const rest = g.count - lines.length;
  return (
    // div 而不是 button：行里还有一个动作按钮，按钮套按钮是无效 HTML
    <div
      role="button"
      tabIndex={0}
      // **点击才算读过**，不是划过。鼠标经过一列告警不代表看过它们，
      // 而已读一旦落下就再也不会自己回来。点一下把这一组整个标掉
      onClick={() => {
        if (g.unread > 0) onRead(g);
      }}
      onKeyDown={(e) => {
        if (e.key === "Enter" && g.unread > 0) onRead(g);
      }}
      className="u-row-shell relative w-full cursor-pointer border-b border-line px-4 py-3 text-left last:border-b-0"
    >
      {/* 未读就是一个红点。整行描边或底色会让面板在告警多时变成一片红，
          而红点只占它该占的那一点地方，读过就没了。
          **点在内距里，不占文字那一列**：排在文字左边的话，每条告警的正文
          就比面板标题和上面那道查找往右缩 18px，一张面板里三种左缘 */}
      <span
        className={cn(
          "absolute left-1.5 top-5 h-1.5 w-1.5 rounded-full",
          g.unread > 0 ? (SEVERITY_DOT[g.severity] ?? "bg-danger") : "bg-transparent",
        )}
      />
      <div className="min-w-0">
        {/* 标题行只放标题和次数。标题一直是正文色：读过只是不再加粗——
            淡下去那一档现在与提示、明细同色，一条读过的告警整条糊成一片 */}
        <div className="flex items-center gap-2">
          <span
            className={cn("min-w-0 flex-1 text-body text-ink", g.unread > 0 && "font-medium")}
          >
            {worded?.title ?? S.alerts.unknownKind(g.kind)}
          </span>
          {/* 次数是"这件事发生了几回"，不是一句补充说明：中性灰把它读成一个
              标签，而它说的是这条告警的分量 */}
          {g.count > 1 && (
            <Chip tone={SEVERITY_TONE[g.severity] ?? "neutral"}>{g.count}</Chip>
          )}
        </div>
        {/* 哪个库、什么时候：落款单独一行。跟在标题后面的话，标题一长就把
            它们挤到下一行，每条告警的头两行长得都不一样 */}
        <div className="mt-1 flex items-center gap-2">
          <Chip tone={g.kb_name ? "neutral" : "violet"}>
            {g.kb_name ?? S.alerts.system}
          </Chip>
          {/* 取组里最新的那一次 */}
          <span className="u-num ml-auto shrink-0 text-fine text-ink-2">
            {new Date(g.latest_at).toLocaleString()}
          </span>
        </div>
        {worded && (
          <p className="mt-1 text-small text-ink-2">{worded.hint}</p>
        )}
        {lines.length > 0 && (
          <ul className="mt-1 space-y-1">
            {[...tally].map(([l, n]) => (
              <li key={l} className="text-fine text-ink-2 break-words">
                {l}
                {n > 1 && <span className="u-num text-ink-2"> ×{n}</span>}
              </li>
            ))}
            {rest > 0 && (
              <li className="text-fine text-ink-2">
                {S.alerts.andMore(rest)}
              </li>
            )}
          </ul>
        )}
        {/* 修好之后接着跑：把这次故障窗口里失败的任务放回队列（#216）。
            余额耗尽是唯一一种「人做完一件具体的事就想让活继续」的失败，
            动作长在告警上，闭环就在这里，不必另建一个队列页 */}
        {REQUEUE_KINDS.has(g.kind) && (
          <Button variant="secondary" size="sm" className="mt-2"
            type="button"
            disabled={requeuing}
            onClick={(e) => {
              e.stopPropagation();
              onRequeue(g);
            }}
          >
            {S.alerts.runAgain}
          </Button>
        )}
      </div>
    </div>
  );
}

function Panel({ panelRef, onClose }: { panelRef: Ref<HTMLDivElement>; onClose: () => void }) {
  const [q, setQ] = useState("");
  const [page, setPage] = useState(0);
  const qc = useQueryClient();

  // 搜索后回第一页：停在第 4 页看一个只有 2 页的结果，
  // 面板会显示空白，而人会读成"没有告警"
  useEffect(() => {
    setPage(0);
  }, [q]);

  const list = useQuery({
    queryKey: ["alerts", "list", q, page],
    queryFn: () => api.alerts({ q, limit: PAGE, offset: page * PAGE }),
    // 翻页时留着上一页，免得面板高度塌一下再弹回来
    placeholderData: (prev) => prev,
  });

  const read = useMutation({
    mutationFn: (g: AlertGroup) =>
      api.alertReadGroup({
        kb_id: g.kb_id,
        kind: g.kind,
        from: g.earliest_at,
        to: g.latest_at,
      }),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["alerts"] }),
  });
  const readAll = useMutation({
    mutationFn: () => api.alertsReadAll(),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["alerts"] }),
  });
  // 时间窗从这组最早那次故障起——之前失败的不是这次的事
  const requeue = useMutation({
    mutationFn: (g: AlertGroup) =>
      api.requeueJobs(g.kb_id, { failed_since: g.earliest_at }),
    onSuccess: (r) => {
      toast.success(S.alerts.requeued(r.requeued));
      qc.invalidateQueries({ queryKey: ["jobs"] });
    },
    onError: (e) => toast.error(String(e)),
  });

  const groups = list.data?.items ?? [];
  const total = list.data?.total ?? 0;

  return (
    // top-0 而不是 top-9：面板要从铃铛**原位**长出来，右上角对齐
    <div
      ref={panelRef}
      className="u-menu-glass absolute right-0 top-0 w-[420px] rounded-overlay shadow-2xl z-50 overflow-hidden"
    >
      {/* 第一行就是关掉这张面板——同库切换器：面板从铃铛原位长出来，
          右端那个朝上的三角正落在铃铛上，"再点一下缩回去"。
          从前这里是一个浮在角上的关闭叉，它得跟发丝边框对齐，永远差半像素 */}
      <div
        onClick={onClose}
        className="u-row-shell flex cursor-pointer items-center gap-3 border-b border-line px-4 py-3"
      >
        {/* 与库切换器的第一行同构：图标 + 名字 + 朝上的三角。图标是铃铛本身
            ——这一行就是那个铃铛长出来的样子 */}
        <Bell size={15} strokeWidth={1.8} className="shrink-0 text-ink-2" />
        <span className="min-w-0 flex-1 truncate text-body font-medium text-ink">
          {S.alerts.title}
        </span>
        <ChevronDown size={12} className="shrink-0 rotate-180 text-ink-2" />
      </div>

      {/* 查找与库切换器同一副样子：没有自己的框（bare），它是面板的一段，
          不是面板里摆的一个控件；Esc 由 popoverFlip 统一关面板 */}
      <div className="border-b border-line px-4 py-3">
        <Input
          bare
          className="w-full text-body"
          placeholder={S.alerts.searchPlaceholder}
          value={q}
          onChange={(e) => setQ(e.target.value)}
        />
      </div>

      <div className="max-h-[420px] overflow-y-auto">
        {groups.length === 0 ? (
          <div className="px-4 py-8 text-center">
            <p className="text-body text-ink-2">
              {q ? S.alerts.noMatch : S.alerts.empty}
            </p>
            {!q && (
              <p className="mt-1 text-small text-ink-2">
                {S.alerts.emptyHint}
              </p>
            )}
          </div>
        ) : (
          groups.map((g) => (
            <AlertRow
              key={`${g.kb_id ?? "system"}|${g.kind}|${g.latest_at}`}
              g={g}
              onRead={(x) => read.mutate(x)}
              onRequeue={(x) => requeue.mutate(x)}
              requeuing={requeue.isPending}
            />
          ))
        )}
      </div>

      {/* 底栏：整张列表级的动作跟翻页放一起，离光标最远 */}
      {groups.length > 0 && (
        <div className="flex items-center gap-3 px-4 py-2 border-t border-line">
          {groups.some((g) => g.unread > 0) && (
            <LinkButton onClick={() => readAll.mutate()}>
              {S.alerts.markAllRead}
            </LinkButton>
          )}
          {/* 只有一页也显示：这条底栏是固定的，分页器一藏它就成了一道空边 */}
          <Pager
            always
            className="ml-auto"
            total={total}
            pageSize={PAGE}
            page={page}
            onPage={setPage}
          />
        </div>
      )}
    </div>
  );
}

export function AlertBell() {
  // 跟用户菜单同一份原地变形：两个面板紧挨着，动画差一点点来回点两下就看得出来
  const { open, setOpen, close, rootRef, anchorRef, panelRef } =
    usePopoverFlip<HTMLButtonElement, HTMLDivElement>();
  const unread = useQuery({
    queryKey: ["alerts", "unread"],
    queryFn: () => api.alertsUnread(),
    // 推送是主路，这个只是断流时的兜底
    refetchInterval: 120_000,
  });
  const n = unread.data?.unread ?? 0;

  return (
    <div ref={rootRef} className="relative">
      <IconButton
        size="md"
        ref={anchorRef}
        label={S.alerts.badgeLabel}
        aria-expanded={open}
        className={cn("relative", open && "bg-surface-2 text-ink")}
        onClick={() => (open ? close() : setOpen(true))}
      >
        <Bell size={15} />
        {/* 角标也是个点，不是数字。"有事没看"是二元的，具体几条打开就知道；
            数字还会随重试一路往上跳，跳到三位数就把铃铛撑变形了 */}
        {n > 0 && (
          <span className="absolute top-1 right-1 h-1.5 w-1.5 rounded-full bg-danger" />
        )}
      </IconButton>
      {/* 关掉的入口在面板第一行（那儿正好压着铃铛），不再是浮在角上的一个叉 */}
      {open && <Panel panelRef={panelRef} onClose={close} />}
    </div>
  );
}

import {
  Button,
  Chip,
  Row,
} from "../ui";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
/* 用户菜单：顶栏右侧的头像胶囊 + 弹出面板（个人信息 / 系统管理 / 登出）。
   Shell（KB 工作区）与 AccountShell（账户层）共用。 */
import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import {
  Check,
  ChevronDown,
  Languages,
  Layers,
  LogOut,
  ShieldCheck,
  UserRound,
  SunMoon,
} from "lucide-react";
import { api, type User } from "../api";
import { LANGS, LANG_NAMES, S, lang, setLang } from "../i18n";

/** 首字母头像：中性灰底（chrome 零色偏），拉丁取词首两枚，CJK 取前两字。 */
export function Avatar({ name, size = 24 }: { name: string; size?: number }) {
  const trimmed = name.trim();
  const words = trimmed.split(/\s+/).filter(Boolean);
  const initials =
    words.length >= 2
      ? (words[0][0] + words[1][0]).toUpperCase()
      : [...trimmed].slice(0, 2).join("").toUpperCase();
  return (
    <span
      className="inline-grid place-items-center rounded-full bg-surface-3 border border-line text-ink select-none shrink-0"
      style={{ width: size, height: size, fontSize: Math.round(size * 0.38) }}
    >
      {initials}
    </span>
  );
}

import { getTheme, setTheme, type Theme } from "../theme";

// 三档的顺序就是菜单里的顺序：本色在前，跟系统在最后
const THEMES: Theme[] = ["dark", "light", "system"];

export function UserMenu({ user }: { user: User }) {
  const [theme, setThemeState] = useState<Theme>(() => getTheme());
  const [open, setOpen] = useState(false);
  const navigate = useNavigate();
  const queryClient = useQueryClient();

  const go = (to: string) => {
    setOpen(false);
    navigate({ to });
  };

  const logout = async () => {
    await api.logout();
    queryClient.clear();
    navigate({ to: "/login" });
  };

  // 行通到面板边缘（与 Dropdown 同语汇）：容器不留内衬，高度由行自身撑

  return (
    /* 顶栏的用户面板。**弹层是 shadcn 的 Popover**——从前是「胶囊原地长成面板」
       的手写过渡（usePopoverFlip），一处一套行为；现在外点关闭、Esc、焦点回到
       触发器、定位翻转都归 Radix，全站三个顶栏面板同一副。 */
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <Button variant="ghost" className="u-avatar-btn">
        {/* 24：胶囊是 32 高（顶栏所有控件同一个高度），头像两边各留 4 */}
        <Avatar name={user.display_name} size={24} />
        <span className="text-body text-ink-2">{user.display_name}</span>
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-64 overflow-hidden p-0">
          {/* 身份头 */}
          <div className="flex items-center gap-3 border-b border-line px-4 py-3">
            <Avatar name={user.display_name} size={32} />
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <span className="truncate text-body font-medium text-ink">
                  {user.display_name}
                </span>
                {user.is_admin && (
                  <Chip tone="neutral" className="text-fine">
                    {S.account.adminChip}
                  </Chip>
                )}
              </div>
              <div className="truncate text-fine text-ink-2">
                {user.email}
              </div>
            </div>
            {/* 朝上的三角：说明这一行是收回去的地方，同库切换器与告警面板。
                三个面板都从各自的胶囊原地长出来，也都从第一行原地缩回去 */}
            <ChevronDown size={12} className="shrink-0 rotate-180 text-ink-2" />
          </div>

          <div>
            {/* 图标走 Row 的 icon 槽——塞在 children 里的 svg 是块级的，会把文字
                挤到第二行 */}
            <Row
              density="menu"
              className="gap-3 px-4 py-2 text-body"
              icon={<UserRound size={13} />}
              onClick={() => go("/account")}
            >
              {S.account.profile}
            </Row>
            {/* 人人可看：全部可见库 + 我在每个库的身份 */}
            <Row
              density="menu"
              className="gap-3 px-4 py-2 text-body"
              icon={<Layers size={13} />}
              onClick={() => go("/account/kbs")}
            >
              {S.account.kbsNav}
            </Row>
            {user.is_admin && (
              <Row
                density="menu"
                className="gap-3 px-4 py-2 text-body"
                icon={<ShieldCheck size={13} />}
                onClick={() => go("/admin")}
              >
                {S.account.administration}
              </Row>
            )}
          </div>

          {/* 界面语言：看的人自己定，不经过后端（docs/decisions/0004）。
              每个选项用**它自己的语言**写——看不懂英文的人才认得出"中文" */}
          <div className="border-t border-line">
            <div className="flex items-center gap-3 px-4 pt-3 pb-1 text-fine text-ink-2">
              <Languages size={13} className="text-ink-2" />
              {S.account.language}
            </div>
            {LANGS.map((l) => (
              <Row
                density="menu"
                className="gap-3 px-4 py-2 text-body"
                key={l}
                icon={
                  <span className="block w-[13px]">
                    {l === lang && <Check size={13} className="text-ink-2" />}
                  </span>
                }
                onClick={() => setLang(l)}
              >
                {LANG_NAMES[l]}
              </Row>
            ))}
          </div>

          {/* 主题（0038）：暗是本色，浅色给白天对着它八小时的人；跟系统是第三档。
              与语言同一个道理——看的人自己定，不经过后端 */}
          <div className="border-t border-line">
            <div className="flex items-center gap-3 px-4 pt-3 pb-1 text-fine text-ink-2">
              <SunMoon size={13} className="text-ink-2" />
              {S.account.theme}
            </div>
            {THEMES.map((t) => (
              <Row
                density="menu"
                className="gap-3 px-4 py-2 text-body"
                key={t}
                icon={
                  <span className="block w-[13px]">
                    {t === theme && <Check size={13} className="text-ink-2" />}
                  </span>
                }
                onClick={() => {
                  setTheme(t);
                  setThemeState(t);
                }}
              >
                {S.account.themeNames[t]}
              </Row>
            ))}
          </div>

          <div className="border-t border-line">
            <Row
              density="menu"
              danger
              className="gap-3 px-4 py-2 text-body"
              icon={<LogOut size={13} />}
              onClick={logout}
            >
              {S.nav.signOut}
            </Row>
          </div>
      </PopoverContent>
    </Popover>
  );
}

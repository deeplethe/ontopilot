import { Button, Chip } from "../ui";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
/* 用户菜单：顶栏右侧的头像胶囊 + 弹出面板（个人信息 / 系统管理 / 登出）。
   Shell（KB 工作区）与 AccountShell（账户层）共用。 */
import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import {
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
    /* 顶栏的用户菜单。**是菜单不是面板**，所以用 DropdownMenu 而不是 Popover：
       它要的是「一列动作」加上二级菜单。语言与主题收进二级——语言以后会有好几种，
       主题有三档，全都平铺在一级里，这张菜单会越长越长，而它们都属于「偏好」，
       不是与个人资料、管理并列的动作。 */
    <DropdownMenu open={open} onOpenChange={setOpen}>
      <DropdownMenuTrigger asChild>
        <Button variant="ghost" className="u-avatar-btn">
          {/* 24：胶囊是 32 高（顶栏所有控件同一个高度），头像两边各留 4 */}
          <Avatar name={user.display_name} size={24} />
          <span className="text-body text-ink-2">{user.display_name}</span>
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="w-64">
        {/* 身份头：不是一个可点的项，只说"你是谁" */}
        <div className="flex items-center gap-3 px-2 py-2">
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
            <div className="truncate text-fine text-ink-2">{user.email}</div>
          </div>
        </div>
        <DropdownMenuSeparator />

        <DropdownMenuItem onSelect={() => go("/account")}>
          <UserRound size={13} />
          {S.account.profile}
        </DropdownMenuItem>
        {/* 人人可看：全部可见库 + 我在每个库的身份 */}
        <DropdownMenuItem onSelect={() => go("/account/kbs")}>
          <Layers size={13} />
          {S.account.kbsNav}
        </DropdownMenuItem>
        {user.is_admin && (
          <DropdownMenuItem onSelect={() => go("/admin")}>
            <ShieldCheck size={13} />
            {S.account.administration}
          </DropdownMenuItem>
        )}

        <DropdownMenuSeparator />

        {/* 界面语言：看的人自己定，不经过后端（docs/decisions/0004）。
            每个选项用**它自己的语言**写——看不懂英文的人才认得出"中文" */}
        <DropdownMenuSub>
          <DropdownMenuSubTrigger>
            <Languages size={13} />
            {S.account.language}
            <span className="ml-auto pl-2 text-fine text-ink-2">
              {LANG_NAMES[lang]}
            </span>
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent>
            <DropdownMenuRadioGroup
              value={lang}
              onValueChange={(v) => setLang(v as (typeof LANGS)[number])}
            >
              {LANGS.map((l) => (
                <DropdownMenuRadioItem key={l} value={l}>
                  {LANG_NAMES[l]}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuSubContent>
        </DropdownMenuSub>

        {/* 主题（0038）：暗是本色，浅色给白天对着它八小时的人；跟系统是第三档 */}
        <DropdownMenuSub>
          <DropdownMenuSubTrigger>
            <SunMoon size={13} />
            {S.account.theme}
            <span className="ml-auto pl-2 text-fine text-ink-2">
              {S.account.themeNames[theme]}
            </span>
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent>
            <DropdownMenuRadioGroup
              value={theme}
              onValueChange={(v) => {
                setTheme(v as Theme);
                setThemeState(v as Theme);
              }}
            >
              {THEMES.map((t) => (
                <DropdownMenuRadioItem key={t} value={t}>
                  {S.account.themeNames[t]}
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuSubContent>
        </DropdownMenuSub>

        <DropdownMenuSeparator />

        <DropdownMenuItem variant="destructive" onSelect={logout}>
          <LogOut size={13} />
          {S.nav.signOut}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/* 顶栏右侧那一组：第一个链接（App 里是 Docs，Docs / 账户页里是回 App）+
   GitHub·版本胶囊 + 告警铃 + 用户菜单。三个顶栏共用这一份——从前各自拼一遍，
   内距和间距各差几像素，换页时右上角就跟着抖。 */
import { Link } from "@tanstack/react-router";
import type { ReactNode } from "react";
import type { User } from "../api";
import { S } from "../i18n";
import { GithubMark } from "../ui";
import { AlertBell } from "./AlertBell";
import { UserMenu } from "./UserMenu";

export function HeaderActions({
  link,
  version,
  user,
  signedOut,
}: {
  /** 最左那个链接：去哪、写什么 */
  link: { to: "/" | "/docs"; label: string };
  version?: string;
  user: User | null | undefined;
  /** 没登录时放在铃铛与用户菜单位置上的东西（Docs 页的「登录」）；缺省什么都不放 */
  signedOut?: ReactNode;
}) {
  return (
    <div className="ml-auto flex items-center gap-3">
      {/* 项目入口是一对（链接 + GitHub·版本），彼此贴得比组间近 */}
      <div className="flex items-center gap-2">
        <Link to={link.to} className="u-navlink">
          {link.label}
        </Link>
        <a
          href={S.login.githubUrl}
          target="_blank"
          rel="noreferrer"
          title="GitHub"
          className="u-pill"
        >
          <GithubMark size={13} />
          {version && <span className="u-num text-fine">v{version}</span>}
        </a>
      </div>
      {user ? (
        <>
          {/* 告警角标跟着人走，不跟着页面走：读文档、改账户的时候库照样在跑 */}
          <AlertBell />
          <UserMenu user={user} />
        </>
      ) : (
        signedOut
      )}
    </div>
  );
}

/* 表格：只管皮。表头 fine 号大写字距，行间细线，指针停上一行变 surface-2。
   数据列的对齐、宽度由页面决定；这里不做排序、不做分页（Pager 在 index.tsx）。 */
import type {
  HTMLAttributes,
  TableHTMLAttributes,
  TdHTMLAttributes,
  ThHTMLAttributes,
} from "react";
import { cn } from "./index";

export function Table({
  className,
  ...props
}: TableHTMLAttributes<HTMLTableElement>) {
  return (
    <div className="w-full overflow-x-auto">
      <table
        className={cn("w-full border-collapse text-body text-ink", className)}
        {...props}
      />
    </div>
  );
}

export function THead({
  className,
  ...props
}: HTMLAttributes<HTMLTableSectionElement>) {
  return <thead className={cn("text-left", className)} {...props} />;
}

export function TBody({
  className,
  ...props
}: HTMLAttributes<HTMLTableSectionElement>) {
  return <tbody className={className} {...props} />;
}

export function Tr({
  interactive,
  active,
  className,
  ...props
}: HTMLAttributes<HTMLTableRowElement> & {
  /** 可点的行：指针停上变面，光标变手 */
  interactive?: boolean;
  /** 选中的那一行。**用 `u-nav-active`，与左栏的行同一个记号**——它是
   *  hover 那一档面的两倍（0.09 对 0.045）再加白字，所以「我选中的」与
   *  「我正指着的」分得开；两者用同一副样子的话，一旦指到别的行，屏幕上
   *  就有两行长得一样 */
  active?: boolean;
}) {
  return (
    <tr
      aria-current={active ? "true" : undefined}
      className={cn(
        "border-b border-line",
        interactive && "cursor-pointer transition-colors duration-fast hover:bg-surface-2",
        active && "u-nav-active",
        className,
      )}
      {...props}
    />
  );
}

export function Th({
  className,
  ...props
}: ThHTMLAttributes<HTMLTableCellElement>) {
  return (
    <th
      className={cn(
        "border-b border-line-strong px-3 py-2 text-fine font-medium uppercase tracking-wider text-ink-2",
        className,
      )}
      {...props}
    />
  );
}

export function Td({
  className,
  ...props
}: TdHTMLAttributes<HTMLTableCellElement>) {
  return <td className={cn("px-3 py-2 align-top", className)} {...props} />;
}

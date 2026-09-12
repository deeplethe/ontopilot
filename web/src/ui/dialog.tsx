/* 对话框：Radix Dialog 打底（焦点困住、Esc、遮罩点击、aria 全在里面），
   皮是 styles.css 的 u-modal-*。近实底而不是玻璃：确认框要人读一句话然后做一个
   不可逆的决定，下层正文透上来是干扰（见 .u-modal-panel 上的说明）。 */
import { Dialog as RadixDialog } from "radix-ui";
import { X } from "lucide-react";
import { useState } from "react";
import type { ReactNode } from "react";
import { Button, IconButton, Input, cn } from "./index";

export function Dialog({
  open,
  onOpenChange,
  title,
  description,
  closeLabel,
  width = "md",
  children,
  footer,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: ReactNode;
  /** 标题下的一句说明；也是读屏器念的 description */
  description?: ReactNode;
  /** 右上角关闭按钮的无障碍名称——页面从 S 里取，组件不认识语言 */
  closeLabel: string;
  width?: "sm" | "md" | "lg";
  children?: ReactNode;
  /** 右下角的按钮区。危险确认用 DangerConfirm，不在这里拼 */
  footer?: ReactNode;
}) {
  const w = { sm: "w-96", md: "w-[32rem]", lg: "w-[44rem]" }[width];
  return (
    <RadixDialog.Root open={open} onOpenChange={onOpenChange}>
      <RadixDialog.Portal>
        <RadixDialog.Overlay className="u-modal-scrim fixed inset-0 z-50 grid place-items-center overflow-y-auto p-4">
          <RadixDialog.Content
            className={cn(
              "u-modal-panel u-modal-in max-w-full rounded-overlay p-6 u-lift-strong outline-none",
              w,
            )}
          >
            <div className="flex items-start gap-3">
              <div className="min-w-0 flex-1">
                <RadixDialog.Title className="u-title text-title">
                  {title}
                </RadixDialog.Title>
                {description ? (
                  <RadixDialog.Description className="mt-1 text-small text-ink-2">
                    {description}
                  </RadixDialog.Description>
                ) : (
                  // Radix 没有 description 会在控制台警告；空的也要有一个
                  <RadixDialog.Description className="sr-only">
                    {typeof title === "string" ? title : ""}
                  </RadixDialog.Description>
                )}
              </div>
              <RadixDialog.Close asChild>
                <IconButton label={closeLabel} size="sm" variant="ghost">
                  <X size={14} />
                </IconButton>
              </RadixDialog.Close>
            </div>
            {children && <div className="mt-4">{children}</div>}
            {footer && (
              <div className="mt-6 flex justify-end gap-2">{footer}</div>
            )}
          </RadixDialog.Content>
        </RadixDialog.Overlay>
      </RadixDialog.Portal>
    </RadixDialog.Root>
  );
}

/* ---------- DangerConfirm（危险操作确认：可要求输入指定文本解锁） ----------
   和从前一样的接口，皮换成 Dialog。**总是挂载着**（open 恒真）：调用方用
   条件渲染控制它出现，这跟 Dialog 的 open 属性一样有效，而且旧代码不用改。 */
export function DangerConfirm({
  title,
  hint,
  requireText,
  confirmLabel,
  cancelLabel,
  busy,
  onConfirm,
  onCancel,
  children,
}: {
  title: string;
  hint: string;
  /** 要求逐字输入的解锁文本（如资源名称）；缺省则直接可确认 */
  requireText?: string;
  confirmLabel: string;
  cancelLabel: string;
  busy?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
  /** 解锁框之下的补充控件（合并弹窗里的理由框）；缺省没有 */
  children?: ReactNode;
}) {
  const [text, setText] = useState("");
  const unlocked = !requireText || text === requireText;
  return (
    <Dialog
      open
      onOpenChange={(o) => {
        if (!o) onCancel();
      }}
      title={<span className="text-danger">{title}</span>}
      description={hint}
      closeLabel={cancelLabel}
      width="sm"
      footer={
        <>
          <Button variant="secondary" size="sm" onClick={onCancel}>
            {cancelLabel}
          </Button>
          <Button
            variant="danger"
            size="sm"
            disabled={!unlocked || busy}
            onClick={onConfirm}
          >
            {confirmLabel}
          </Button>
        </>
      }
    >
      {requireText && (
        <Input
          autoFocus
          className="w-full"
          placeholder={requireText}
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
      )}
      {children}
    </Dialog>
  );
}

/* ---------- FormDialog（编辑弹窗） ----------
   类、属性、关系、实体、事实区间的改动都在这一个壳里：面板只展示，改动开一扇窗。
   正文包在 <form> 里——输入框里按 Enter 就是保存；页脚在表单之外，取消与删除
   不会被当成提交。与 DangerConfirm 一样**总是挂载着**（open 恒真）：调用方条件渲染。
   删除放在左下角、次要样式：它和保存不是一对，隔开才不会顺手点到 */
type DangerAction = {
  label: string;
  disabled?: boolean;
  title?: string;
  onClick: () => void;
};

export function FormDialog({
  title,
  description,
  closeLabel,
  width = "md",
  saveLabel,
  cancelLabel,
  canSave = true,
  busy,
  onSave,
  onCancel,
  danger,
  children,
}: {
  title: ReactNode;
  description?: ReactNode;
  closeLabel: string;
  width?: "sm" | "md" | "lg";
  saveLabel: string;
  cancelLabel: string;
  /** 没填够就点不动保存 */
  canSave?: boolean;
  busy?: boolean;
  onSave: () => void;
  onCancel: () => void;
  /** 左下角的破坏性动作；缺省没有。**可以给一个，也可以给几个**——
   *  一个成员既能停用又能移出，两件事影响面不同，不该挤成一个按钮，
   *  也不该把其中一个赶到弹窗正文里去装成一个字段。 */
  danger?: DangerAction | DangerAction[];
  children: ReactNode;
}) {
  return (
    <Dialog
      open
      onOpenChange={(o) => {
        if (!o) onCancel();
      }}
      title={title}
      description={description}
      closeLabel={closeLabel}
      width={width}
      footer={
        <>
          {danger && (
            <div className="mr-auto flex items-center gap-2">
              {(Array.isArray(danger) ? danger : [danger]).map((d) => (
                <Button
                  key={d.label}
                  variant="secondary"
                  size="sm"
                  disabled={d.disabled || busy}
                  title={d.title}
                  onClick={d.onClick}
                >
                  {d.label}
                </Button>
              ))}
            </div>
          )}
          <Button variant="secondary" size="sm" onClick={onCancel}>
            {cancelLabel}
          </Button>
          <Button
            variant="primary"
            size="sm"
            disabled={!canSave}
            busy={busy}
            onClick={onSave}
          >
            {saveLabel}
          </Button>
        </>
      }
    >
      <form
        onSubmit={(e) => {
          e.preventDefault();
          if (canSave && !busy) onSave();
        }}
      >
        {children}
      </form>
    </Dialog>
  );
}


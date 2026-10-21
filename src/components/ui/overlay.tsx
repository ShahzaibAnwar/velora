import { type ReactNode } from "react";
import * as DialogPrimitive from "@radix-ui/react-dialog";
import * as DropdownPrimitive from "@radix-ui/react-dropdown-menu";
import * as PopoverPrimitive from "@radix-ui/react-popover";
import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import { AnimatePresence, motion } from "motion/react";
import { X } from "lucide-react";

import { cn } from "@/lib/cn";

// --- Tooltip -------------------------------------------------------------------------------------

export function TooltipProvider({ children }: { children: ReactNode }) {
  return (
    <TooltipPrimitive.Provider delayDuration={350} skipDelayDuration={150}>
      {children}
    </TooltipPrimitive.Provider>
  );
}

export function Tooltip({ content, children, side = "top" }: { content: ReactNode; children: ReactNode; side?: "top" | "bottom" | "left" | "right" }) {
  if (!content) return <>{children}</>;
  return (
    <TooltipPrimitive.Root>
      <TooltipPrimitive.Trigger asChild>{children}</TooltipPrimitive.Trigger>
      <TooltipPrimitive.Portal>
        <TooltipPrimitive.Content
          side={side}
          sideOffset={6}
          className="z-[80] max-w-72 rounded-lg border border-line bg-raised px-2.5 py-1.5 text-xs font-medium text-fg shadow-md data-[state=delayed-open]:animate-in"
        >
          {content}
        </TooltipPrimitive.Content>
      </TooltipPrimitive.Portal>
    </TooltipPrimitive.Root>
  );
}

// --- Dialog --------------------------------------------------------------------------------------

interface DialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: ReactNode;
  description?: ReactNode;
  children: ReactNode;
  footer?: ReactNode;
  width?: number;
  icon?: ReactNode;
}

export function Dialog({ open, onOpenChange, title, description, children, footer, width = 520, icon }: DialogProps) {
  return (
    <DialogPrimitive.Root open={open} onOpenChange={onOpenChange}>
      <AnimatePresence>
        {open && (
          <DialogPrimitive.Portal forceMount>
            <DialogPrimitive.Overlay asChild forceMount>
              <motion.div
                className="fixed inset-0 z-50 bg-[var(--overlay)] backdrop-blur-[3px]"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
                transition={{ duration: 0.18 }}
              />
            </DialogPrimitive.Overlay>
            <DialogPrimitive.Content asChild forceMount aria-describedby={description ? undefined : undefined}>
              <motion.div
                className="fixed left-1/2 top-1/2 z-50 flex max-h-[86vh] w-[calc(100vw-48px)] flex-col overflow-hidden rounded-[20px] border border-line-strong bg-card shadow-lg outline-none"
                style={{ maxWidth: width, x: "-50%", y: "-50%" }}
                initial={{ opacity: 0, scale: 0.96, y: "-48%" }}
                animate={{ opacity: 1, scale: 1, y: "-50%" }}
                exit={{ opacity: 0, scale: 0.97, y: "-49%" }}
                transition={{ type: "spring", stiffness: 420, damping: 32 }}
              >
                <div className="flex items-start gap-3 px-6 pt-5 pb-3">
                  {icon}
                  <div className="min-w-0 flex-1">
                    <DialogPrimitive.Title className="text-[17px] font-semibold tracking-[-0.01em]">{title}</DialogPrimitive.Title>
                    {description ? (
                      <DialogPrimitive.Description className="mt-1 text-[13px] text-muted">{description}</DialogPrimitive.Description>
                    ) : (
                      <DialogPrimitive.Description className="sr-only">{typeof title === "string" ? title : "Dialog"}</DialogPrimitive.Description>
                    )}
                  </div>
                  <DialogPrimitive.Close
                    className="-mr-2 grid size-8 place-items-center rounded-lg text-muted transition hover:bg-hover hover:text-fg"
                    aria-label="Close"
                  >
                    <X size={17} />
                  </DialogPrimitive.Close>
                </div>
                <div className="scroll-area min-h-0 flex-1 px-6 pb-5">{children}</div>
                {footer && <div className="flex items-center justify-end gap-2 border-t border-line bg-[color-mix(in_oklab,var(--card)_70%,var(--bg))] px-6 py-3.5">{footer}</div>}
              </motion.div>
            </DialogPrimitive.Content>
          </DialogPrimitive.Portal>
        )}
      </AnimatePresence>
    </DialogPrimitive.Root>
  );
}

// --- Dropdown menu -------------------------------------------------------------------------------

export const Menu = DropdownPrimitive.Root;
export const MenuTrigger = DropdownPrimitive.Trigger;

export function MenuContent({ children, align = "end" }: { children: ReactNode; align?: "start" | "end" | "center" }) {
  return (
    <DropdownPrimitive.Portal>
      <DropdownPrimitive.Content
        align={align}
        sideOffset={6}
        className="z-[70] min-w-52 origin-[var(--radix-dropdown-menu-content-transform-origin)] rounded-[14px] border border-line-strong bg-raised p-1.5 shadow-lg data-[state=open]:animate-[menu-in_140ms_ease-out]"
      >
        {children}
      </DropdownPrimitive.Content>
    </DropdownPrimitive.Portal>
  );
}

export function MenuItem({
  icon,
  children,
  onSelect,
  danger,
  disabled,
  shortcut,
}: {
  icon?: ReactNode;
  children: ReactNode;
  onSelect?: () => void;
  danger?: boolean;
  disabled?: boolean;
  shortcut?: string;
}) {
  return (
    <DropdownPrimitive.Item
      disabled={disabled}
      onSelect={onSelect}
      className={cn(
        "flex h-8.5 cursor-default items-center gap-2.5 rounded-[9px] px-2.5 text-[13px] font-medium outline-none transition-colors data-[disabled]:opacity-40 data-[highlighted]:bg-hover",
        danger ? "text-coral" : "text-fg",
      )}
    >
      <span className={cn("grid w-4 place-items-center", danger ? "text-coral" : "text-muted")}>{icon}</span>
      <span className="flex-1">{children}</span>
      {shortcut && <span className="text-[11px] text-subtle">{shortcut}</span>}
    </DropdownPrimitive.Item>
  );
}

export function MenuSeparator() {
  return <DropdownPrimitive.Separator className="my-1 h-px bg-line" />;
}

export function MenuLabel({ children }: { children: ReactNode }) {
  return <DropdownPrimitive.Label className="px-2.5 pt-1.5 pb-1 text-[11px] font-semibold uppercase tracking-wider text-subtle">{children}</DropdownPrimitive.Label>;
}

export const MenuSub = DropdownPrimitive.Sub;

export function MenuSubTrigger({ icon, children }: { icon?: ReactNode; children: ReactNode }) {
  return (
    <DropdownPrimitive.SubTrigger className="flex h-8.5 cursor-default items-center gap-2.5 rounded-[9px] px-2.5 text-[13px] font-medium outline-none data-[highlighted]:bg-hover data-[state=open]:bg-hover">
      <span className="grid w-4 place-items-center text-muted">{icon}</span>
      <span className="flex-1">{children}</span>
      <span className="text-subtle">›</span>
    </DropdownPrimitive.SubTrigger>
  );
}

export function MenuSubContent({ children }: { children: ReactNode }) {
  return (
    <DropdownPrimitive.Portal>
      <DropdownPrimitive.SubContent sideOffset={6} className="z-[71] min-w-44 rounded-[14px] border border-line-strong bg-raised p-1.5 shadow-lg">
        {children}
      </DropdownPrimitive.SubContent>
    </DropdownPrimitive.Portal>
  );
}

// --- Popover -------------------------------------------------------------------------------------

export function Popover({ trigger, children, align = "end", width = 340 }: { trigger: ReactNode; children: ReactNode; align?: "start" | "end"; width?: number }) {
  return (
    <PopoverPrimitive.Root>
      <PopoverPrimitive.Trigger asChild>{trigger}</PopoverPrimitive.Trigger>
      <PopoverPrimitive.Portal>
        <PopoverPrimitive.Content
          align={align}
          sideOffset={8}
          style={{ width }}
          className="z-[70] rounded-2xl border border-line-strong bg-raised p-2 shadow-lg outline-none data-[state=open]:animate-[menu-in_160ms_ease-out]"
        >
          {children}
        </PopoverPrimitive.Content>
      </PopoverPrimitive.Portal>
    </PopoverPrimitive.Root>
  );
}

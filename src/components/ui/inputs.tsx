import { forwardRef, useId, type InputHTMLAttributes, type ReactNode, type TextareaHTMLAttributes } from "react";
import * as SwitchPrimitive from "@radix-ui/react-switch";
import * as SelectPrimitive from "@radix-ui/react-select";
import { Check, ChevronDown } from "lucide-react";
import { motion } from "motion/react";

import { cn } from "@/lib/cn";

const fieldBase =
  "w-full rounded-xl border border-line bg-[color-mix(in_oklab,var(--bg)_55%,var(--card))] px-3 text-[13px] text-fg placeholder:text-subtle transition-[border-color,box-shadow] outline-none focus:border-[color-mix(in_oklab,var(--accent)_60%,transparent)] focus:shadow-[0_0_0_3px_color-mix(in_oklab,var(--accent)_16%,transparent)] disabled:opacity-50";

export const Input = forwardRef<HTMLInputElement, InputHTMLAttributes<HTMLInputElement> & { leading?: ReactNode; trailing?: ReactNode }>(
  function Input({ className, leading, trailing, ...props }, ref) {
    if (!leading && !trailing) return <input ref={ref} className={cn(fieldBase, "h-9.5", className)} {...props} />;
    return (
      <div className="relative flex items-center">
        {leading && <span className="pointer-events-none absolute left-3 text-subtle">{leading}</span>}
        <input ref={ref} className={cn(fieldBase, "h-9.5", leading && "pl-9", trailing && "pr-10", className)} {...props} />
        {trailing && <span className="absolute right-1.5">{trailing}</span>}
      </div>
    );
  },
);

export const Textarea = forwardRef<HTMLTextAreaElement, TextareaHTMLAttributes<HTMLTextAreaElement>>(function Textarea({ className, ...props }, ref) {
  return <textarea ref={ref} className={cn(fieldBase, "min-h-20 resize-none py-2.5 leading-relaxed", className)} {...props} />;
});

export function Field({ label, hint, children, className }: { label: ReactNode; hint?: ReactNode; children: ReactNode; className?: string }) {
  return (
    <label className={cn("grid gap-1.5", className)}>
      <span className="text-[12px] font-semibold text-muted">{label}</span>
      {children}
      {hint && <span className="text-[11.5px] text-subtle">{hint}</span>}
    </label>
  );
}

export function Switch({ checked, onCheckedChange, disabled, label }: { checked: boolean; onCheckedChange: (v: boolean) => void; disabled?: boolean; label?: string }) {
  return (
    <SwitchPrimitive.Root
      checked={checked}
      onCheckedChange={onCheckedChange}
      disabled={disabled}
      aria-label={label}
      className={cn(
        "relative h-[22px] w-[38px] shrink-0 rounded-full border border-line transition-colors duration-200 disabled:opacity-40",
        checked ? "bg-accent" : "bg-track",
      )}
    >
      <SwitchPrimitive.Thumb asChild>
        <motion.span
          layout
          transition={{ type: "spring", stiffness: 700, damping: 35 }}
          className={cn("block size-4 rounded-full shadow-sm", checked ? "ml-[18px] bg-on-accent" : "ml-[2px] bg-[color-mix(in_oklab,var(--fg)_75%,transparent)]")}
        />
      </SwitchPrimitive.Thumb>
    </SwitchPrimitive.Root>
  );
}

export interface Option<T extends string> {
  value: T;
  label: ReactNode;
}

export function Select<T extends string>({
  value,
  onValueChange,
  options,
  className,
  disabled,
  ariaLabel,
}: {
  value: T;
  onValueChange: (v: T) => void;
  options: Option<T>[];
  className?: string;
  disabled?: boolean;
  ariaLabel?: string;
}) {
  return (
    <SelectPrimitive.Root value={value} onValueChange={(v) => onValueChange(v as T)} disabled={disabled}>
      <SelectPrimitive.Trigger aria-label={ariaLabel} className={cn(fieldBase, "flex h-9.5 items-center justify-between gap-2 text-left", className)}>
        <SelectPrimitive.Value />
        <SelectPrimitive.Icon>
          <ChevronDown size={15} className="text-subtle" />
        </SelectPrimitive.Icon>
      </SelectPrimitive.Trigger>
      <SelectPrimitive.Portal>
        <SelectPrimitive.Content
          position="popper"
          sideOffset={6}
          className="z-[90] max-h-72 min-w-[var(--radix-select-trigger-width)] overflow-hidden rounded-[14px] border border-line-strong bg-raised p-1.5 shadow-lg"
        >
          <SelectPrimitive.Viewport>
            {options.map((o) => (
              <SelectPrimitive.Item
                key={o.value}
                value={o.value}
                className="flex h-8.5 cursor-default items-center gap-2 rounded-[9px] px-2.5 text-[13px] font-medium outline-none data-[highlighted]:bg-hover"
              >
                <SelectPrimitive.ItemText>{o.label}</SelectPrimitive.ItemText>
                <SelectPrimitive.ItemIndicator className="ml-auto text-accent">
                  <Check size={14} />
                </SelectPrimitive.ItemIndicator>
              </SelectPrimitive.Item>
            ))}
          </SelectPrimitive.Viewport>
        </SelectPrimitive.Content>
      </SelectPrimitive.Portal>
    </SelectPrimitive.Root>
  );
}

/** Pill-style single choice with an animated highlight. */
export function Segmented<T extends string>({
  value,
  onValueChange,
  options,
  className,
}: {
  value: T;
  onValueChange: (v: T) => void;
  options: Option<T>[];
  className?: string;
}) {
  const id = useId();
  return (
    <div role="radiogroup" className={cn("inline-flex rounded-xl border border-line bg-[color-mix(in_oklab,var(--bg)_55%,var(--card))] p-1", className)}>
      {options.map((o) => {
        const active = o.value === value;
        return (
          <button
            key={o.value}
            role="radio"
            aria-checked={active}
            onClick={() => onValueChange(o.value)}
            className={cn("relative h-7.5 flex-1 rounded-[9px] px-3 text-[12.5px] font-semibold transition-colors", active ? "text-fg" : "text-muted hover:text-fg")}
          >
            {active && (
              <motion.span
                layoutId={`seg-${id}`}
                className="absolute inset-0 rounded-[9px] border border-line-strong bg-raised shadow-sm"
                transition={{ type: "spring", stiffness: 500, damping: 36 }}
              />
            )}
            <span className="relative">{o.label}</span>
          </button>
        );
      })}
    </div>
  );
}

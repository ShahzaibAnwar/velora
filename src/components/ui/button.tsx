import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from "react";
import { motion, type HTMLMotionProps } from "motion/react";

import { cn } from "@/lib/cn";
import { Tooltip } from "./overlay";

type Variant = "primary" | "secondary" | "ghost" | "danger" | "subtle";
type Size = "sm" | "md" | "lg";

const variants: Record<Variant, string> = {
  primary:
    "text-on-accent bg-[linear-gradient(135deg,var(--accent),color-mix(in_oklab,var(--accent)_55%,var(--accent-2)))] shadow-[0_6px_20px_-8px_color-mix(in_oklab,var(--accent)_70%,transparent)] hover:brightness-110",
  secondary: "bg-raised text-fg border border-line hover:border-line-strong hover:bg-[color-mix(in_oklab,var(--raised)_80%,var(--fg)_6%)]",
  ghost: "text-muted hover:text-fg hover:bg-hover",
  subtle: "bg-hover text-fg hover:bg-[color-mix(in_oklab,var(--fg)_9%,transparent)]",
  danger: "bg-[color-mix(in_oklab,var(--coral)_16%,transparent)] text-coral hover:bg-[color-mix(in_oklab,var(--coral)_24%,transparent)]",
};

const sizes: Record<Size, string> = {
  sm: "h-8 px-3 text-[12.5px] gap-1.5 rounded-[10px]",
  md: "h-9 px-3.5 text-[13px] gap-2 rounded-xl",
  lg: "h-11 px-5 text-sm gap-2 rounded-xl",
};

export interface ButtonProps extends Omit<HTMLMotionProps<"button">, "children"> {
  variant?: Variant;
  size?: Size;
  icon?: ReactNode;
  children?: ReactNode;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = "secondary", size = "md", icon, className, children, ...props },
  ref,
) {
  return (
    <motion.button
      ref={ref}
      whileTap={{ scale: 0.97 }}
      transition={{ type: "spring", stiffness: 600, damping: 30 }}
      className={cn(
        "inline-flex select-none items-center justify-center whitespace-nowrap font-semibold transition-[background,color,border-color,filter,box-shadow] duration-150 disabled:pointer-events-none disabled:opacity-45",
        variants[variant],
        sizes[size],
        className,
      )}
      {...props}
    >
      {icon}
      {children}
    </motion.button>
  );
});

interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  label: string;
  tone?: "default" | "accent" | "danger";
  size?: "sm" | "md";
  tooltip?: boolean;
}

export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(function IconButton(
  { label, tone = "default", size = "md", tooltip = true, className, children, ...props },
  ref,
) {
  const btn = (
    <button
      ref={ref}
      aria-label={label}
      className={cn(
        "inline-grid shrink-0 place-items-center rounded-[10px] transition-[background,color,transform] duration-150 active:scale-[0.92] disabled:pointer-events-none disabled:opacity-40",
        size === "sm" ? "size-7" : "size-8.5",
        tone === "default" && "text-muted hover:bg-hover hover:text-fg",
        tone === "accent" && "text-accent hover:bg-[color-mix(in_oklab,var(--accent)_14%,transparent)]",
        tone === "danger" && "text-muted hover:bg-[color-mix(in_oklab,var(--coral)_14%,transparent)] hover:text-coral",
        className,
      )}
      {...props}
    >
      {children}
    </button>
  );
  return tooltip ? <Tooltip content={label}>{btn}</Tooltip> : btn;
});

export function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd className="rounded-md border border-line bg-hover px-1.5 py-px font-sans text-[10.5px] font-semibold text-muted">{children}</kbd>
  );
}

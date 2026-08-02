import type { ButtonHTMLAttributes } from "react";

// `violet` and `teal` are the accent colours the sweep and walk-forward tabs
// already used for their run buttons; they are variants rather than className
// overrides because two competing `bg-*` utilities resolve by CSS source order,
// not by class-string order, which makes overriding unreliable.
export type ButtonVariant =
  | "primary"
  | "secondary"
  | "danger"
  | "ghost"
  | "violet"
  | "teal";
export type ButtonSize = "sm" | "md";

const DISABLED_SOLID = "disabled:bg-slate-700 disabled:text-slate-400";

const VARIANTS: Record<ButtonVariant, string> = {
  primary: `bg-sky-500 text-white hover:bg-sky-400 ${DISABLED_SOLID}`,
  secondary:
    "border border-slate-600 bg-slate-800 text-slate-200 hover:bg-slate-700 disabled:opacity-50",
  danger: `bg-rose-600 text-white hover:bg-rose-500 ${DISABLED_SOLID}`,
  ghost: "text-slate-400 hover:text-slate-100 disabled:opacity-50",
  violet: `bg-violet-600 text-white hover:bg-violet-500 ${DISABLED_SOLID}`,
  teal: `bg-teal-600 text-white hover:bg-teal-500 ${DISABLED_SOLID}`,
};

const SIZES: Record<ButtonSize, string> = {
  sm: "px-2.5 py-1 text-xs",
  md: "px-4 py-2 text-sm",
};

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  loading?: boolean;
}

export function Button({
  variant = "primary",
  size = "md",
  loading = false,
  disabled,
  className = "",
  children,
  ...rest
}: ButtonProps) {
  return (
    <button
      type="button"
      disabled={disabled || loading}
      className={`inline-flex items-center justify-center gap-2 rounded-md font-medium transition disabled:cursor-not-allowed ${VARIANTS[variant]} ${SIZES[size]} ${className}`}
      {...rest}
    >
      {loading && (
        <span className="h-3.5 w-3.5 animate-spin rounded-full border-2 border-white/30 border-t-white" />
      )}
      {children}
    </button>
  );
}

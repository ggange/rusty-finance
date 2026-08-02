import type { InputHTMLAttributes } from "react";

export type ControlSize = "sm" | "md";

// The shared control styling for inputs and selects. Kept here so the two stay
// visually identical — they sit next to each other in most forms.
export const controlBase =
  "w-full rounded-md border border-slate-600 bg-slate-900 text-sm text-slate-100 focus:border-sky-400 focus:outline-none disabled:cursor-not-allowed disabled:opacity-50";

export const controlPadding: Record<ControlSize, string> = {
  sm: "px-2.5 py-1.5",
  md: "px-3 py-1.5",
};

export function controlClass(size: ControlSize = "md", extra = ""): string {
  return `${controlBase} ${controlPadding[size]} ${extra}`.trim();
}

// `size` is omitted from the native attributes: on <input> it means "visible
// character width" (a number), and we want it to mean control density here.
interface InputProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "size"> {
  size?: ControlSize;
}

export function Input({ size = "md", className = "", ...rest }: InputProps) {
  return <input className={controlClass(size, className)} {...rest} />;
}

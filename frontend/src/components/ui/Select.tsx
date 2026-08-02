import type { SelectHTMLAttributes } from "react";
import { controlClass, type ControlSize } from "./Input";

// See Input.tsx: native `size` on <select> is a row count, so it is replaced
// with the same density scale used by the other controls.
interface SelectProps extends Omit<SelectHTMLAttributes<HTMLSelectElement>, "size"> {
  size?: ControlSize;
}

export function Select({ size = "md", className = "", ...rest }: SelectProps) {
  return <select className={controlClass(size, className)} {...rest} />;
}

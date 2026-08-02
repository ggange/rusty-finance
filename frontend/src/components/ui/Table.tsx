import type { ReactNode, ThHTMLAttributes, TdHTMLAttributes } from "react";

type Align = "left" | "right" | "center";

// Looked up rather than interpolated: Tailwind's scanner only sees literal
// class strings, so `text-${align}` would never be generated.
const ALIGN: Record<Align, string> = {
  left: "text-left",
  right: "text-right",
  center: "text-center",
};

interface TableProps {
  head: ReactNode;
  children: ReactNode;
  /** Tailwind max-height class; the header stays sticky while the body scrolls. */
  maxHeight?: string;
  className?: string;
}

export function Table({ head, children, maxHeight = "max-h-80", className = "" }: TableProps) {
  return (
    <div className={`${maxHeight} overflow-auto rounded-lg border border-slate-700 ${className}`}>
      <table className="w-full text-sm">
        <thead className="sticky top-0 bg-slate-900 text-xs uppercase tracking-wide text-slate-400">
          <tr>{head}</tr>
        </thead>
        <tbody className="divide-y divide-slate-800">{children}</tbody>
      </table>
    </div>
  );
}

interface ThProps extends ThHTMLAttributes<HTMLTableCellElement> {
  align?: Align;
}

export function Th({ align = "left", className = "", ...rest }: ThProps) {
  return <th className={`px-3 py-2 ${ALIGN[align]} ${className}`} {...rest} />;
}

interface TdProps extends TdHTMLAttributes<HTMLTableCellElement> {
  align?: Align;
  /** Numeric cells get tabular figures so columns line up. */
  numeric?: boolean;
}

export function Td({ align = "left", numeric = false, className = "", ...rest }: TdProps) {
  return (
    <td
      className={`px-3 py-1.5 ${ALIGN[align]} ${numeric ? "tabular-nums" : ""} ${className}`}
      {...rest}
    />
  );
}

export function Tr({ children, className = "" }: { children: ReactNode; className?: string }) {
  return <tr className={`text-slate-200 ${className}`}>{children}</tr>;
}

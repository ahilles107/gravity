import type { ReactElement } from "react";

interface TableHeadProps {
  readonly columns: readonly string[];
  /** Screen-reader label for a trailing, visually empty action column. */
  readonly actionLabel?: string;
}

export default function TableHead({ columns, actionLabel }: TableHeadProps): ReactElement {
  return (
    <thead>
      <tr>
        {columns.map((column) => (
          <th key={column}>{column}</th>
        ))}
        {actionLabel === undefined ? null : (
          <th>
            <span className="visually-hidden">{actionLabel}</span>
          </th>
        )}
      </tr>
    </thead>
  );
}

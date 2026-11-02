import type { ReactNode } from "react";
import { Skeleton } from "./ui/skeleton";
import { TableRow, TableCell } from "./ui/table";
export function ResultsToolbar({ children }: { children: ReactNode }) {
  return <div className="results-toolbar">{children}</div>;
}
export function LoadingRows({
  columns,
  rows = 8,
}: {
  columns: number;
  rows?: number;
}) {
  return (
    <>
      {Array.from({ length: rows }, (_, i) => (
        <TableRow aria-hidden="true" key={i}>
          {Array.from({ length: columns }, (_, j) => (
            <TableCell key={j}>
              <Skeleton className="h-4 w-full" />
            </TableCell>
          ))}
        </TableRow>
      ))}
    </>
  );
}

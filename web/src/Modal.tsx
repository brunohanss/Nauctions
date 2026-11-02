import { Separator } from "./components/ui/separator";
import { useRef, type ReactNode } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "./components/ui/dialog";
export function Modal({
  title,
  children,
  onClose,
}: {
  title: string;
  children: ReactNode;
  onClose: () => void;
}) {
  const opener = useRef(
    document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null,
  );
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <DialogContent
        aria-describedby={undefined}
        className="detail-dialog"
        onCloseAutoFocus={(e) => {
          e.preventDefault();
          const target =
            opener.current?.isConnected &&
            opener.current !== document.body &&
            opener.current !== document.documentElement
              ? opener.current
              : document.getElementById("main-content");
          target?.focus({ preventScroll: true });
        }}
      >
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        <Separator />
        <div className="detail-dialog-body">{children}</div>
      </DialogContent>
    </Dialog>
  );
}

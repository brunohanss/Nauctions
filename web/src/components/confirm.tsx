import { createContext, useContext, useState, type ReactNode } from "react";
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogCancel,
  AlertDialogAction,
} from "./ui/alert-dialog";
const Context = createContext<(message: string) => Promise<boolean>>(
  async () => false,
);
export const useConfirm = () => useContext(Context);
export function ConfirmProvider({ children }: { children: ReactNode }) {
  const [pending, setPending] = useState<{
    message: string;
    resolve: (value: boolean) => void;
  } | null>(null);
  const finish = (value: boolean) => {
    pending?.resolve(value);
    setPending(null);
  };
  return (
    <Context.Provider
      value={(message) =>
        new Promise((resolve) => setPending({ message, resolve }))
      }
    >
      {children}
      <AlertDialog
        open={!!pending}
        onOpenChange={(open) => {
          if (!open) finish(false);
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Confirm action</AlertDialogTitle>
            <AlertDialogDescription>{pending?.message}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel onClick={() => finish(false)}>
              Cancel
            </AlertDialogCancel>
            <AlertDialogAction onClick={() => finish(true)}>
              Continue
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </Context.Provider>
  );
}

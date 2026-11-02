import { useEffect, useId, useRef, type ReactNode } from 'react';
import { X } from 'lucide-react';

let openDialogs = 0;
let previousOverflow = '';

/** Native modal dialogs provide focus containment, inert backgrounds and Escape. */
export function Modal({ title, children, onClose }: { title: string; children: ReactNode; onClose: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null);
  const heading = useId();
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    const element = dialog.current!;
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    if (openDialogs++ === 0) { previousOverflow = document.body.style.overflow; document.body.style.overflow = 'hidden'; }
    element.showModal();
    return () => {
      element.close();
      if (--openDialogs === 0) document.body.style.overflow = previousOverflow;
      if (opener?.isConnected) opener.focus({ preventScroll: true });
    };
  }, []);
  return <dialog ref={dialog} className="product-modal" aria-labelledby={heading}
    onCancel={event => { event.preventDefault(); close.current(); }}>
    <div className="product-modal-heading"><h2 id={heading}>{title}</h2><button type="button" aria-label={`Close ${title}`} onClick={onClose} autoFocus><X size={20}/></button></div>
    <div className="product-modal-content">{children}</div>
  </dialog>;
}

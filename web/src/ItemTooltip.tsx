import { cloneElement, useEffect, useMemo, useState, type ReactElement } from 'react';
import { api } from './api';
import { Tooltip, TooltipContent, TooltipTrigger } from './components/ui/tooltip';

type Metadata = { name: string; quality: number; tooltip: string };
const bundled = import.meta.glob<Metadata>('../../assets/item-icon-metadata/*.json', { eager: true, import: 'default' });
const requests = new Map<number, Promise<Metadata>>();

// Convert source markup to text. Never insert third-party HTML into the page.
export function itemDescription(html: string, name: string): string[] {
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll('script, style, iframe, object').forEach(node => node.remove());
  doc.querySelectorAll('.moneygold, .moneysilver, .moneycopper').forEach(node => {
    node.append(node.classList.contains('moneygold') ? 'g' : node.classList.contains('moneysilver') ? 's' : 'c');
  });
  doc.querySelectorAll('br').forEach(node => node.replaceWith('\n'));
  doc.querySelectorAll('div, tr, table').forEach(node => { node.prepend('\n'); node.append('\n'); });
  doc.querySelectorAll('td').forEach(node => node.append(' '));
  return (doc.body.textContent ?? '').split('\n').map(line => line.replace(/\s+/g, ' ').trim()).filter(line => line && line !== name);
}

export function ItemTooltip({ name, itemId, itemKey, category, rarity, children }: {
  name: string; itemId?: number | string | null; itemKey?: string; category?: string | null; rarity?: number | null; children: ReactElement<{ 'data-item-rarity'?: number }>;
}) {
  const id = Number(itemId ?? /^i(\d+)(?=:|$)/.exec(itemKey ?? '')?.[1]);
  const local = bundled[`../../assets/item-icon-metadata/${id}.json`];
  const [open, setOpen] = useState(false);
  const [loaded, setLoaded] = useState<{ id: number; data?: Metadata; failed?: boolean }>();
  const data = local ?? (loaded?.id === id ? loaded.data : undefined);
  const failed = loaded?.id === id && loaded.failed;
  useEffect(() => {
    if (!open || local || !Number.isSafeInteger(id) || id <= 0) return;
    let active = true;
    let request = requests.get(id);
    if (!request) {
      request = api<Metadata>(`/item-metadata/${id}`);
      requests.set(id, request);
      // Bound the session cache. Failed requests can be retried on the next hover.
      if (requests.size > 256) requests.delete(requests.keys().next().value!);
      request.catch(() => requests.delete(id));
    }
    request.then(data => { if (active) setLoaded({ id, data }); }, () => { if (active) setLoaded({ id, failed: true }); });
    return () => { active = false; };
  }, [open, id, local]);
  const lines = useMemo(() => open && data ? itemDescription(data.tooltip, data.name) : [], [open, data]);
  const quality = rarity ?? data?.quality;
  return <Tooltip open={open} onOpenChange={setOpen} delayDuration={250}>
    <TooltipTrigger asChild>{cloneElement(children, { 'data-item-rarity': quality })}</TooltipTrigger>
    <TooltipContent className="item-tooltip" data-item-rarity={quality} side="right" sideOffset={10} collisionPadding={12}>
      <strong className="item-tooltip-name">{/^Item \d+$/.test(name) ? data?.name ?? name : name}</strong>
      {category && <span className="item-tooltip-category">{category}</span>}
      {lines.map((line, index) => <span key={index} className={/^(Use:|Equip:|Chance on hit:)/.test(line) ? 'item-tooltip-effect' : undefined}>{line}</span>)}
      {!data && <span className="item-tooltip-category">{!Number.isSafeInteger(id) || id <= 0 || failed ? 'Description unavailable.' : 'Loading description…'}</span>}
      {data && <small className="item-tooltip-source">WoW Classic · Wowhead / Blizzard</small>}
    </TooltipContent>
  </Tooltip>;
}

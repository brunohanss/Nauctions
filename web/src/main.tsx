import { MarketMedianCells } from './MarketMedianCells';
import { Percentage } from './Percentage';
import { ItemTooltip } from './ItemTooltip';
import { ResultsToolbar, LoadingRows } from './components/results';
import { TooltipProvider } from './components/ui/tooltip';
import { Kbd } from './components/ui/kbd';
import { Card } from './components/ui/card';
import { Empty } from './components/ui/empty';
import { Skeleton } from './components/ui/skeleton';
import { Alert } from './components/ui/alert';
import { ConfirmProvider, useConfirm } from './components/confirm';
import { Popover, PopoverAnchor, PopoverContent } from './components/ui/popover';
import { Command, CommandInput, CommandList, CommandItem, CommandEmpty } from './components/ui/command';
import { Pager, Help, Button, Input, Label, Select, Table, TableHeader, TableBody, TableRow, TableHead, TableCell, Details } from './components/controls';
import { MarketClock, useMarketRefresh } from './marketClock';
import React, { useEffect, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { Search, Settings as SettingsIcon, ChevronLeft, ChevronRight, ArrowDown, ArrowUp, X } from 'lucide-react';
import { api, ApiError, setCsrf, time, type Item, type Member, type Market } from './api';
import { ItemIcon, WowMoney, FreshnessIndicator, PriceSparkline } from './market-ui';
import { Detail } from './Detail';
import { Watchlist } from './Watchlist';
import { WatchlistProvider, useWatchlist } from './WatchlistState';
import { marketPage } from './marketPage';
import { Modal } from './Modal';
import { MarketForecastCells, useMarketForecasts } from './MarketForecastCells';
import './shadcn.css';
import './style.css';

import { Toaster } from './components/ui/sonner';
import { Strategy } from './Strategy';
import { Crafting } from './Crafting';
import { ForecastExportButton, forecastInstallNote } from './Forecasts';
import { forecastFresh } from './forecast-types';
import { normalizeRoute, supportedMarkets } from './navigation';
import { DesktopRoot } from './DesktopShell';
import { DesktopSetup } from './DesktopSetup';
import { ApplicationSettings, SignedOutSettings } from './SettingsAccess';
import { currentRoute, routeUrl, desktopInvoke, isDesktop } from './desktop';
import { initTelemetry, setTelemetryEnabled, trackScreen } from './telemetry';
import { RecoveryBoundary } from './Diagnostics';
import { StartupGate, StartupScreen } from './StartupScreen';
type PageData = {
    items: Item[];
    next_cursor: string | null;
    total_count?: number;
};
type ListState = {
    category: string;
    sort: string;
    direction: string;
    cursors: string[];
    page: number;
    scroll: number;
};
const initialList = (): ListState => ({ category: 'all', sort: 'reference_profit', direction: 'desc', cursors: [''], page: 0, scroll: 0 });
const categories = ['all', 'materials', 'consumables', 'recipes', 'equipment', 'other', 'watchlist'];
function App() {
    const [member, setMember] = useState<Member | null>(null), [boot, setBoot] = useState(true), [markets, setMarkets] = useState<Market[]>([]), [market, setMarket] = useState(''), [route, setRoute] = useState(normalizeRoute(currentRoute())), [query, setQuery] = useState(''), [error, setError] = useState('');
    const [list, setList] = useState(initialList);
    const [modalItem, setModalItem] = useState<string | null>(null);
    useEffect(() => { trackScreen(route); }, [route]);
    async function session() { try {
        const me = await api<Member>('/me');
        setCsrf(me.csrf_token);
        const m = await api<Market[]>('/markets');
        const supported = supportedMarkets(m);
        setMarkets(supported);
        setMarket(current => supported.some(x => x.id === current) ? current : supported[0]?.id ?? '');
        setMember(me);
    }
    catch (e) {
        if (!(e instanceof ApiError && e.status === 401))
            setError((e as Error).message);
    }
    finally {
        setBoot(false);
    } }
    useEffect(() => {
        void session();
        const syncRoute = () => {
            const current = currentRoute(), next = normalizeRoute(current);
            if (next !== current) history.replaceState(null, '', routeUrl(next) + location.search);
            setModalItem(null); setRoute(next);
        };
        syncRoute();
        window.addEventListener('popstate', syncRoute);
        return () => window.removeEventListener('popstate', syncRoute);
    }, []);
    function navigate(path: string) {
        if (path.startsWith('/items/')) { setModalItem(decodeURIComponent(path.slice(7))); return; }
        setModalItem(null);
        if (route === '/market') setList(s => ({ ...s, scroll: window.scrollY }));
        const next = normalizeRoute(path);
        history.pushState(null, '', routeUrl(next)); setRoute(next); window.scrollTo(0, 0);
    }
    function closeItem() {
        setModalItem(null);
        if (route.startsWith('/items/')) { history.replaceState(null, '', routeUrl('/market')); setRoute('/market'); }
    }
    const selectedMarket = markets.find(m => m.id === market);
    const detail = route.startsWith('/items/');
    const openItem = modalItem ?? (detail ? decodeURIComponent(route.slice(7)) : null);
    useEffect(() => { if (route === '/market') {
        requestAnimationFrame(() => window.scrollTo(0, list.scroll));
    } }, [route]);
    if (boot)
        return <StartupScreen/>;
    if (!member)
        return <><SignedOutSettings/><Join onLogin={session} error={error}/>{error && <Button onClick={() => { setError(''); setBoot(true); void session(); }}>Retry connection</Button>}</>;
    return <WatchlistProvider key={member.id}><DesktopSetup member={member} demo={selectedMarket?.demo}><a className="skip-link" href="#main-content">Skip to content</a><header className="app-header"><div className="header-inner"><a href={routeUrl('/market')} className="brand" onClick={e => { if (e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return; e.preventDefault(); navigate('/market'); }}><span className="brand-bars" aria-hidden="true"><i /><i /><i /></span>Nauctions</a><nav className="nav-tabs" aria-label="Main navigation">{['market', 'crafting', 'strategy', 'watchlist'].map(p => <a key={p} href={routeUrl(`/${p}`)} aria-current={route === `/${p}` || (p === 'market' && detail) ? 'page' : undefined} onClick={e => { if (e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return; e.preventDefault(); navigate(`/${p}`); }}>{p === 'market' ? 'Market' : p === 'crafting' ? 'Crafting' : p === 'strategy' ? 'Strategy' : 'Watchlist'}</a>)}</nav><a className="settings-button" href={routeUrl('/settings')} aria-label="Settings" aria-current={route === '/settings' ? 'page' : undefined} onClick={e => { if (e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return; e.preventDefault(); navigate('/settings'); }}><SettingsIcon size={19}/></a></div></header><main id="main-content" tabIndex={-1} className="app-main"><MarketClock asOf={selectedMarket?.demo ? selectedMarket.as_of : null}><div className="market-context"><strong>{selectedMarket ? `${selectedMarket.realm} · ${selectedMarket.house}` : 'No market available'}</strong><span>{selectedMarket?.last_observed_at ? `Latest scan: ${time(selectedMarket.last_observed_at)}` : 'Waiting for the first accepted market scan'}</span></div>{selectedMarket?.demo && <p className="demo-notice" role="note"><strong>Simulated market</strong><span>50 items · 63 days of test scenarios{selectedMarket.as_of ? ` · Replay: ${time(selectedMarket.as_of)}` : ""}</span></p>}{!['/crafting', '/settings', '/watchlist'].includes(route) && <GlobalItemSearch market={market} query={query} setQuery={q => { setQuery(q); setList(s => ({ ...s, page: 0, cursors: [''], scroll: 0 })); }} onSelect={key => navigate(`/items/${encodeURIComponent(key)}`)}/>}{error && <Alert className="error-state" role="alert">{error}<Button onClick={() => { setError(''); void session(); }}>Retry connection</Button><Button aria-label="Dismiss error" onClick={() => setError('')}><X size={16}/></Button></Alert>}{route === '/settings' ? <Settings member={member} markets={markets} market={market} setMarket={id => { setMarket(id); setList(initialList()); }} logout={() => setMember(null)}/> : route === '/crafting' ? <Crafting market={market} neutral={selectedMarket?.house === 'Neutral'} onItem={key => navigate('/items/' + encodeURIComponent(key))}/> : route === '/watchlist' ? <Watchlist market={market} onItem={key => navigate('/items/' + encodeURIComponent(key))}/> : route === '/strategy' ? <Strategy market={market} query={query} onItem={key => navigate('/items/' + encodeURIComponent(key))}/> : <MarketTable market={market} query={query} state={list} update={setList} onSelect={key => navigate(`/items/${encodeURIComponent(key)}`)} resetQuery={() => setQuery('')}/>}{openItem && <Modal title="Item details" onClose={closeItem}><Detail key={`${market}:${openItem}`} itemKey={openItem} market={market} back={closeItem}/></Modal>}</MarketClock></main></DesktopSetup></WatchlistProvider>;
}
function GlobalItemSearch({ market, query, setQuery, onSelect }: {
    market: string;
    query: string;
    setQuery: (q: string) => void;
    onSelect: (key: string) => void;
}) {
    const input = useRef<HTMLInputElement>(null), [open, setOpen] = useState(false), [rows, setRows] = useState<Item[]>([]), [selected, setSelected] = useState(''), [busy, setBusy] = useState(false), [error, setError] = useState('');
    useEffect(() => { const key = (e: KeyboardEvent) => { const el = e.target as HTMLElement; if (e.key === '/' && !document.querySelector('[role=dialog]:not([data-state=closed]), [role=alertdialog]:not([data-state=closed])') && !el.closest('input,textarea,select,[contenteditable="true"]')) {
        e.preventDefault();
        input.current?.focus();
    } }; window.addEventListener('keydown', key); return () => window.removeEventListener('keydown', key); }, []);
    useEffect(() => { let active = true; setRows([]); setSelected(''); setError(''); setBusy(!!query.trim() && !!market); if (!query.trim() || !market)
        return; const timer = setTimeout(() => api<PageData>(`/items?${new URLSearchParams({ market_id: market, q: query, limit: '12', sort: 'name', direction: 'asc' })}`).then(data => { if (active)
        setRows(data.items); }).catch(e => { if (active)
        setError(e.message); }).finally(() => { if (active)
        setBusy(false); }), 200); return () => { active = false; clearTimeout(timer); }; }, [query, market]);
    function select(key: string) { setOpen(false); onSelect(key); input.current?.blur(); }
    const [listId, setListId] = useState('');
    const expanded = open && !!query.trim();
    return <Popover open={expanded} onOpenChange={setOpen}><div className="global-search"><Command shouldFilter={false} value={selected} onValueChange={setSelected}>
      <PopoverAnchor asChild><div><CommandInput asChild ref={input} value={query} onFocus={() => setOpen(true)} onValueChange={q => {setQuery(q); setOpen(true);}} onKeyDown={e => {
        if(e.key === 'Enter' && expanded && rows.length && !e.nativeEvent.isComposing) {e.preventDefault();e.stopPropagation();select((rows.find(row=>row.item_key===selected) ?? rows[0]).item_key);}
        if(e.key === 'Escape') {e.preventDefault(); if(expanded)setOpen(false); else setQuery('');}
      }}><Input aria-label="Search for an item" placeholder="Search for an item..." aria-expanded={expanded} aria-controls={expanded && listId ? listId : undefined} className="border-0 shadow-none focus-visible:ring-0"/></CommandInput></div></PopoverAnchor>
      {query ? <Button variant="ghost" aria-label="Clear search" onClick={() => {setQuery('');input.current?.focus();}}>Clear</Button> : <Kbd>/</Kbd>}
      <PopoverContent onEscapeKeyDown={e=>{e.preventDefault();e.stopPropagation();setOpen(false);}} align="start" side="bottom" avoidCollisions={false} onOpenAutoFocus={e=>e.preventDefault()} onCloseAutoFocus={e=>e.preventDefault()} className="w-[var(--radix-popover-trigger-width)] p-0"><CommandList ref={element=>{if(element)setListId(element.id);}}>
        {busy ? <p role="status">Searching...</p> : error ? <p role="alert">{error}</p> : <><CommandEmpty>No items found.</CommandEmpty>{rows.map(item=><ItemTooltip key={item.item_key} name={item.name} itemId={item.item_id} itemKey={item.item_key} category={item.category} rarity={item.rarity}><CommandItem value={item.item_key} onSelect={()=>select(item.item_key)}><ItemIcon name={item.name} icon={item.icon}/><span data-item-name>{item.name}</span></CommandItem></ItemTooltip>)}</>}
      </CommandList></PopoverContent>
    </Command></div></Popover>;

}
function MarketTable({ market, query, state, update, onSelect, resetQuery }: {
    market: string;
    query: string;
    state: ListState;
    update: (s: ListState) => void;
    onSelect: (k: string) => void;
    resetQuery: () => void;
}) {
    const [data, setData] = useState<PageData>({ items: [], next_cursor: null }), [busy, setBusy] = useState(true), [error, setError] = useState(''), [retry, setRetry] = useState(0);
    const refresh = useMarketRefresh();
    const loadedRequest = useRef('');
    const watch = useWatchlist();
    const watchlistRevision = state.category === 'watchlist' ? JSON.stringify(watch?.keys) : '';
    const limit = 20;
    const requestKey = new URLSearchParams({ market_id: market, q: query, category: state.category, sort: state.sort, direction: state.direction, limit: String(limit), ...(state.cursors[state.page] ? { cursor: state.cursors[state.page] } : {}) }).toString();
    const forecasts = useMarketForecasts(market, busy ? [] : data.items.map(row => row.item_key), refresh);
    useEffect(() => {
        let active = true;
        const background = loadedRequest.current === requestKey;
        setBusy(!background); setError('');
        if (!background) {
            loadedRequest.current = '';
            setData({ items: [], next_cursor: null });
        }
        if (!market) { setBusy(false); return; }
        const timer = setTimeout(() => {
            marketPage(requestKey, watch?.keys ?? []).then(d => {
                if (!active) return;
                loadedRequest.current = requestKey;
                setData(d);
                if (!background && state.scroll > 0)
                    requestAnimationFrame(() => window.scrollTo(0, state.scroll));
            }).catch(e => { if (active) setError(e.message); })
              .finally(() => { if (active) setBusy(false); });
        }, 200);
        return () => { active = false; clearTimeout(timer); };
    }, [market, requestKey, retry, refresh, watchlistRevision]);
    function sort(key: string) { update({ ...state, sort: key, direction: state.sort === key ? (state.direction === 'asc' ? 'desc' : 'asc') : ['name', 'current', 'age', 'discount'].includes(key) ? 'asc' : 'desc', page: 0, cursors: [''] }); }
    function heading(label: React.ReactNode, key: string, cls = 'numeric') { return <TableHead className={cls} scope="col" aria-sort={state.sort === key ? (state.direction === 'asc' ? 'ascending' : 'descending') : undefined}><Button title={key === 'score' ? 'Opportunity score: price deviation, historical coverage, freshness and observed quantity' : key === 'discount' ? 'Current minimum below each median: 7 days first, then last scan. Click to sort by the 7-day comparison.' : key === 'reference_profit' ? 'Scenario margin if resold at the 7-day median after the 5% auction cut.' : undefined} onClick={() => sort(key)}>{label}{state.sort === key && (state.direction === 'asc' ? <ArrowUp size={12}/> : <ArrowDown size={12}/>)}</Button></TableHead>; }
    const reset = () => { resetQuery(); update(initialList()); };
    return <section aria-label="Market"><h1 className="market-title">Market</h1><ResultsToolbar><Label>Order by<Select value={state.sort} onChange={e => update({ ...state, sort: e.target.value, direction: ['name', 'current', 'age', 'discount'].includes(e.target.value) ? 'asc' : 'desc', page: 0, cursors: [''] })}><option value="reference_profit">Margin at 7d median</option><option value="discount">Below median (7 days)</option><option value="score">Opportunity score</option><option value="name">Item name</option><option value="current">Current price</option><option value="quantity">Listed quantity</option><option value="age">Last seen</option></Select></Label><Button onClick={() => update({ ...state, direction: state.direction === 'asc' ? 'desc' : 'asc', page: 0, cursors: [''] })}>{state.direction === 'asc' ? 'Ascending' : 'Descending'}</Button><ForecastExportButton menu market={market} keys={forecasts.rows.filter(f => !f.demo && forecastFresh(f, Date.now()) && f.rolling.price !== null && ['baseline', 'experimental'].includes(f.rolling.status)).map(f => f.item_key)} disabled={busy || forecasts.loading || !!forecasts.error} label="Download visible forecasts for addon"/></ResultsToolbar><div className="category-filters" aria-label="Item category">{categories.map(c => <Button className={`filter-chip ${state.category === c ? 'active' : ''}`} aria-pressed={state.category === c} key={c} onClick={() => update({ ...state, category: c, page: 0, cursors: [''] })}>{c === 'watchlist' ? 'Watch list' : c[0].toUpperCase() + c.slice(1)}</Button>)}</div><div className="table-panel market-results"><Table className="market-table"><TableHeader><TableRow>{heading('#', 'score', 'column-rank')}{heading('Item', 'name', 'column-item')}{heading('Current minimum', 'current', 'numeric column-current')}<TableHead className="numeric column-reference" ><Help label="First: median of daily medians over seven completed days (Europe/Paris). Second: median asking price in the last scan, used as the starting price for forecasts."><span>Median<br/><small>(7 days/last scan)</small></span></Help></TableHead>{heading(<span>Below median<br/><small>(7 days/last scan)</small></span>, 'discount', 'numeric column-discount')}{heading(<span>Margin / unit<br/><small>(7 days)</small></span>, 'reference_profit')}{heading('Quantity', 'quantity', 'numeric column-quantity')}{heading('Last seen', 'age', 'numeric column-seen')}<TableHead className="column-trend" title="Observed median prices over the last 24 hours">24h trend</TableHead><TableHead className="numeric" title="Estimated median price change over the next 24 hours, relative to the current median">24h forecast</TableHead><TableHead className="numeric" title="Estimated median asking prices in 12 and 24 hours">Forecasted trend · 12h / 24h</TableHead></TableRow></TableHeader><TableBody>{busy ? <LoadingRows columns={11}/> : data.items.map((item, i) => <TableRow key={item.item_key} onClick={() => onSelect(item.item_key)}><TableCell className="column-rank">{state.page * limit + i + 1}</TableCell><TableCell className="column-item"><ItemTooltip name={item.name} itemId={item.item_id} itemKey={item.item_key} category={item.category} rarity={item.rarity}><div className="item-cell" tabIndex={0}><ItemIcon name={item.name} icon={item.icon}/><span className="item-name"><a data-item-name className="item-link" href={routeUrl(`/items/${encodeURIComponent(item.item_key)}`)} onClick={e => { e.stopPropagation(); if(e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return; e.preventDefault(); onSelect(item.item_key); }}>{item.name}</a><span className="mobile-item-age"><FreshnessIndicator seconds={item.freshness_seconds} stale={item.freshness_status === "stale"} observedAt={item.observed_at}/></span></span></div></ItemTooltip></TableCell><TableCell className="numeric column-current"><WowMoney value={item.min_buyout}/></TableCell><MarketMedianCells item={item}/><TableCell className="numeric market-metric" title="If one unit sells at the 7-day median: sale price after the 5% Horde auction cut minus the current minimum. Deposits and failed sales excluded."><WowMoney value={item.reference_profit} /></TableCell><TableCell className="numeric column-quantity">{item.total_units?.toLocaleString('en-GB') ?? '—'}</TableCell><TableCell className="numeric column-seen"><FreshnessIndicator seconds={item.freshness_seconds} stale={item.freshness_status === 'stale'} observedAt={item.observed_at}/></TableCell><TableCell className="column-trend"><PriceSparkline points={item.trend ?? []}/></TableCell><MarketForecastCells part="forecast" forecast={forecasts.rows.find(row => row.item_key === item.item_key)} loading={forecasts.loading} error={forecasts.error}/><MarketForecastCells part="prices" forecast={forecasts.rows.find(row => row.item_key === item.item_key)} loading={forecasts.loading} error={forecasts.error}/></TableRow>)}</TableBody></Table>{busy && <span className="sr-only" role="status">Loading items…</span>}{error ? <Alert className="error-state" role="alert">{error}<Button onClick={() => setRetry(n => n + 1)}>Retry</Button></Alert> : !busy && !data.items.length && <Empty className="empty-state"><h2>No items match these filters.</h2><p>{market ? 'Try another category or search.' : 'Waiting for an accepted scan from Normal · Horde.'}</p><Button onClick={reset}>Reset filters</Button></Empty>}</div><div className="table-footer"><span>{data.items.length ? `Showing ${state.page * limit + 1}–${state.page * limit + data.items.length}${data.total_count != null ? ` of ${data.total_count}` : ''} items` : 'No items'}<span className="table-note"> · Observed prices</span></span><Pager><Button aria-label="Previous page" disabled={busy || state.page === 0} onClick={() => update({ ...state, page: state.page - 1 })}><ChevronLeft size={16}/></Button><span aria-current="page">{state.page + 1}</span><Button aria-label="Next page" disabled={busy || !data.next_cursor} onClick={() => update({ ...state, page: state.page + 1, cursors: [...state.cursors.slice(0, state.page + 1), data.next_cursor!] })}><ChevronRight size={16}/></Button></Pager></div>{data.items.some(item => item.catalog_attribution) && <p className="catalog-credit">Icons: Blizzard Entertainment. Item mapping: <a href="https://www.wowhead.com/classic" target="_blank" rel="noreferrer">Wowhead Classic</a>.</p>}<Details className="quiet-details"><summary>About these numbers</summary><p>Margin assumes resale at the 7-day median after the 5% auction cut. Sales are not guaranteed. Historical rises describe past observations, not future probabilities.</p><p>{forecastInstallNote}</p></Details></section>;
}
type Row = Record<string, any>;
function Settings({ member, markets, market, setMarket, logout }: {
    member: Member;
    markets: Market[];
    market: string;
    setMarket: (id: string) => void;
    logout: () => void;
}) {
    const selectedMarket = markets.find(m => m.id === market);
    const confirm = useConfirm();
    const [devices, setDevices] = useState<Row[]>([]), [members, setMembers] = useState<Row[]>([]), [sources, setSources] = useState<Row[]>([]), [code, setCode] = useState<Row | null>(null), [error, setError] = useState(''), [busy, setBusy] = useState(false);
    async function load() { try {
        setDevices(await api<Row[]>('/devices'));
        if (member.role === 'admin') {
            const [m, s] = await Promise.all([api<Row[]>('/admin/members'), api<Row[]>('/admin/sources')]);
            setMembers(m);
            setSources(s);
        }
    }
    catch (e) {
        setError((e as Error).message);
    } }
    useEffect(() => { void load(); }, []);
    async function action(path: string, method: string, body?: unknown) { setBusy(true); setError(''); try {
        const result = await api<Row | undefined>(path, method, body);
        if (result?.code)
            setCode(result);
        await load();
        return true;
    }
    catch (e) {
        setError((e as Error).message);
        return false;
    }
    finally {
        setBusy(false);
    } }
    return <div className="settings-page"><h1>Settings</h1><ApplicationSettings market={markets.find(m => m.id === market)}/>{error && <Alert role="alert" className="error-state">{error}<Button onClick={() => void load()}>Retry</Button></Alert>}<section className="settings-section"><h2>Market & account</h2><div className="settings-row"><span>Realm and auction house</span><strong>{selectedMarket ? `${selectedMarket.realm} · ${selectedMarket.house}` : 'No market available'}</strong></div><div className="settings-row"><span>{member.display_name}<small>{member.role} · Display timezone: Europe/Paris</small></span><Button disabled={busy} onClick={async () => { if (await action('/auth/logout', 'POST', {}))
        logout(); }}>Sign out</Button></div></section><Details className="settings-section"><summary>Collectors</summary><p>Pair the collector, select your WTF folder, then save an AHDB scan with /reload or logout.</p><Button className="primary" disabled={busy} onClick={() => void action('/devices/pair-code', 'POST', {})}>Create pairing code</Button>{code && <div className="code-box" role="status"><strong>{code.code}</strong><span>Expires {time(code.expires_at)}. Keep this code private.</span><Button onClick={() => setCode(null)}>Hide code</Button></div>}{devices.length ? devices.map(d => <div className="settings-row" key={d.id}><div><strong>{d.display_name ?? d.name ?? d.id}</strong><small>{d.revoked_at ? 'Revoked' : d.status ?? 'Paired'} · Last seen {time(d.last_seen_at)}</small><small>Last accepted upload: {time(d.last_accepted_at)} · Queued: {d.health?.queue_count ?? 'unknown'}</small>{d.last_error && <small className="negative">{d.last_error}</small>}</div>{!d.revoked_at && <Button disabled={busy} onClick={async () => { if (await confirm('Revoke this collector? It will need a new pairing code.'))
        void action(`/devices/${encodeURIComponent(d.id)}`, 'DELETE'); }}>Revoke</Button>}</div>) : <p className="muted">No collectors paired.</p>}<Details><summary>Load shared prices into AHDB</summary><p>Enable AHDB Record full scans. Close every WoW client before you sync. Use the same collector data directory as pairing.</p><pre>market-collector sync --realm "Normal" --house Horde --account "ACCOUNT_NAME" --dry-run</pre><p>Use your actual realm, auction house, and account directory. Shared-history writes remain disabled. Local recovery and restore are available in Settings without a server connection.</p></Details></Details>{member.role === 'admin' && <Details className="settings-section"><summary>Workspace administration</summary><h2>Members</h2><Button disabled={busy} onClick={() => void action('/admin/invites', 'POST', { role: 'member' })}>Invite a member</Button>{code && <div className="code-box" role="status"><strong>{code.code}</strong><span>Expires {time(code.expires_at)}. Keep this code private.</span><Button onClick={() => setCode(null)}>Hide code</Button></div>}{members.map(m => <div className="settings-row" key={m.id}><span><strong>{m.display_name}</strong><small>{m.role} · {m.status}</small></span><Button disabled={busy || m.id === member.id} onClick={async () => { if (m.status !== 'active' || await confirm('Revoke this member access?')) void action(`/admin/members/${encodeURIComponent(m.id)}`, 'PATCH', { status: m.status === 'active' ? 'disabled' : 'active' }); }}>{m.status === 'active' ? 'Revoke access' : 'Restore access'}</Button></div>)}<h2>Source health</h2>{sources.length ? sources.map((s, i) => <div className="settings-row" key={s.id ?? i}><span>{s.display_name ?? s.name ?? s.id}<small>{s.pending ?? 0} uploads pending · {time(s.last_seen_at)}{s.error_code ? ` · ${s.error_code}` : ''}</small></span></div>) : <p className="muted">No source reports yet.</p>}</Details>}</div>;
}
function Join({ onLogin, error }: { onLogin: () => Promise<void>; error: string }) {
    const [admin, setAdmin] = useState(false), [busy, setBusy] = useState(false), [failure, setFailure] = useState(error);
    if (admin) return <><AdminLogin onLogin={onLogin} error={error}/><div className="admin-back"><Button className="text-button" onClick={() => setAdmin(false)}>Back to join</Button></div></>;
    const messages: Record<string, string> = { invalid_invitation_code: 'The invitation code is incorrect.', invalid_nickname: 'Use 2–32 letters, numbers, dots, hyphens or underscores.', nickname_taken: 'This nickname belongs to a password account. Use the account login or contact the administrator.', member_disabled: 'This account is disabled. Contact the administrator.', rate_limited: 'Too many attempts. Please wait a minute.' };
    return <div className="auth"><Card className="login-card"><div className="brand"><span className="brand-bars" aria-hidden="true"><i/><i/><i/></span>Nauctions</div><h1>Join your friends</h1><p>Enter your nickname and invitation code. Use your existing nickname to reconnect. This computer will remember you.</p>{isDesktop() && <p>After sign-in, Nauctions searches for WoW, installs the addon, and connects your collector automatically.</p>}
        <form onSubmit={async e => { e.preventDefault(); setBusy(true); setFailure(''); const form = new FormData(e.currentTarget); try { await api('/auth/join', 'POST', { nickname: form.get('nickname'), code: form.get('code') }); await onLogin(); } catch (e) { const message = (e as Error).message; setFailure(messages[message] ?? message); } finally { setBusy(false); } }}>
            <Label>Nickname<Input name="nickname" autoComplete="nickname" minLength={2} maxLength={32} required disabled={busy}/></Label>
            <Label>Invitation code<Input name="code" autoComplete="off" maxLength={64} required disabled={busy}/></Label>
            {failure && <Alert className="error-state" role="alert">{failure}</Alert>}<Button className="primary" disabled={busy}>{busy ? 'Joining…' : 'Join'}</Button>
        </form><Details className="admin-access"><summary>Administration</summary><Button className="text-button" onClick={() => setAdmin(true)}>Administrator access</Button></Details>
    </Card></div>;
}
function AdminLogin({ onLogin, error }: {
    onLogin: () => Promise<void>;
    error: string;
}) { const [invite, setInvite] = useState(new URLSearchParams(location.search).has('invite')), [busy, setBusy] = useState(false), [failure, setFailure] = useState(error); return <div className="auth"><Card className="login-card"><div className="brand"><span className="brand-bars" aria-hidden="true"><i /><i /><i /></span>Nauctions</div><h1>{invite ? 'Join your workspace' : 'Sign in'}</h1><p>Search your market. Find your next opportunity.</p><form onSubmit={async (e) => { e.preventDefault(); setBusy(true); setFailure(''); const form = new FormData(e.currentTarget); try {
    await api(invite ? '/auth/accept-invite' : '/auth/login', 'POST', Object.fromEntries(form));
    if (invite)
        await api('/auth/login', 'POST', { login: form.get('login'), password: form.get('password') });
    await onLogin();
}
catch (e) {
    setFailure((e as Error).message);
}
finally {
    setBusy(false);
} }}>{invite && <><Label>Invitation code<Input name="code" required defaultValue={new URLSearchParams(location.search).get('invite') ?? ''}/></Label><Label>Display name<Input name="display_name" required autoComplete="nickname"/></Label></>}<Label>Login<Input name="login" autoComplete="username" required/></Label><Label>Password<Input name="password" type="password" minLength={invite ? 12 : 1} required autoComplete={invite ? 'new-password' : 'current-password'}/></Label>{failure && <Alert role="alert" className="error-state">{failure}</Alert>}<Button className="primary" disabled={busy}>{busy ? 'Please wait…' : invite ? 'Accept invitation' : 'Sign in'}</Button></form><Button className="text-button" onClick={() => setInvite(!invite)}>{invite ? 'Back to sign in' : 'Have an invitation code?'}</Button></Card></div>; }
async function start() {
    if (isDesktop()) {
        try { setTelemetryEnabled(await desktopInvoke<boolean>('observability_preference')); } catch { setTelemetryEnabled(false); }
    }
    initTelemetry();
    createRoot(document.getElementById('root')!).render(<React.StrictMode><RecoveryBoundary><StartupGate><TooltipProvider><ConfirmProvider><DesktopRoot><App /><Toaster/></DesktopRoot></ConfirmProvider></TooltipProvider></StartupGate></RecoveryBoundary></React.StrictMode>);
}
void start();

import { MarketClock, useMarketRefresh } from './marketClock';
import React, { useEffect, useRef, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { Search, Settings as SettingsIcon, ChevronLeft, ChevronRight, ArrowDown, ArrowUp, X } from 'lucide-react';
import { api, ApiError, setCsrf, time, type Item, type Member, type Market } from './api';
import { ItemIcon, WowMoney, DiscountBadge, FreshnessIndicator, PriceSparkline } from './market-ui';
import { Detail } from './Detail';
import './style.css';
import { Strategy } from './Strategy';
import { Crafting } from './Crafting';
import { Forecasts } from './Forecasts';
import { DesktopRoot, DesktopCollector } from './DesktopShell';
import { currentRoute, routeUrl, desktopInvoke, isDesktop } from './desktop';
import { initTelemetry, setTelemetryEnabled, trackScreen } from './telemetry';
import { Diagnostics, RecoveryBoundary } from './Diagnostics';
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
const initialList = (opportunities: boolean): ListState => ({ category: 'all', sort: opportunities ? 'score' : 'name', direction: opportunities ? 'desc' : 'asc', cursors: [''], page: 0, scroll: 0 });
const categories = ['all', 'materials', 'consumables', 'recipes', 'equipment', 'other'];
function App() {
    const [member, setMember] = useState<Member | null>(null), [boot, setBoot] = useState(true), [markets, setMarkets] = useState<Market[]>([]), [market, setMarket] = useState(''), [route, setRoute] = useState(currentRoute()), [query, setQuery] = useState(''), [error, setError] = useState('');
    const [lists, setLists] = useState({ opportunities: initialList(true), items: initialList(false) });
    const previous = useRef('/opportunities');
    useEffect(() => { trackScreen(route); }, [route]);
    async function session() { try {
        const me = await api<Member>('/me');
        setCsrf(me.csrf_token);
        setMember(me);
        const m = await api<Market[]>('/markets');
        setMarkets(m);
        setMarket(current => m.some(x => x.id === current) ? current : m[0]?.id ?? '');
    }
    catch (e) {
        if (!(e instanceof ApiError && e.status === 401))
            setError((e as Error).message);
    }
    finally {
        setBoot(false);
    } }
    useEffect(() => { void session(); const pop = () => setRoute(currentRoute()); window.addEventListener('popstate', pop); if (currentRoute() === '/') {
        history.replaceState(null, '', routeUrl('/opportunities') + location.search);
        setRoute('/opportunities');
    } return () => window.removeEventListener('popstate', pop); }, []);
    function navigate(path: string) { if (route === '/strategy' || route === '/crafting') previous.current = route; if (route === '/opportunities' || route === '/items') {
        previous.current = route;
        const key = route === '/items' ? 'items' : 'opportunities';
        setLists(s => ({ ...s, [key]: { ...s[key], scroll: window.scrollY } }));
    } history.pushState(null, '', routeUrl(path)); setRoute(path); window.scrollTo(0, 0); }
    const selectedMarket = markets.find(m => m.id === market);
    const detail = route.startsWith('/items/'), listKey = route === '/items' ? 'items' : 'opportunities';
    useEffect(() => { if (route === '/opportunities' || route === '/items') {
        const y = lists[route === '/items' ? 'items' : 'opportunities'].scroll;
        requestAnimationFrame(() => window.scrollTo(0, y));
    } }, [route]);
    if (boot)
        return <div className="auth" role="status">Loading Nauctions…</div>;
    if (!member)
        return <><Join onLogin={session} error={error}/>{error && <button onClick={() => { setError(''); setBoot(true); void session(); }}>Retry connection</button>}</>;
    return <><header className="app-header"><div className="header-inner"><a href="/opportunities" className="brand" onClick={e => { e.preventDefault(); navigate('/opportunities'); }}><span className="brand-bars" aria-hidden="true"><i /><i /><i /></span>Nauctions</a><nav className="nav-tabs" aria-label="Main navigation">{['opportunities', 'strategy', 'crafting', 'forecasts', 'items'].map(p => <a key={p} href={`/${p}`} aria-current={route === `/${p}` || (p === 'items' && detail) ? 'page' : undefined} onClick={e => { e.preventDefault(); navigate(`/${p}`); }}>{p === 'items' ? 'Items' : p === 'strategy' ? 'Strategy' : p === 'crafting' ? 'Crafting' : p === 'forecasts' ? 'Forecasts' : 'Opportunities'}</a>)}</nav><a className="settings-button" href="/settings" aria-label="Settings" aria-current={route === '/settings' ? 'page' : undefined} onClick={e => { e.preventDefault(); navigate('/settings'); }}><SettingsIcon size={19}/></a></div></header><main className="app-main"><MarketClock asOf={selectedMarket?.demo ? selectedMarket.as_of : null}>{selectedMarket?.demo && <p className="demo-notice" role="note"><strong>Simulated market</strong><span>50 items · 63 days of test scenarios{selectedMarket.as_of ? ` · Replay: ${time(selectedMarket.as_of)}` : ""}</span></p>}{!['/crafting', '/forecasts'].includes(route) && <GlobalItemSearch market={market} query={query} setQuery={q => { setQuery(q); setLists(s => ({ opportunities: { ...s.opportunities, page: 0, cursors: [''] }, items: { ...s.items, page: 0, cursors: [''] } })); }} onSelect={key => navigate(`/items/${encodeURIComponent(key)}`)}/>}{error && <p className="error-state" role="alert">{error}<button onClick={() => { setError(''); void session(); }}>Retry connection</button><button aria-label="Dismiss error" onClick={() => setError('')}><X size={16}/></button></p>}{detail ? <Detail itemKey={decodeURIComponent(route.slice(7))} market={market} back={() => navigate(previous.current)}/> : route === '/settings' ? <Settings member={member} markets={markets} market={market} setMarket={id => { setMarket(id); setLists({ opportunities: initialList(true), items: initialList(false) }); }} logout={() => setMember(null)}/> : route === '/forecasts' ? <Forecasts market={selectedMarket} admin={member.role === 'admin'}/> : route === '/crafting' ? <Crafting market={market} neutral={selectedMarket?.house === 'Neutral'} onItem={key => navigate('/items/' + encodeURIComponent(key))}/> : route === '/strategy' ? <Strategy market={market} query={query} onItem={key => navigate('/items/' + encodeURIComponent(key))}/> : <MarketTable key={listKey} opportunities={listKey === 'opportunities'} market={market} query={query} state={lists[listKey]} update={s => setLists(old => ({ ...old, [listKey]: s }))} onSelect={key => navigate(`/items/${encodeURIComponent(key)}`)} resetQuery={() => setQuery('')}/>}</MarketClock></main></>;
}
function GlobalItemSearch({ market, query, setQuery, onSelect }: {
    market: string;
    query: string;
    setQuery: (q: string) => void;
    onSelect: (key: string) => void;
}) {
    const input = useRef<HTMLInputElement>(null), [open, setOpen] = useState(false), [rows, setRows] = useState<Item[]>([]), [selected, setSelected] = useState(0), [busy, setBusy] = useState(false), [error, setError] = useState('');
    useEffect(() => { const key = (e: KeyboardEvent) => { const el = e.target as HTMLElement; if (e.key === '/' && !el.closest('input,textarea,select,[contenteditable="true"]')) {
        e.preventDefault();
        input.current?.focus();
    } }; window.addEventListener('keydown', key); return () => window.removeEventListener('keydown', key); }, []);
    useEffect(() => { let active = true; setRows([]); setSelected(0); setError(''); setBusy(!!query.trim() && !!market); if (!query.trim() || !market)
        return; const timer = setTimeout(() => api<PageData>(`/items?${new URLSearchParams({ market_id: market, q: query, limit: '12', sort: 'name', direction: 'asc' })}`).then(data => { if (active)
        setRows(data.items); }).catch(e => { if (active)
        setError(e.message); }).finally(() => { if (active)
        setBusy(false); }), 200); return () => { active = false; clearTimeout(timer); }; }, [query, market]);
    useEffect(() => { if (open)
        document.getElementById(`search-item-${selected}`)?.scrollIntoView({ block: 'nearest' }); }, [selected, open]);
    function select(key: string) { setOpen(false); onSelect(key); input.current?.blur(); }
    return <div className="global-search" onBlur={e => { if (!e.currentTarget.contains(e.relatedTarget))
        setOpen(false); }}><div className="search-field"><Search size={17}/><input ref={input} role="combobox" aria-label="Search for an item" aria-autocomplete="list" aria-controls="item-search-results" aria-expanded={open && !!query.trim()} aria-activedescendant={open && rows.length ? `search-item-${selected}` : undefined} placeholder="Search for an item..." value={query} onFocus={() => setOpen(true)} onChange={e => { setQuery(e.target.value); setOpen(true); }} onKeyDown={e => { if (e.key === 'Escape') {
        if (open)
            setOpen(false);
        else
            setQuery('');
    } if (e.key === 'ArrowDown') {
        e.preventDefault();
        setOpen(true);
        setSelected(n => Math.min(n + 1, rows.length - 1));
    } if (e.key === 'ArrowUp') {
        e.preventDefault();
        setSelected(n => Math.max(0, n - 1));
    } if (e.key === 'Enter' && open && rows[selected]) {
        e.preventDefault();
        select(rows[selected].item_key);
    } }}/>{query ? <button aria-label="Clear search" onClick={() => { setQuery(''); input.current?.focus(); }}><X size={15}/></button> : <kbd>/</kbd>}</div>{open && !!query.trim() && <div className="search-dropdown" id="item-search-results" role="listbox" aria-label="Matching items">{busy ? <p role="status">Searching…</p> : error ? <p role="alert">{error}</p> : !rows.length ? <p role="status">No items found.</p> : rows.map((item, i) => <div id={`search-item-${i}`} role="option" aria-selected={selected === i} className={`search-result ${selected === i ? 'selected' : ''}`} key={item.item_key} onMouseDown={e => e.preventDefault()} onMouseEnter={() => setSelected(i)} onClick={() => select(item.item_key)}><ItemIcon name={item.name} icon={item.icon}/><span>{item.name}</span><small>{item.category ?? 'Unclassified'}</small></div>)}{rows.some(item => item.catalog_attribution) && <p className="catalog-credit">Icons: Blizzard. Mapping: <a href="https://www.wowhead.com/classic" target="_blank" rel="noreferrer">Wowhead Classic</a>.</p>}</div>}</div>;
}
function MarketTable({ market, opportunities, query, state, update, onSelect, resetQuery }: {
    market: string;
    opportunities: boolean;
    query: string;
    state: ListState;
    update: (s: ListState) => void;
    onSelect: (k: string) => void;
    resetQuery: () => void;
}) {
    const [data, setData] = useState<PageData>({ items: [], next_cursor: null }), [busy, setBusy] = useState(true), [error, setError] = useState(''), [retry, setRetry] = useState(0);
    const refresh = useMarketRefresh();
    const limit = 20;
    useEffect(() => { let active = true; setBusy(true); setError(''); if (!market) {
        setData({ items: [], next_cursor: null });
        setBusy(false);
        return;
    } const timer = setTimeout(() => { const params = new URLSearchParams({ market_id: market, q: query, category: state.category, sort: state.sort, direction: state.direction, limit: String(limit), ...(state.cursors[state.page] ? { cursor: state.cursors[state.page] } : {}) }); api<PageData>(`/${opportunities ? 'opportunities' : 'items'}?${params}`).then(d => { if (active) {
        setData(d);
        if (state.scroll > 0)
            requestAnimationFrame(() => window.scrollTo(0, state.scroll));
    } }).catch(e => { if (active)
        setError(e.message); }).finally(() => { if (active)
        setBusy(false); }); }, 200); return () => { active = false; clearTimeout(timer); }; }, [market, opportunities, query, state.category, state.sort, state.direction, state.page, state.cursors, retry, refresh]);
    function sort(key: string) { update({ ...state, sort: key, direction: state.sort === key ? (state.direction === 'asc' ? 'desc' : 'asc') : ['name', 'current', 'age', 'discount'].includes(key) ? 'asc' : 'desc', page: 0, cursors: [''] }); }
    function heading(label: string, key: string, cls = 'numeric') { return <th className={cls} scope="col" aria-sort={state.sort === key ? (state.direction === 'asc' ? 'ascending' : 'descending') : undefined}><button title={key === 'score' ? 'Opportunity score: price deviation, historical coverage, freshness and observed quantity' : undefined} onClick={() => sort(key)}>{label}{state.sort === key && (state.direction === 'asc' ? <ArrowUp size={12}/> : <ArrowDown size={12}/>)}</button></th>; }
    const reset = () => { resetQuery(); update(initialList(opportunities)); };
    return <section aria-label={opportunities ? 'Market opportunities' : 'All items'}><div className="category-filters" aria-label="Item category">{categories.map(c => <button className={`filter-chip ${state.category === c ? 'active' : ''}`} aria-pressed={state.category === c} key={c} onClick={() => update({ ...state, category: c, page: 0, cursors: [''] })}>{c[0].toUpperCase() + c.slice(1)}</button>)}</div><div className="table-panel"><table className="market-table"><thead><tr>{opportunities && heading('#', 'score', 'column-rank')}{heading('Item', 'name', 'column-item')}{heading(opportunities ? 'Current' : 'Latest minimum', 'current', 'numeric column-current')}{!opportunities && <th className="numeric column-median">Latest median</th>}<th className="numeric column-reference" title="Median of daily medians over seven completed days (Europe/Paris)">7d reference</th>{opportunities ? heading('Discount', 'discount', 'numeric column-discount') : <th className="numeric column-change">24h change</th>}{heading('Quantity', 'quantity', 'numeric column-quantity')}{heading('Last seen', 'age', 'numeric column-seen')}{opportunities && <th className="column-trend">Trend</th>}</tr></thead><tbody>{busy ? Array.from({ length: 10 }, (_, i) => <tr key={i} className="skeleton-row" aria-hidden="true">{Array.from({ length: opportunities ? 8 : 7 }, (_, j) => <td key={j}><span className="skeleton"/></td>)}</tr>) : !error && data.items.map((item, i) => <tr key={item.item_key} onClick={() => onSelect(item.item_key)}>{opportunities && <td className="column-rank">{state.page * limit + i + 1}</td>}<td className="column-item"><div className="item-cell"><ItemIcon name={item.name} icon={item.icon}/><span className="item-name"><a className="item-link" href={`/items/${encodeURIComponent(item.item_key)}`} onClick={e => { e.preventDefault(); e.stopPropagation(); onSelect(item.item_key); }}>{item.name}</a><span className="mobile-item-age"><FreshnessIndicator seconds={item.freshness_seconds} stale={item.freshness_status === "stale"} observedAt={item.observed_at}/></span></span></div></td><td className="numeric column-current"><WowMoney value={item.min_buyout}/></td>{!opportunities && <td className="numeric column-median"><WowMoney value={item.median_buyout}/></td>}<td className="numeric column-reference"><WowMoney value={item.baseline.price}/></td><td className={`numeric ${opportunities ? 'column-discount' : 'column-change'}`}><DiscountBadge value={opportunities ? item.discount_percentage : item.change_24h_percentage} comparison={opportunities ? "7-day reference" : "median observed 24 hours earlier"}/></td><td className="numeric column-quantity">{item.total_units?.toLocaleString('en-GB') ?? '—'}</td><td className="numeric column-seen"><FreshnessIndicator seconds={item.freshness_seconds} stale={item.freshness_status === 'stale'} observedAt={item.observed_at}/></td>{opportunities && <td className="column-trend"><PriceSparkline points={item.trend ?? []}/></td>}</tr>)}</tbody></table>{busy && <span className="sr-only" role="status">Loading items…</span>}{error ? <div className="error-state" role="alert">{error}<button onClick={() => setRetry(n => n + 1)}>Retry</button></div> : !busy && !data.items.length && <div className="empty-state"><h2>{opportunities ? 'No opportunities match these filters.' : 'No items match these filters.'}</h2><p>{market ? 'Try another category or search.' : 'Select a market in Settings after a scan is accepted.'}</p><button onClick={reset}>Reset filters</button></div>}</div><div className="table-footer"><span>{data.items.length ? `Showing ${state.page * limit + 1}–${state.page * limit + data.items.length}${data.total_count != null ? ` of ${data.total_count}` : ''} items` : 'No items'}<span className="table-note"> · Observed prices</span></span><div className="pagination"><button aria-label="Previous page" disabled={busy || state.page === 0} onClick={() => update({ ...state, page: state.page - 1 })}><ChevronLeft size={16}/></button><span aria-current="page">{state.page + 1}</span><button aria-label="Next page" disabled={busy || !data.next_cursor} onClick={() => update({ ...state, page: state.page + 1, cursors: [...state.cursors.slice(0, state.page + 1), data.next_cursor!] })}><ChevronRight size={16}/></button></div></div>{data.items.some(item => item.catalog_attribution) && <p className="catalog-credit">Icons: Blizzard Entertainment. Item mapping: <a href="https://www.wowhead.com/classic" target="_blank" rel="noreferrer">Wowhead Classic</a>.</p>}</section>;
}
type Row = Record<string, any>;
function Settings({ member, markets, market, setMarket, logout }: {
    member: Member;
    markets: Market[];
    market: string;
    setMarket: (id: string) => void;
    logout: () => void;
}) {
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
    return <div className="settings-page"><h1>Settings</h1><DesktopCollector market={markets.find(m => m.id === market)}/>{error && <p role="alert" className="error-state">{error}<button onClick={() => void load()}>Retry</button></p>}<section className="settings-section"><h2>Market & account</h2><div className="settings-row"><label htmlFor="market">Realm and auction house</label><select id="market" value={market} onChange={e => setMarket(e.target.value)}>{markets.length ? markets.map(m => <option key={m.id} value={m.id}>{m.realm} · {m.house}</option>) : <option value="">No market data yet</option>}</select></div><div className="settings-row"><span>{member.display_name}<small>{member.role} · Display timezone: Europe/Paris</small></span><button disabled={busy} onClick={async () => { if (await action('/auth/logout', 'POST', {}))
        logout(); }}>Sign out</button></div></section><details className="settings-section"><summary>Collectors</summary><p>Pair the collector, select your WTF folder, then save an AHDB scan with /reload or logout.</p><button className="primary" disabled={busy} onClick={() => void action('/devices/pair-code', 'POST', {})}>Create pairing code</button>{code && <div className="code-box" role="status"><strong>{code.code}</strong><span>Expires {time(code.expires_at)}. Keep this code private.</span><button onClick={() => setCode(null)}>Hide code</button></div>}{devices.length ? devices.map(d => <div className="settings-row" key={d.id}><div><strong>{d.display_name ?? d.name ?? d.id}</strong><small>{d.revoked_at ? 'Revoked' : d.status ?? 'Paired'} · Last seen {time(d.last_seen_at)}</small><small>Last accepted upload: {time(d.last_accepted_at)} · Queued: {d.health?.queue_count ?? 'unknown'}</small>{d.last_error && <small className="negative">{d.last_error}</small>}</div>{!d.revoked_at && <button disabled={busy} onClick={() => { if (confirm('Revoke this collector? It will need a new pairing code.'))
        void action(`/devices/${encodeURIComponent(d.id)}`, 'DELETE'); }}>Revoke</button>}</div>) : <p className="muted">No collectors paired.</p>}<details><summary>Load shared prices into AHDB</summary><p>Enable AHDB Record full scans. Close every WoW client before you sync. Use the same collector data directory as pairing.</p><pre>market-collector sync --realm "YOUR REALM" --house Alliance --account "ACCOUNT_NAME" --dry-run</pre><p>Use your actual realm, auction house, and account directory. Shared-history writes remain disabled. Local recovery and restore are available in the desktop connection area without a server connection.</p></details></details>{member.role === 'admin' && <details className="settings-section"><summary>Workspace administration</summary><h2>Members</h2><button disabled={busy} onClick={() => void action('/admin/invites', 'POST', { role: 'member' })}>Invite a member</button>{code && <div className="code-box" role="status"><strong>{code.code}</strong><span>Expires {time(code.expires_at)}. Keep this code private.</span><button onClick={() => setCode(null)}>Hide code</button></div>}{members.map(m => <div className="settings-row" key={m.id}><span><strong>{m.display_name}</strong><small>{m.role} · {m.status}</small></span><button disabled={busy || m.id === member.id} onClick={() => void action(`/admin/members/${encodeURIComponent(m.id)}`, 'PATCH', { status: m.status === 'active' ? 'disabled' : 'active' })}>{m.status === 'active' ? 'Revoke access' : 'Restore access'}</button></div>)}<h2>Source health</h2>{sources.length ? sources.map((s, i) => <div className="settings-row" key={s.id ?? i}><span>{s.display_name ?? s.name ?? s.id}<small>{s.pending ?? 0} uploads pending · {time(s.last_seen_at)}{s.error_code ? ` · ${s.error_code}` : ''}</small></span></div>) : <p className="muted">No source reports yet.</p>}</details>}</div>;
}
function Join({ onLogin, error }: { onLogin: () => Promise<void>; error: string }) {
    const [admin, setAdmin] = useState(false), [busy, setBusy] = useState(false), [failure, setFailure] = useState(error);
    if (admin) return <><AdminLogin onLogin={onLogin} error={error}/><div className="admin-back"><button className="text-button" onClick={() => setAdmin(false)}>Back to join</button></div></>;
    const messages: Record<string, string> = { invalid_invitation_code: 'The invitation code is incorrect.', invalid_nickname: 'Use 2–32 letters, numbers, dots, hyphens or underscores.', nickname_taken: 'This nickname belongs to a password account. Use the account login or contact the administrator.', member_disabled: 'This account is disabled. Contact the administrator.', rate_limited: 'Too many attempts. Please wait a minute.' };
    return <div className="auth"><div className="login-card"><div className="brand"><span className="brand-bars" aria-hidden="true"><i/><i/><i/></span>Nauctions</div><h1>Join your friends</h1><p>Enter your nickname and invitation code. Use your existing nickname to reconnect. This computer will remember you.</p>
        <form onSubmit={async e => { e.preventDefault(); setBusy(true); setFailure(''); const form = new FormData(e.currentTarget); try { await api('/auth/join', 'POST', { nickname: form.get('nickname'), code: form.get('code') }); await onLogin(); } catch (e) { const message = (e as Error).message; setFailure(messages[message] ?? message); } finally { setBusy(false); } }}>
            <label>Nickname<input name="nickname" autoComplete="nickname" minLength={2} maxLength={32} required disabled={busy}/></label>
            <label>Invitation code<input name="code" autoComplete="off" maxLength={64} required disabled={busy}/></label>
            {failure && <p className="error-state" role="alert">{failure}</p>}<button className="primary" disabled={busy}>{busy ? 'Joining…' : 'Join'}</button>
        </form><details className="admin-access"><summary>Administration</summary><button className="text-button" onClick={() => setAdmin(true)}>Administrator access</button></details>
    </div></div>;
}
function AdminLogin({ onLogin, error }: {
    onLogin: () => Promise<void>;
    error: string;
}) { const [invite, setInvite] = useState(new URLSearchParams(location.search).has('invite')), [busy, setBusy] = useState(false), [failure, setFailure] = useState(error); return <div className="auth"><div className="login-card"><div className="brand"><span className="brand-bars" aria-hidden="true"><i /><i /><i /></span>Nauctions</div><h1>{invite ? 'Join your workspace' : 'Sign in'}</h1><p>Search your market. Find your next opportunity.</p><form onSubmit={async (e) => { e.preventDefault(); setBusy(true); setFailure(''); const form = new FormData(e.currentTarget); try {
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
} }}>{invite && <><label>Invitation code<input name="code" required defaultValue={new URLSearchParams(location.search).get('invite') ?? ''}/></label><label>Display name<input name="display_name" required autoComplete="nickname"/></label></>}<label>Login<input name="login" autoComplete="username" required/></label><label>Password<input name="password" type="password" minLength={invite ? 12 : 1} required autoComplete={invite ? 'new-password' : 'current-password'}/></label>{failure && <p role="alert" className="error-state">{failure}</p>}<button className="primary" disabled={busy}>{busy ? 'Please wait…' : invite ? 'Accept invitation' : 'Sign in'}</button></form><button className="text-button" onClick={() => setInvite(!invite)}>{invite ? 'Back to sign in' : 'Have an invitation code?'}</button></div></div>; }
async function start() {
    if (isDesktop()) {
        try { setTelemetryEnabled(await desktopInvoke<boolean>('observability_preference')); } catch { setTelemetryEnabled(false); }
    }
    initTelemetry();
    createRoot(document.getElementById('root')!).render(<React.StrictMode><RecoveryBoundary><DesktopRoot><App /></DesktopRoot><Diagnostics/></RecoveryBoundary></React.StrictMode>);
}
void start();

import {
  createContext,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { Star } from "lucide-react";
import { api } from "./api";
import { useMarketRefresh } from "./marketClock";
import { Button } from "./components/controls";

type WatchState = {
  keys: string[];
  loading: boolean;
  pending: boolean;
  error: string;
  reload: () => void;
  toggle: (key: string) => Promise<void>;
};
const Context = createContext<WatchState | null>(null);
export const useWatchlist = () => useContext(Context);

export function WatchlistProvider({ children }: { children: ReactNode }) {
  const [keys, setKeys] = useState<string[]>([]),
    [loading, setLoading] = useState(true);
  const [pending, setPending] = useState(false),
    [error, setError] = useState(""),
    [retry, setRetry] = useState(0);
  const refresh = useMarketRefresh(),
    generation = useRef(0),
    saving = useRef(false);
  useEffect(() => {
    if (saving.current) return;
    let active = true;
    const version = ++generation.current;
    api<{ item_key: string }[]>("/watchlists")
      .then((rows) => {
        if (active && version === generation.current) {
          setKeys(rows.map((row) => row.item_key));
          setError("");
        }
      })
      .catch((e) => {
        if (active && version === generation.current) setError(e.message);
      })
      .finally(() => {
        if (active && version === generation.current) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, [refresh, retry]);
  async function toggle(key: string) {
    if (saving.current || loading || error) return;
    saving.current = true;
    ++generation.current;
    setPending(true);
    const exists = keys.includes(key);
    try {
      await api(
        exists ? `/watchlists/${encodeURIComponent(key)}` : "/watchlists",
        exists ? "DELETE" : "POST",
        exists ? undefined : { item_key: key },
      );
      setKeys((current) =>
        exists
          ? current.filter((k) => k !== key)
          : [...new Set([...current, key])],
      );
      setError("");
    } catch (e) {
      setError((e as Error).message);
    } finally {
      saving.current = false;
      ++generation.current;
      setPending(false);
    }
  }
  return (
    <Context.Provider
      value={{
        keys,
        loading,
        pending,
        error,
        reload: () => setRetry((n) => n + 1),
        toggle,
      }}
    >
      {children}
    </Context.Provider>
  );
}

export function WatchlistButton({ itemKey }: { itemKey: string }) {
  const watch = useWatchlist();
  if (!watch) return null;
  const saved = watch.keys.includes(itemKey);
  return (
    <div className="watchlist-action">
      <Button
        disabled={watch.loading || watch.pending || !!watch.error}
        aria-pressed={saved}
        onClick={() => void watch.toggle(itemKey)}
      >
        <Star
          size={16}
          fill={saved ? "currentColor" : "none"}
          aria-hidden="true"
        />
        {watch.loading
          ? "Loading watchlist…"
          : saved
            ? "Remove from watchlist"
            : "Add to watchlist"}
      </Button>
      {watch.error && (
        <div role="alert">
          Could not update the watchlist. {watch.error}{" "}
          <Button onClick={watch.reload}>Retry watchlist</Button>
        </div>
      )}
    </div>
  );
}

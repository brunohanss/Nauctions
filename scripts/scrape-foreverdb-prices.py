"""Archive and extract the public data that renders ForeverDB's item price charts.

No authenticated API, game-file writes, synthetic observations, or Lua execution.
Prices are source daily asking-price summaries. Realms are never combined.
"""
import argparse
from collections import Counter
from datetime import date, datetime, timezone
import hashlib
import html
import json
from pathlib import Path
import re
import time
import urllib.error
import urllib.request

BASE = "https://foreverdb.net"
REALMS = ("Classic Beta PvE 2", "Classic Beta PvE")
ATTRIBUTION = {"text": "Data from ForeverDB.net", "url": BASE}


def validate_integer(value, field, nullable=False):
    if nullable and value is None:
        return None
    if isinstance(value, bool) or not isinstance(value, int) or value < 0 or value > 9007199254740991:
        raise ValueError(f"Invalid {field}: {value!r}")
    return value


def extract_item(item_id, item, names, source_url, retrieved_at, source_updated):
    if not isinstance(item, dict) or not isinstance(item.get("h", {}), dict):
        raise ValueError(f"Invalid item price record: {item_id}")
    rows, latest = [], []
    for realm in REALMS:
        market = f"{realm}|H"
        if market not in item.get("h", {}):
            continue
        record = item["h"][market]
        common = {"source": "ForeverDB", "source_url": f"{BASE}/item/{item_id}#price-history",
                  "source_data_url": source_url, "retrieved_at": retrieved_at,
                  "source_updated_at": source_updated, "item_id": item_id,
                  "item_key": f"i{item_id}", "name": names.get(item_id),
                  "realm": realm, "house": "Horde", "region": None,
                  "price_kind": "asking_price", "unit": "copper_per_unit",
                  "variants_combined": bool(item.get("v")),
                  "variant_count": item.get("v"), "attribution": ATTRIBUTION}
        seen = set()
        for point in record.get("hist", []):
            if not isinstance(point, list) or len(point) != 4:
                raise ValueError(f"Unsupported history shape: item {item_id} market {market}")
            day, minimum, median, quantity = point
            date.fromisoformat(day)
            if day in seen:
                raise ValueError(f"Duplicate daily identity: {item_id} {market} {day}")
            seen.add(day)
            minimum = validate_integer(minimum, "minimum", True)
            median = validate_integer(median, "median", True)
            quantity = validate_integer(quantity, "quantity", True)
            rows.append({**common, "date": day, "min_buyout_copper": minimum or None,
                         "median_buyout_copper": median or None, "listed_quantity": quantity,
                         "source_granularity": "daily_summary", "daily_scan_count": None,
                         "daily_contributor_count": None, "daily_confirmation": "unknown"})
        # These counts and confirmation flags describe the displayed latest record,
        # not each earlier daily observation. Do not propagate them into history.
        latest.append({**common, "last_seen_date": record.get("d"),
                       "min_buyout_copper": validate_integer(record.get("p"), "latest minimum", True) or None,
                       "median_buyout_copper": validate_integer(record.get("m"), "latest median", True) or None,
                       "listed_quantity": validate_integer(record.get("q"), "latest quantity", True),
                       "listing_count": record.get("n"), "reported_players": record.get("u"),
                       "reported_scans": record.get("s"), "confirmed": bool(record.get("c")),
                       "history_days": len(seen)})
    return rows, latest


class Downloader:
    def __init__(self, directory, delay):
        self.directory, self.delay, self.last = directory, delay, 0.0
        self.records = []

    def fetch(self, path, filename):
        if not path.startswith("/") or path.startswith("//") or path.startswith("/api/"):
            raise ValueError("Only the public page and chart assets are supported")
        url = BASE + path
        for attempt in range(3):
            time.sleep(max(0, self.delay - (time.monotonic() - self.last)))
            self.last = time.monotonic()
            request = urllib.request.Request(url, headers={"User-Agent": "NauctionsResearch/1.0 (public chart archive; attribution retained)"})
            try:
                with urllib.request.urlopen(request, timeout=30) as response:
                    if urllib.parse.urlsplit(response.url).netloc != "foreverdb.net":
                        raise ValueError("Unexpected cross-site redirect")
                    body = response.read(64 * 1024 * 1024 + 1)
                    if len(body) > 64 * 1024 * 1024:
                        raise ValueError("Public source exceeds the 64 MiB download limit")
                    headers = {k: response.headers.get(k) for k in ["ETag", "Last-Modified", "Content-Type"]}
                break
            except urllib.error.HTTPError as error:
                if error.code not in [429, 502, 503, 504] or attempt == 2:
                    raise
                retry = error.headers.get("Retry-After", "5")
                if not retry.isdigit() or int(retry) > 60:
                    raise RuntimeError("Source requested a longer delay; run later") from error
                time.sleep(max(int(retry), 2 ** attempt))
        target = self.directory / filename
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(body)
        self.records.append({"url": url, "file": str(target), "bytes": len(body),
                             "sha256": hashlib.sha256(body).hexdigest(), "headers": headers})
        return body


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def make_report(out, summary, latest, daily):
    esc = lambda v: html.escape(str(v if v is not None else "Unknown"))
    grouped = {realm: [] for realm in REALMS}
    for row in latest:
        grouped[row["realm"]].append(row)
    sections = []
    for realm in REALMS:
        info = summary["markets"][realm]
        lookup = {(r["item_id"], r["date"]): r for r in daily if r["realm"] == realm}
        dates = info["distinct_dates"]
        body = []
        for item in sorted(grouped[realm], key=lambda r: (r["name"] or "", r["item_id"])):
            cells = []
            for day in dates:
                row = lookup.get((item["item_id"], day))
                cells.append('<td>—</td>' if row is None else f'<td>{esc(row["min_buyout_copper"])} / {esc(row["median_buyout_copper"])}</td>')
            body.append(f'<tr><td><a href="{BASE}/item/{item["item_id"]}#price-history">{esc(item["name"] or item["item_id"])}</a></td><td>{item["item_id"]}</td><td>{item["history_days"]}</td>{"".join(cells)}</tr>')
        headers = ''.join(f'<th>{esc(day)}<br>Min / Median (copper)</th>' for day in dates)
        sections.append(f'<section><h2>{esc(realm)} — Horde</h2><p>{info["items_with_history"]:,} items with history · {info["daily_rows"]:,} daily records · {esc(info["first_date"])} to {esc(info["last_date"])}</p><div style="overflow-x:auto"><table><thead><tr><th>Item</th><th>ID</th><th>Observed days</th>{headers}</tr></thead><tbody>{"".join(body)}</tbody></table></div></section>')
    document = '<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>ForeverDB normal Horde beta history</title><style>body{font:15px system-ui;max-width:1250px;margin:36px auto;padding:0 20px;color:#172333;background:#f7f9fc}a{color:#145ab6}table{width:100%;border-collapse:collapse;background:white}td,th{text-align:left;padding:9px;border-bottom:1px solid #dce3ec}th{position:sticky;top:0;background:#eaf0f8}section{margin:34px 0}input{padding:10px;width:350px;max-width:90%}</style><h1>Normal Horde beta price history</h1><p>Data from <a href="https://foreverdb.net">ForeverDB.net</a>. Extracted from the same public data that renders its item-page charts. Realms are shown separately.</p><p>Asking prices, in copper per unit. Missing dates remain missing. Latest confirmation counts do not establish daily history quality. No region or beta reset time is supplied by the source. Random item variants can be combined.</p><p><a href="daily-prices.jsonl">All daily records (JSON Lines)</a> · <a href="latest-prices.json">Latest displayed prices</a> · <a href="coverage.json">Full catalog coverage</a> · <a href="summary.json">Collection summary</a></p><label>Filter items <input id="filter" type="search" placeholder="Name or item ID"></label>' + "".join(sections) + '<script>document.getElementById("filter").addEventListener("input",function(){const q=this.value.toLowerCase();document.querySelectorAll("tbody tr").forEach(r=>r.hidden=!r.textContent.toLowerCase().includes(q))})</script></html>'
    (out / "report.html").write_text(document, encoding="utf-8")


def run(out, delay):
    out.mkdir(parents=True, exist_ok=True)
    download = Downloader(out / "raw", delay)
    retrieved = datetime.now(timezone.utc).isoformat()
    robots = download.fetch("/robots.txt", "robots.txt").decode()
    if re.search(r"Disallow:\s*/data/?\s*$", robots, re.M):
        raise RuntimeError("The source excludes its page data from crawling")
    page = download.fetch("/item/1180", "sample-item.html").decode()
    if "Price over time" not in page:
        raise ValueError("Sample item page did not expose the expected historical-price section")
    index_bytes = download.fetch("/data/prices/index.json", "prices-index.json")
    index = json.loads(index_bytes)
    if index.get("schema") != 1:
        raise ValueError("Unsupported price index schema")
    catalog = json.loads(download.fetch("/data/items.json", "items.json"))
    if not isinstance(catalog, list):
        raise ValueError("Unsupported public item catalog shape")
    names = {int(i["id"]): i.get("n") for i in catalog}
    daily, latest, all_ids = [], [], set()
    for shard in range(64):
        path = f"/data/prices/{shard}.json"
        prices = json.loads(download.fetch(path, f"prices-{shard}.json"))
        if not isinstance(prices, dict):
            raise ValueError("Unsupported public chart shard shape")
        for key, item in prices.items():
            item_id = int(key)
            if item_id % 64 != shard or item_id in all_ids:
                raise ValueError(f"Duplicate or misplaced price identity: {item_id}")
            all_ids.add(item_id)
            rows, current = extract_item(item_id, item, names, BASE + path, retrieved, index.get("updated"))
            daily.extend(rows)
            latest.extend(current)
        if shard % 8 == 7:
            print(f"Read {shard + 1}/64 chart files; {len(daily):,} normal Horde daily rows", flush=True)
    after = download.fetch("/data/prices/index.json", "prices-index-after.json")
    if json.loads(after) != index:
        raise RuntimeError("The public price index changed during extraction; raw evidence retained, report not published")
    if len(all_ids) != index.get("totals", {}).get("items"):
        raise RuntimeError("Chart item count does not match the source index; raw evidence retained for review")
    daily.sort(key=lambda r: (REALMS.index(r["realm"]), r["item_id"], r["date"]))
    latest.sort(key=lambda r: (REALMS.index(r["realm"]), r["item_id"]))
    coverage = []
    for realm in REALMS:
        counts = Counter(r["item_id"] for r in daily if r["realm"] == realm)
        covered = {r["item_id"] for r in latest if r["realm"] == realm}
        for item_id in sorted(set(names) | covered):
            coverage.append({"item_id": item_id, "name": names.get(item_id), "realm": realm,
                             "house": "Horde", "observed_days": counts[item_id],
                             "status": "history_available" if counts[item_id] else "latest_only" if item_id in covered else "no_recorded_price_history"})
    summary = {"schema_version": 1, "source": BASE, "attribution": ATTRIBUTION,
               "retrieved_at": retrieved, "source_updated_at": index.get("updated"),
               "source_window_days": index.get("days"), "catalog_items": len(names),
               "source_items_with_prices_any_market": len(all_ids), "markets": {},
               "limitations": ["Daily summaries are not individual scans or confirmed sales.",
                               "No beta reset time or market region is supplied; none is inferred.",
                               "Classic Beta PvE and Classic Beta PvE 2 remain separate.",
                               "Missing days are not filled. Unknown prices are not zero.",
                               "Some prices combine random item variants.",
                               "Latest player/scan/confirmation counts do not describe every daily point."]}
    for realm in REALMS:
        rows = [r for r in daily if r["realm"] == realm]
        dates = sorted({r["date"] for r in rows})
        summary["markets"][realm] = {"house": "Horde", "items_with_history": len({r["item_id"] for r in rows}),
                                   "daily_rows": len(rows), "distinct_dates": dates,
                                   "first_date": dates[0] if dates else None, "last_date": dates[-1] if dates else None,
                                   "items_with_multiple_days": sum(n >= 2 for n in Counter(r["item_id"] for r in rows).values()),
                                   "source_house_metadata": next((h for h in index.get("houses", []) if h["realm"] == realm and h["faction"] == "H"), None)}
    with (out / "daily-prices.jsonl").open("w", encoding="utf-8") as file:
        for row in daily:
            file.write(json.dumps(row, ensure_ascii=False) + "\n")
    for realm, filename in [(REALMS[0], "normal-horde-daily.jsonl"), (REALMS[1], "older-pve-horde-daily.jsonl")]:
        with (out / filename).open("w", encoding="utf-8") as file:
            for row in daily:
                if row["realm"] == realm:
                    file.write(json.dumps(row, ensure_ascii=False) + "\n")
    write_json(out / "latest-prices.json", latest)
    write_json(out / "coverage.json", coverage)
    write_json(out / "summary.json", summary)
    write_json(out / "provenance.json", {"attribution": ATTRIBUTION, "retrieved_at": retrieved, "downloads": download.records})
    make_report(out, summary, latest, daily)
    print(json.dumps(summary, indent=2), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path("data/foreverdb-normal-horde"))
    parser.add_argument("--delay", type=float, default=1.0)
    parser.add_argument("--render-existing", action="store_true", help="Rebuild only the HTML report from the local extracted data")
    args = parser.parse_args()
    if args.delay < 1:
        parser.error("Use at least one second between requests")
    if args.render_existing:
        make_report(args.output, json.loads((args.output / "summary.json").read_text(encoding="utf-8")),
                    json.loads((args.output / "latest-prices.json").read_text(encoding="utf-8")),
                    [json.loads(line) for line in (args.output / "daily-prices.jsonl").read_text(encoding="utf-8").splitlines()])
    else:
        run(args.output, args.delay)

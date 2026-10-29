import { api } from './api';
export type ExportFile = { name: string; content: string };
const bytes = (n: number, width: number) => { const a = new Uint8Array(width); for (let i = 0; i < width; i++) a[i] = n >>> (8 * i) & 255; return a; };
function crc32(a: Uint8Array) { let crc = 0xffffffff; for (const b of a) { crc ^= b; for (let i = 0; i < 8; i++) crc = crc >>> 1 ^ (crc & 1 ? 0xedb88320 : 0); } return (crc ^ 0xffffffff) >>> 0; }
function join(parts: Uint8Array[]) { const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0)); let offset = 0; for (const p of parts) { out.set(p, offset); offset += p.length; } return out; }
/** Stored ZIP: two data-only addon files; no dependencies or game-folder writes. */
export function addonZip(files: ExportFile[]): Uint8Array {
  const body: Uint8Array[] = [], central: Uint8Array[] = []; let offset = 0;
  for (const file of files) {
    if (!['NauctionsForecastCache/NauctionsForecastCache.toc', 'NauctionsForecastCache/ForecastData.lua'].includes(file.name)) throw new Error('Unexpected addon export file.');
    const name = new TextEncoder().encode(file.name), data = new TextEncoder().encode(file.content), crc = crc32(data);
    const local = join([bytes(0x04034b50, 4), bytes(20, 2), bytes(0x800, 2), bytes(0, 2), bytes(0, 2), bytes(33, 2), bytes(crc, 4), bytes(data.length, 4), bytes(data.length, 4), bytes(name.length, 2), bytes(0, 2), name, data]);
    central.push(join([bytes(0x02014b50, 4), bytes(20, 2), bytes(20, 2), bytes(0x800, 2), bytes(0, 2), bytes(0, 2), bytes(33, 2), bytes(crc, 4), bytes(data.length, 4), bytes(data.length, 4), bytes(name.length, 2), bytes(0, 2), bytes(0, 2), bytes(0, 2), bytes(0, 2), bytes(0, 4), bytes(offset, 4), name]));
    body.push(local); offset += local.length;
  }
  const directory = join(central);
  return join([...body, directory, join([bytes(0x06054b50, 4), bytes(0, 2), bytes(0, 2), bytes(files.length, 2), bytes(files.length, 2), bytes(directory.length, 4), bytes(offset, 4), bytes(0, 2)])]);
}
export async function downloadForecasts(market: string, keys: string[], day = 0) {
  const result = await api<{ files: ExportFile[] }>('/forecasts/addon', 'POST', { market_id: market, item_keys: keys, day });
  const zip = addonZip(result.files);
  const url = URL.createObjectURL(new Blob([zip as BlobPart], { type: 'application/zip' }));
  const a = document.createElement('a'); a.href = url; a.download = 'nauctions-forecast-cache.zip'; a.click();
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
}

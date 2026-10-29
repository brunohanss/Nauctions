import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { readFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { mkdirSync, readdirSync, renameSync } from 'node:fs';
import posthogPlugin from '@posthog/rollup-plugin';
let local: { project_key?: string } = {};
try { local = JSON.parse(readFileSync(new URL('../telemetry.local.json', import.meta.url), 'utf8')); } catch { /* CI uses environment configuration. */ }
const release = JSON.parse(readFileSync(new URL('../desktop/tauri.conf.json', import.meta.url), 'utf8')).version;
let revision = process.env.NAUCTIONS_REVISION ?? 'unknown';
try { revision = execFileSync('git', ['rev-parse', '--short=12', 'HEAD'], { encoding: 'utf8' }).trim(); } catch { /* Source archive build. */ }
const projectKey = process.env.POSTHOG_API_KEY ?? local.project_key ?? '';
if (projectKey && !projectKey.startsWith('phc_')) throw new Error('POSTHOG_API_KEY must be a project ingestion key. Use POSTHOG_PERSONAL_API_KEY for source-map uploads.');
const uploadMaps = !!process.env.POSTHOG_PERSONAL_API_KEY && !!process.env.POSTHOG_PROJECT_ID;
export default defineConfig({
  define: {
    __POSTHOG_KEY__: JSON.stringify(projectKey),
    __RELEASE__: JSON.stringify(release),
    __REVISION__: JSON.stringify(revision),
  },
  build: { sourcemap: 'hidden' },
  clearScreen: false,
  plugins: [react(), ...(uploadMaps ? [posthogPlugin({
    personalApiKey: process.env.POSTHOG_PERSONAL_API_KEY!, projectId: process.env.POSTHOG_PROJECT_ID!,
    host: 'https://eu.posthog.com',
    sourcemaps: { enabled: true, releaseName: 'nauctions', releaseVersion: `${release}+${revision}`, deleteAfterUpload: true },
  })] : [{
    name: 'archive-local-source-maps',
    apply: 'build' as const,
    closeBundle() {
      const assets = new URL('./dist/assets/', import.meta.url);
      const archive = new URL(`../artifacts/symbols/web/${release}-${revision}/`, import.meta.url);
      mkdirSync(archive, { recursive: true });
      for (const file of readdirSync(assets)) if (file.endsWith('.map')) renameSync(new URL(file, assets), new URL(file, archive));
    },
  }])],
  server: {
    host: "127.0.0.1",
    strictPort: true,
    proxy: { "/v1": { target: "http://127.0.0.1:8080", changeOrigin: true } },
  },
});

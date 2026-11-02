import { spawn } from "node:child_process";
import { createServer } from "node:net";
const listener = createServer();
await new Promise((resolve) => listener.listen(0, "127.0.0.1", resolve));
const port = listener.address().port;
await new Promise((resolve) => listener.close(resolve));
const origin = `http://127.0.0.1:${port}`;
const server = spawn(
  process.execPath,
  [
    "node_modules/vite/bin/vite.js",
    "preview",
    "--host",
    "127.0.0.1",
    "--port",
    String(port),
    "--strictPort",
  ],
  { stdio: "ignore" },
);
try {
  let ready = false;
  for (let i = 0; i < 80; i++) {
    try {
      ready = (await fetch(origin)).ok;
      if (ready) break;
    } catch {}
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
  if (!ready) throw new Error("Production preview did not start");
  for (const script of (process.env.SCENARIO ? ["e2e/acceptance.mjs"] : ["e2e/migration.mjs", "e2e/acceptance.mjs"])) {
    const child = spawn(process.execPath, [script], {
      stdio: "inherit",
      env: { ...process.env, UI_ORIGIN: origin },
    });
    const code = await new Promise((resolve, reject) => {
      child.on("error", reject);
      child.on("exit", resolve);
    });
    if (code !== 0) throw new Error(`${script} failed with exit code ${code}`);
  }
} finally {
  server.kill();
}

# Nauctions shadcn UI migration

The migration is local on `codex/shadcn-migration`. The existing Market navigation and telemetry changes were preserved. Release candidate: 0.5.0. Publication and server deployment are authorized after validation.

## Completed implementation

- Tailwind 4, the Vite plugin, the source alias, and the official Radix shadcn primitives are configured.
- Market, Crafting, Strategy, authentication, setup, connection, Settings, updates, diagnostics, and recovery use the shared UI foundation.
- Dark and light themes use one semantic palette. The dark theme is the first-run default. Theme selection survives reload and sign-out. If browser storage is blocked, the session preference still works.
- The previous blue theme, color aliases, global control styles, native modal implementation, and obsolete layout rules were removed. The remaining application CSS defines layout and domain presentation. The existing SVG chart logic is retained.
- Shared results toolbars, loading rows, pagination, item artwork, money formatting, and dialog layouts are used across features.
- Settings sections use Accordion. Technical methods use Collapsible. Chart controls use ToggleGroup. Brief export feedback uses Sonner. Export errors remain visible. Revocation and restore actions use AlertDialog with Cancel as the default.
- Item and recipe dialogs preserve list state. Search suggestions close before a detail dialog opens. Escape affects the active overlay. Dialog focus returns to the opener or the page for direct links.
- Navigation and item links use browser paths or Tauri hash links. Modifier-click behavior is preserved.
- Market keeps server sorting and cursor pagination. Crafting keeps server sorting and offset pagination. Forecast calculations and missing-data semantics are unchanged.
- Numeric table columns remain available in narrow windows. Horizontal scrolling stays inside focusable table regions.
- The frontend bundle is split into application, UI foundation, React, and telemetry chunks. The production build has no large-chunk warning.

## Component inventory

Button, Input, Label, Field, Select, ToggleGroup, Table, Pagination, Dialog, AlertDialog, Popover, Command, DropdownMenu, Tooltip, Badge, Separator, Accordion, Collapsible, Checkbox, Switch, Skeleton, Alert, Empty, Progress, Spinner, Sonner, Card, and Kbd.

Item artwork, money formatting, freshness calculations, SVG charts, semantic navigation, definition lists, and the setup step list remain domain components. No remote fonts or replacement chart library were added.

## Validation

- Baseline: 54 tests passed.
- Current frontend suite: 60 tests passed.
- TypeScript and production build passed.
- The final production browser run passed all 20 acceptance scenarios and regenerated all six screenshots.
- Native `cargo check --manifest-path desktop/Cargo.toml --locked` passed.
- An isolated debug companion compiled with a separate application identifier and data folder. Native checks confirmed the connection screen, signed-out Settings dialog, scroll area, theme popup, and light-theme selection. Native UI automation was then stopped with Escape. No further native UI input was sent.
- Browser validation uses the production build. The runner starts and stops an isolated Vite preview server. API and desktop IPC fixtures do not change live data or game files.
- The acceptance suite covers server queries, pagination, filters, list-state preservation, keyboard search, forecast states and export, quote warnings, recipe-to-item transitions, artwork failure, missing prices, stale observations, theme persistence, blocked storage, routing, zoom, focus trapping, confirmations, Strategy review, offline recovery, setup, and simulated update progress.
- Accessibility checks cover populated Market, item and recipe dialogs, signed-out Settings, zoom, and desktop fixture screens.
- Layout checks cover 640, 900, 1280, and 1600 px widths, long-page sticky navigation, and horizontal overflow. Browser tests also check 200 percent zoom and reduced motion.

## Run checks

Use PowerShell from the repository root:

```powershell
& 'C:\nvm4w\nodejs\npm.cmd' test --prefix web
& 'C:\nvm4w\nodejs\npm.cmd' run build --prefix web
& 'C:\nvm4w\nodejs\npm.cmd' run test:browser --prefix web
cargo check --manifest-path desktop/Cargo.toml --locked
```

Install the Playwright Chromium browser before the first browser run. The browser runner uses a free local port. It closes its preview process after the run. Screenshots are in `artifacts/ui`: Market, Crafting, and item details in both themes.

## Native verification limits

Browser desktop fixtures do not prove native IPC or game-file behavior. Populated native Market and Crafting, native hash navigation, native window resizing, actual setup, addon installation, collection, update installation, and recovery writes were not exercised against live state. The native UI check was stopped by the user. These operational checks must remain explicit release-verification items.

The earlier installer build could not download NSIS because the environment could not resolve GitHub. Native compilation passed, but installer packaging was not completed. No signed artifacts or release were published.

## Merchant crafting profit

Merchant profit is implemented in the recipe list, recipe dialog, and product details. Each merchant estimate uses the known output yield, collected sell price, and whole-stack reagent purchase cost. Additional costs apply to the batch. Auction fees apply only to auction sales. Missing prices, insufficient supply, variable yields, service outputs, and known merchant refusals leave profit unavailable.

The server sorts and filters the complete result before pagination. The list supports all displayed data columns and a positive-merchant-profit filter. Product details request recipes for the exact output item. The addon records output metadata from profession discoveries even when there is no output auction. Existing generic item metadata stores these prices, so the release needs no SQL migration.

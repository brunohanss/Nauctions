
# NAUCTIONS — ANALYTICS FOCUS UI SPECIFICATION
Version: 1.0
Status: Approved design direction
Target: React + TypeScript web application
Primary use: WoW Forever Auction House opportunity tracking

## 1. DESIGN INTENT

Create a compact, dark, data-focused web application.

The interface must feel like a modern market analysis tool.
It must not look like a traditional World of Warcraft interface.

Design principles:
- Data first. Decoration second.
- Dark navy backgrounds with subtle blue accents.
- High information density without sacrificing readability.
- Bright green for profitable opportunity indicators.
- Red for adverse price movements or negative states.
- Compact tables and minimal navigation.
- No permanent sidebar in the initial version.
- No large KPI cards on the main opportunities screen.
- No decorative images or fantasy textures.
- No glassmorphism, heavy shadows, or excessive gradients.
- Use real WoW item icons as the primary visual connection to the game.

The interface must prioritize two user actions:
1. Search for a specific item.
2. Browse and inspect the best market opportunities.

## 2. APPLICATION STRUCTURE

Global header:
- Nauctions logo on the left.
- Opportunities tab.
- Items tab.
- Strategy tab (research candidates; see `strategy.md`).
- Settings icon on the right.

Below the header:
- Global item search input.
- Context-specific content.

Opportunities page:
- Category filters.
- Sortable results table.
- Optional pagination or virtual scrolling.

Item details:
- Back navigation.
- Item icon and name.
- Current pricing information.
- Large historical chart.
- Listing details.
- Historical summary.

Strategy is the additional research section. It uses the same compact table design.

Routes:
- /opportunities
- /strategy
- /items
- /items/:itemKey
- /settings

Default route: /opportunities

## 3. CSS DESIGN TOKENS

Implement the following variables in the root stylesheet.

All components must consume tokens rather than duplicate literal colors, sizes, or spacing.

### 3.1 Colors

:root {
  color-scheme: dark;

  /* Background hierarchy */
  --bg-app: #08111e;
  --bg-header: #091525;
  --bg-surface: #101d2d;
  --bg-surface-raised: #142438;
  --bg-surface-hover: #192d43;
  --bg-surface-active: #1c354d;
  --bg-table-header: #0b1828;
  --bg-table-row: #0b1725;
  --bg-table-row-alt: #0d1b2a;
  --bg-input: #152437;
  --bg-input-hover: #192b40;
  --bg-popover: #132236;
  --bg-tooltip: #16273b;
  --bg-overlay: #020814cc;

  /* Borders */
  --border-default: #25374b;
  --border-subtle: #1b2a3a;
  --border-strong: #38516b;
  --border-focus: #2585f8;
  --border-table: #1b2c3d;
  --border-input: #486078;

  /* Brand colors */
  --accent-primary: #2585f8;
  --accent-primary-hover: #449bff;
  --accent-primary-active: #1769ce;
  --accent-primary-muted: #12385d;
  --accent-primary-foreground: #ffffff;

  /* Text */
  --text-primary: #dde8f5;
  --text-secondary: #879aaf;
  --text-muted: #627990;
  --text-disabled: #526378;
  --text-inverse: #08111e;
  --text-link: #67b7ff;

  /* Opportunity and price status */
  --success: #40d7a0;
  --success-strong: #20e6a3;
  --success-bg: #103b35;
  --success-border: #205e4f;

  --danger: #f46b70;
  --danger-strong: #ff525b;
  --danger-bg: #46222c;
  --danger-border: #713841;

  --warning: #ffb454;
  --warning-bg: #46351e;
  --warning-border: #795727;

  --info: #58adff;
  --info-bg: #15375c;

  /* Chart colors */
  --chart-median: #2585f8;
  --chart-minimum: #40d7a0;
  --chart-maximum: #f6ac50;
  --chart-quantity: #334e68;
  --chart-grid: #213247;
  --chart-axis: #879aaf;
  --chart-crosshair: #b0c4d8;
  --chart-highlight: #ffffff;

  /* Item rarity colors */
  --rarity-poor: #9d9d9d;
  --rarity-common: #ffffff;
  --rarity-uncommon: #1eff00;
  --rarity-rare: #0070dd;
  --rarity-epic: #a335ee;
  --rarity-legendary: #ff8000;
}

### 3.2 Typography

:root {
  --font-family: Inter, system-ui, sans-serif;
  --font-family-numeric: Inter, system-ui, sans-serif;

  --font-size-2xs: 10px;
  --font-size-xs: 11px;
  --font-size-sm: 12px;
  --font-size-md: 13px;
  --font-size-base: 14px;
  --font-size-lg: 16px;
  --font-size-xl: 20px;
  --font-size-2xl: 24px;
  --font-size-3xl: 30px;

  --font-weight-normal: 400;
  --font-weight-medium: 500;
  --font-weight-semibold: 600;
  --font-weight-bold: 700;

  --line-height-tight: 1.2;
  --line-height-normal: 1.5;
  --line-height-relaxed: 1.7;

  --letter-spacing-tight: -0.025em;
  --letter-spacing-normal: 0;
  --letter-spacing-wide: 0.04em;
}

Typography rules:
- Use Inter for all UI elements.
- Use tabular numbers for prices, quantities, and percentages.
- Apply font-variant-numeric: tabular-nums to numerical columns.
- Do not use fantasy or ornamental fonts.
- Use semibold text for item names.
- Use medium or semibold text for column headers.
- Use muted text for secondary metadata.
- Do not depend on color alone to convey status.

### 3.3 Spacing

:root {
  --space-0: 0;
  --space-1: 4px;
  --space-2: 8px;
  --space-3: 12px;
  --space-4: 16px;
  --space-5: 20px;
  --space-6: 24px;
  --space-7: 28px;
  --space-8: 32px;
  --space-10: 40px;
  --space-12: 48px;
  --space-16: 64px;
}

Spacing policy:
- Default compact gap: 8px.
- Gap between controls: 8–12px.
- Gap between major sections: 16–24px.
- Desktop page padding: 24px.
- Tablet page padding: 16px.
- Mobile page padding: 12px.

### 3.4 Radius, borders and shadows

:root {
  --radius-xs: 3px;
  --radius-sm: 4px;
  --radius-md: 6px;
  --radius-lg: 8px;
  --radius-xl: 12px;
  --radius-pill: 9999px;

  --border-width: 1px;

  --shadow-popover:
    0 12px 32px rgba(0, 0, 0, 0.35);

  --shadow-modal:
    0 20px 60px rgba(0, 0, 0, 0.45);

  --shadow-focus:
    0 0 0 3px rgba(37, 133, 248, 0.22);
}

Rules:
- Prefer borders to shadows.
- Use shadows for floating elements only.
- No large drop shadows around data tables.
- Do not use decorative outer glows.

### 3.5 Dimensions

:root {
  --header-height: 56px;
  --content-max-width: 1600px;

  --search-height: 40px;
  --search-max-width: 100%;

  --tab-height: 44px;
  --filter-height: 30px;

  --table-header-height: 34px;
  --table-row-height: 38px;
  --table-cell-padding-x: 12px;

  --item-icon-size: 26px;
  --item-icon-size-lg: 54px;
  --item-icon-radius: 5px;

  --badge-height: 22px;
  --badge-padding-x: 8px;

  --sparkline-width: 72px;
  --sparkline-height: 24px;

  --chart-height-desktop: 340px;
  --chart-height-tablet: 300px;
  --chart-height-mobile: 240px;

  --details-summary-height: 66px;
}

### 3.6 Motion

:root {
  --duration-fast: 100ms;
  --duration-normal: 160ms;
  --duration-slow: 240ms;

  --ease-standard: cubic-bezier(0.2, 0, 0, 1);
}

Rules:
- Hover transitions: 100–160ms.
- No animated background.
- No bouncing buttons.
- Do not animate price values unless an actual update occurs.
- Support prefers-reduced-motion.

### 3.7 Z-index

:root {
  --z-base: 0;
  --z-sticky: 10;
  --z-dropdown: 30;
  --z-tooltip: 40;
  --z-dialog: 50;
  --z-toast: 60;
}

## 4. GLOBAL HEADER

Height: 56px.
Background: var(--bg-header).
Bottom border: 1px solid var(--border-subtle).

Desktop layout:
- Logo left.
- Main tabs beside the logo.
- Settings icon right.

Nauctions logo:
- Simple blue bar-chart icon.
- White "Nauctions" label.
- Text size: 20px.
- Weight: 600.
- Do not use a decorative fantasy emblem.

Tabs:
- Opportunities.
- Items.

Active tab:
- Primary text color.
- 2px blue underline.

Inactive tab:
- Secondary text color.
- No filled background.

On mobile:
- Preserve logo and navigation.
- Reduce header text size if necessary.
- Never allow horizontal page overflow.

## 5. SEARCH COMPONENT

The search component must be reusable across routes.

Dimensions:
- Height: 40px.
- Width: 100% of content area.
- Background: var(--bg-input).
- Border: 1px solid var(--border-input).
- Radius: 6px.
- Padding: 12px horizontal.

Content:
- Search icon on the left.
- Placeholder: "Search for an item..."
- Optional keyboard shortcut indicator "/" on the right.

Behavior:
- Focus when user presses "/" outside editable fields.
- Escape clears the query or closes the dropdown.
- Search item names and supported aliases.
- Debounce remote searches by approximately 200ms.
- Support keyboard navigation.
- Enter selects the highlighted result.

Search dropdown:
- Same width as the input.
- Maximum height: 360px.
- Scroll when results exceed available space.
- Background: var(--bg-popover).
- Border: var(--border-strong).
- Shadow: var(--shadow-popover).

Each result:
- Item icon: 26px.
- Item name.
- Optional category on the right.
- Selected row background: var(--bg-surface-active).

States:
- Idle.
- Typing.
- Loading.
- Results.
- No results.
- Error.

Do not display search suggestions until the user types, unless recent items are explicitly supported.

## 6. OPPORTUNITIES PAGE

Route: /opportunities

This is the default landing page.

Vertical order:
1. Header.
2. Search.
3. Category filters.
4. Opportunities table.
5. Pagination or infinite loading.

No hero section.
No chart above the results.
No large summary cards.

### 6.1 Category filters

Default:
- All.
- Materials.
- Consumables.
- Recipes.
- Equipment.
- Other.

Appearance:
- Height: 30px.
- Rounded pills.
- Background: transparent or var(--bg-surface).
- Border: var(--border-default).
- Horizontal padding: 14px.
- Gap: 8px.

Active filter:
- Blue background.
- White text.
- Blue border.

Behavior:
- Exactly one active category.
- Switching categories updates the results.
- Preserve sort selection.
- On mobile, filters may scroll horizontally.

### 6.2 Table columns

Desktop order:
1. Rank.
2. Item.
3. Current price.
4. Reference price.
5. Discount.
6. Quantity.
7. Last seen.
8. Trend.

Use actual item data from the backend.

Table styling:
- Background: var(--bg-table-row).
- Header: var(--bg-table-header).
- Header height: 34px.
- Row height: 38px.
- Horizontal separators: var(--border-table).
- No vertical cell borders.
- Border radius: 8px on the outer table.
- Outer border: var(--border-default).

Row hover:
- Background: var(--bg-surface-hover).
- Cursor: pointer.

Selected row:
- Background: var(--bg-surface-active).

Numeric columns:
- Right aligned.
- Tabular numbers.

### 6.3 Item cell

Display:
- 26px item icon.
- Item name.
- Rarity color only where useful.

Gap: 8px.

Icon:
- Use actual item icon when available.
- Neutral placeholder when missing.
- Border: subtle.
- Radius: 5px.

The row click must navigate to the item details route.

### 6.4 Pricing columns

Current price:
- Lowest valid observed buyout per unit.
- Display gold, silver, and copper.
- Never display zero buyout as a free item.

Reference price:
- Clearly defined historical baseline.
- Default to a robust 7-day reference.

Discount:
- Relative deviation from the reference price.
- Display negative percentage when current price is below reference.
- Use green for favorable buying opportunities.

The backend must calculate reference prices and opportunity scores.
The frontend must not independently invent these values.

### 6.5 Opportunity ranking

Default sort:
- Opportunity score descending.

The score must account for:
- Price deviation.
- Historical coverage.
- Freshness.
- Available quantity or depth.
- Optional confidence metrics.

Do not rank by percentage discount alone.

Allow sorting by:
- Opportunity score.
- Discount.
- Current price.
- Quantity.
- Last seen.

Use a small arrow on the active sort header.

### 6.6 Freshness

Display:
- 2m.
- 12m.
- 1h.
- 3h.

The values refer to observation age, not upload age.

Stale observations:
- Use muted text.
- Show a stale indicator when the backend marks them stale.
- Do not present stale listings as immediately actionable.

### 6.7 Sparklines

Size:
- 72px wide.
- 24px high.

Style:
- Stroke width: 1.5px.
- No axis labels.
- No visible grid.
- No dots except optionally on hover.
- No shadow.

Data:
- Historical market reference price.
- Default time window: 24 hours.
- Consistent data source for all rows.

Color:
- Use green for a favorable downward movement.
- Red for an unfavorable upward movement.
- Neutral blue if direction is not classified.

Do not invent intermediate observations to fill scan gaps.

## 7. ITEM DETAIL PAGE

Route: /items/:itemKey

Opening an opportunity must navigate to this page.

Primary purpose:
Provide enough historical information to decide whether the listing is worth investigating.

### 7.1 Header

Display:
- Back to opportunities.
- Item icon: 54px.
- Item name: 24px, weight 600.
- Item category.
- Opportunity discount badge.
- Last observation age.

Back navigation should preserve the previous list's filters, query and scroll position.

### 7.2 Pricing summary

Place a compact summary strip beneath the item title.

Fields:
- Current minimum unit price.
- Current median unit price.
- 7-day reference.
- Difference from reference.
- Latest observed quantity.
- Last seen timestamp.

Design:
- One horizontal surface on desktop.
- Subtle separators between fields.
- Background: var(--bg-surface).
- Radius: 8px.
- Vertical padding: 12px.

On mobile:
- Use a two-column grid.
- Let the final metric occupy the full width if necessary.

### 7.3 Main historical graph

The graph must be the dominant element on the detail page.

Desktop height: 340px.
Tablet height: 300px.
Mobile height: 240px.

Chart library recommendation:
- Apache ECharts, or an equivalent library that supports multiple axes, tooltips and synchronized chart interactions.

Series:
1. Observed quantity-weighted median unit price.
2. Observed minimum unit buyout price.
3. Total observed listed quantity.

Visual configuration:

Median:
- Color: var(--chart-median).
- Stroke width: 2px.

Minimum:
- Color: var(--chart-minimum).
- Stroke width: 2px.

Quantity:
- Color: var(--chart-quantity).
- Low-opacity bars or filled area.
- Use the secondary Y axis.

Chart background:
- Transparent over var(--bg-surface).

Grid:
- Thin horizontal lines.
- Color: var(--chart-grid).
- Avoid dense vertical grid lines.

Axes:
- X axis: time.
- Left Y axis: unit price.
- Right Y axis: observed listed quantity.

Price formatting:
- Use WoW gold, silver and copper.
- Use integer copper internally.
- Convert only for display.

### 7.4 Date range selection

Options:
- 24h.
- 7d.
- 30d.
- All.

Default: 24h.

Active control:
- Background: var(--accent-primary).
- White text.
- Radius: 6px.

Inactive control:
- Background: var(--bg-surface-raised).
- Secondary text.

Changing range:
- Fetch or select appropriate historical data.
- Update axis formatting.
- Preserve selected visible series if possible.

### 7.5 Tooltip

On hover or touch, display:
- Observation date and time.
- Minimum price.
- Median price.
- Listed quantity.
- Optional coverage/quality indicator.

Tooltip:
- Background: var(--bg-tooltip).
- Border: var(--border-strong).
- Radius: 6px.
- Padding: 12px.
- Font size: 12px.
- Shadow: var(--shadow-popover).

Use a vertical crosshair to connect values to the timestamp.

The tooltip must show actual observed values.
Do not interpolate data and present it as a real scan.

### 7.6 Chart gaps

Scan observations are discrete measurements.

Requirements:
- Do not represent missing scans as continuous market activity.
- Display gaps when data coverage is insufficient.
- Do not infer actual trades from disappearing auctions.
- Distinguish missing data from zero observed listings.
- Show a "Limited data" message when necessary.

### 7.7 Secondary content

Place beneath the chart.

Desktop:
- Recent listings on the left, approximately 60% width.
- Price summary on the right, approximately 40% width.

Recent listings:
- Stack size.
- Unit buyout price.
- Total buyout price.
- Observation age.

Price summary:
- Current minimum.
- Current median.
- 7-day reference.
- 7-day minimum.
- 7-day maximum.
- Latest observed quantity.
- Data coverage.

Only show recent listings if full-scan information is available.

If not available:
- Display a concise explanation.
- Do not generate synthetic listing rows.

## 8. ITEMS PAGE

Route: /items

Purpose:
Explore all known items, including items without active opportunities.

Use the same search component.

Table columns:
- Item.
- Latest minimum.
- Latest median.
- 7-day reference.
- 24-hour change.
- Observed quantity.
- Last seen.

Reuse table components from Opportunities.

Do not build a second visual system for this page.

## 9. RESPONSIVE DESIGN

### 9.1 Breakpoints

:root {
  --breakpoint-mobile: 640px;
  --breakpoint-tablet: 1024px;
  --breakpoint-desktop: 1280px;
}

Note:
CSS custom properties must not be used directly in media query conditions. Define equivalent breakpoints in the styling tool configuration or use literal media query values.

Desktop:
- Width greater than 1024px.
- Full table visible.
- Detail chart and secondary panels use available width.

Tablet:
- Width 641–1024px.
- Reduce content padding.
- Allow secondary panels to stack if necessary.
- Keep the main chart visible.

Mobile:
- Width 640px and below.
- Page padding: 12px.
- Search remains full width.
- Filters scroll horizontally.
- Reduce font sizes carefully.
- Preserve minimum touch-target sizes.

### 9.2 Mobile opportunities list

Do not compress all eight table columns into an unreadable grid.

Use compact item rows containing:
- Item icon and name.
- Current price.
- Discount badge.
- Last seen.
- Optional small sparkline.

Hide secondary columns or put them in the detail view.

Each row must remain clickable.

Use a minimum 44px interactive touch target.

### 9.3 Mobile item details

Order:
1. Back.
2. Item title.
3. Discount.
4. Compact summary metrics.
5. Date range selector.
6. Chart.
7. Listings.
8. Historical summary.

The chart must not require horizontal page scrolling.

## 10. COMPONENT INVENTORY

Build reusable components:

- AppHeader
- NavTabs
- GlobalItemSearch
- SearchResultDropdown
- CategoryFilters
- OpportunitiesTable
- OpportunityRow
- ItemIcon
- WowMoney
- DiscountBadge
- FreshnessIndicator
- PriceSparkline
- TableSortHeader
- ItemDetailHeader
- ItemMetricsStrip
- PriceHistoryChart
- ChartRangeSelector
- ChartSeriesLegend
- RecentListingsTable
- PriceSummaryPanel
- LoadingSkeleton
- EmptyState
- ErrorState

Use shared visual tokens across all components.

Suggested component structure:

src/
  app/
    routes/
  components/
    layout/
    search/
    market/
    charts/
    ui/
  features/
    opportunities/
    items/
  lib/
    api/
    formatting/
  styles/
    tokens.css
    globals.css

## 11. DATA CONTRACT EXPECTATIONS

Every item observation should expose:
- itemKey
- itemId
- itemName
- icon
- marketId
- observedAt
- receivedAt
- minUnitCopper
- medianUnitCopper
- quantityListed
- quality or coverage information

Opportunity data should expose:
- itemKey
- currentMinUnitCopper
- referenceUnitCopper
- discountPercentage
- opportunityScore
- lastObservedAt
- quantityListed
- freshnessStatus
- evidence or confidence information

Price history should contain:
- observedAt
- minUnitCopper
- medianUnitCopper
- quantityListed
- sourceQuality

Use integer copper values for money.
Never use floating-point numbers as the stored currency representation.

The frontend must not create fake prices, scans, trades, or listings.

## 12. FORMATTING RULES

Currency:
- 100 copper = 1 silver.
- 100 silver = 1 gold.

Examples:
- 48 -> 48c
- 850 -> 8s 50c
- 12000 -> 1g 20s
- 125050 -> 12g 50s 50c

Choose one compact formatting convention and apply it consistently.

Percentages:
- Maximum one decimal place by default.
- Always include %.
- Negative values indicate price below reference.

Quantities:
- Use thousand separators.
- Use compact notation only when needed.

Times:
- Relative time for compact lists.
- Absolute timestamps in chart tooltips and detailed information.
- Show the configured display timezone.

## 13. ACCESSIBILITY

Requirements:
- Keyboard navigation for search and tabs.
- Visible focus states.
- Semantic tables where applicable.
- Labels for all icon-only controls.
- Sufficient contrast for small text.
- No meaning conveyed only through color.
- Reduced-motion support.
- Touch-friendly targets on mobile.

## 14. EMPTY, LOADING AND ERROR STATES

Loading:
- Use skeleton rows.
- Preserve table column widths.
- Avoid major layout shifts.

No opportunities:
- Show "No opportunities match these filters."
- Allow filter reset.

No item history:
- Show "Not enough observations yet."
- Do not display a fake trend line.

Stale data:
- Show observed timestamp.
- Explain that the market may have changed.

API error:
- Show a compact retry action.
- Preserve search and filters.

## 15. IMPLEMENTATION ACCEPTANCE CRITERIA

AC-01:
The Opportunities page is the default landing page.

AC-02:
The top search field supports item lookup with keyboard navigation.

AC-03:
The Opportunities page displays real backend results in a sortable table.

AC-04:
The default ranking uses the backend opportunity score.

AC-05:
Each row opens the matching item detail page.

AC-06:
Item details display minimum, median and quantity history.

AC-07:
The chart supports 24h, 7d, 30d and All ranges.

AC-08:
Chart tooltips show timestamped observed data.

AC-09:
The interface clearly identifies stale or insufficient observations.

AC-10:
Desktop and mobile layouts are usable without horizontal page overflow.

AC-11:
All reusable colors, sizes and spacing values come from design tokens.

AC-12:
No fantasy textures, ornamental frames or unrelated dashboard widgets are added.

AC-13:
The same components and design tokens are used across Opportunities, Items and Item Details.

## 16. IMPLEMENTATION PRIORITY

P0 — Design foundation:
- Global tokens.
- Typography.
- App header.
- Navigation.
- Responsive page container.

P1 — Opportunities:
- Search.
- Category filters.
- Opportunities table.
- Sorting.
- Row navigation.

P2 — Item details:
- Detail header.
- Metrics.
- Historical chart.
- Date ranges.
- Tooltips.
- Listing and summary panels.

P3 — Polish:
- Loading states.
- Error states.
- Accessibility.
- Mobile optimizations.
- Visual regression tests.

## 17. FINAL DESIGN RULE

The interface must maximize useful market information per screen while keeping the visual hierarchy clear.

Do not add UI elements solely to make the dashboard appear more complex.

The user should be able to:
1. Open Nauctions.
2. Identify the strongest opportunities.
3. Select an item.
4. Inspect its price history and available listings.
5. Decide whether to investigate the opportunity in WoW.

These five actions define the primary user experience.

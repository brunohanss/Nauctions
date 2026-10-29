# Delivery order before and after game access

Date: 29 October 2026. The target game cannot be run yet. Keep actual client validation pending. Do not substitute Classic evidence for Forever evidence.

| Order | Deliverable | Work available now | Acceptance that requires game access |
| --- | --- | --- | --- |
| 1 | Safe synchronization | Complete H02-H12 in the [hardening plan](ahdb-sync-hardening-plan.md). Use disposable Windows files, persisted crash states, the bundled Lua harness, and isolated database tests. | Verify the exact client and addon adapter, then complete H13/H14 before release writes. |
| 2 | Two-player pilot | Exercise two members/devices with synthetic contributions and prepare the trial record. Test replay, outages, queues, revocation, and feedback exclusion. | Two real computers over at least two days; verify actual scans, profession observations, saves, imports, and restore. |
| 3 | Clear data quality | Show observation time, age, coverage, recipe conflict state, missing data, and collection/update status. Test stale, unknown, partial, offline, and rejected states. | Compare displayed values with the client. Do not promote unknown scan completeness automatically. |
| 4 | Crafting shopping lists | Extend the existing quote with batch quantities, selected whole stacks, surplus, missing supply, and a copyable list. Retain one market/snapshot per quote. | Check actual material purchases, recipe yield, fees, and output binding. |
| 5 | Actual trade results | Add explicit manual purchase, craft, sale, fee, deposit-loss, and unsold-stock records. Keep asking-price scenarios separate from realized results. | Record real trades and compare their totals with the ledger. Do not infer a sale from a missing listing. |
| 6 | Group coordination | Add opt-in crafting capabilities, material requests, and expiring opportunity reservations. Enforce ownership and cancellation. | Verify actual known recipes and shared workflows with both players. Recipe discovery alone does not prove that a character knows a recipe. |

Current evidence: the synthetic addon roundtrip and the two-device database roundtrip are implemented. They are separate test layers. They are not a complete two-computer or real-game pilot. See [implementation status](implementation-status.md).

Keep each release gate linked to a candidate revision, test command, observed result, and retained evidence. Label every fixture as synthetic or captured. Record the client build for captured data. Keep alternate-client markets separate from the target market.

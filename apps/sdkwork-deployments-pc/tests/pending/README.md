# Parked console specifications

`delivery-ownership-filter-tabs.test.ts.pending` is the **unlanded half** of the
certificate-coverage picker's specification. It covers the two decisions the
picker makes about *which* names it is showing — which tab a certificate type
opens on (`scopeFilterTab`), and which names a tab stands for
(`rowMatchesFilterTab`).

## Why this half is parked, and the grouping half is not

The original file was one piece of work with two halves. They are parked for
**different** reasons, and only one of them has been resolved:

| Half | State |
| --- | --- |
| Grouping — ownership claims folded by the DNS record they land in | **Landed 2026-09-28.** `groupOwnershipRecords` is exported from `DeliveryManagement.tsx`, its cases now run at `apps/sdkwork-deployments-pc/tests/delivery-ownership-grouping.test.ts`, and the panel renders one block per record with one value per hostname. |
| Filter tabs — `scopeFilterTab` / `rowMatchesFilterTab` | **Still parked.** No implementation, and a live design question behind it: see below. |

The grouping half was a specification that arrived ahead of its implementation,
and it failed for that reason and no other. It also turned out to be an operator
-reported defect that was still live: on 2026-09-28 an operator reported the
duplicate `_sdkwork-verification` rows again, which is what landed it.

The filter half is different. It is not blocked on code; it is blocked on a
question the same picker already answers the other way:

> `DeliveryManagement.tsx`: "The row is disabled rather than hidden — the operator
> asked which hostnames this root domain has, and an answer that omitted the ones
> the current type cannot take would read as a root domain that is missing them."

A tab that filtered the list down to the certificate type's scope would **hide**
exactly those rows, contradicting that sentence and the per-row refusal tooltips
added on 2026-09-28 (`rowRefusalKey`). One of the two answers has to go:

- land the tabs ⇒ delete `rowRefusalKey` and the "disabled rather than hidden"
  rationale, and accept that a root domain under `WILDCARD` shows nothing but its
  wildcard rows; or
- keep the tooltips ⇒ drop the tabs, and delete this file.

Parked rather than deleted, because it records a requirement nothing else does.
Note also that reactivating it needs more than the two helpers: the picker has no
tab strip today, and a scope change has to re-derive the active tab rather than
leaving a stale one selected.

## The `.pending` suffix is load-bearing

`vitest` runs on its defaults (`**/*.{test,spec}.?(c|m)[jt]s?(x)`) and the app
root has no `vitest.config.ts`, so moving the file into a subdirectory would not
have been enough — it would still be collected. The suffix takes it out of
`vitest`'s and `tsc`'s scan, matching what `tests/pending/` does for the cargo
test targets in `crates/sdkwork-intelligence-deploy-repository-sqlx`.

## Re-activating

Once the row-hiding question is settled, and after adding the helpers and the tab
strip:

```sh
cd apps/sdkwork-deployments-pc/tests
git mv pending/delivery-ownership-filter-tabs.test.ts.pending delivery-ownership-filter-tabs.test.ts
rmdir pending 2>/dev/null || true
```

Then run `pnpm --dir apps/sdkwork-deployments-pc test`.

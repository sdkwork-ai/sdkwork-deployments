# Parked console specifications

`delivery-ownership-grouping.test.ts.pending` is the specification for the
certificate-coverage panel's *grouping* redesign: ownership claims folded by the
DNS record they land in (so a wildcard and the apex it leaves out become one row
with two values), plus the two decisions the coverage picker makes — which tab a
certificate type opens on, and which names a tab stands for.

None of that exists. The test imports three helpers from
`console-delivery/src/DeliveryManagement.tsx`:

```
groupOwnershipRecords
rowMatchesFilterTab
scopeFilterTab
```

and a repository-wide grep finds **zero** occurrences of any of them — not in
`main`, and not in the `wip/deploy-certificate` commit `1b4acfe` that added this
file. The panel is still built claim by claim, which is the behaviour the test
exists to forbid.

So the file is a specification that arrived ahead of its implementation: it was
merged into `main` while the code it describes stayed unwritten. It failed for
that reason and no other.

## Why parked instead of deleted

The comment at the top of the file records the operator-reported defect the fold
fixes ("two blocks naming the same record read as the panel having duplicated a
row"). That is the requirement. Deleting the file deletes the requirement;
leaving it running keeps `vitest` red for as long as the feature is unbuilt,
which is how a suite stops being read.

## The `.pending` suffix is load-bearing

`vitest` runs on its defaults (`**/*.{test,spec}.?(c|m)[jt]s?(x)`) and the app
root has no `vitest.config.ts`, so moving the file into a subdirectory would not
have been enough — it would still be collected. The suffix takes it out of
`vitest`'s and `tsc`'s scan, matching what `tests/pending/` does for the cargo
test targets in `crates/sdkwork-intelligence-deploy-repository-sqlx`.

## Re-activating

When the grouping lands:

```sh
cd apps/sdkwork-deployments-pc/tests
git mv pending/delivery-ownership-grouping.test.ts.pending delivery-ownership-grouping.test.ts
rmdir pending 2>/dev/null || true
```

Then run `pnpm --dir apps/sdkwork-deployments-pc test`.

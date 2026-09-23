# Parked publish-UI specifications

These two files are specifications for a publish-dialog redesign ("v7") that was
**never implemented** — not on `main`, and not on `wip/deploy-certificate`
either. They were merged in ahead of their implementation, so on `main` they
failed for the only reason a spec can fail without a defect: the code they
describe does not exist.

Evidence, so this can be re-checked rather than believed:

```
$ git show 1b4acfe:apps/.../console-delivery/src/DeliveryManagement.tsx | grep -c groupOwnershipRecords
0                      # the same holds for the siblings below — 0 on the wip branch too
$ grep -rn "FieldFeedback\|publish-field-validation" --include=*.tsx --include=*.ts packages/
                       # no production consumer: only these parked specs import them
```

What each file waits on:

| File | Waits on |
| --- | --- |
| `publish-first-step-v7.spec.ts.pending` | `CreateDeployAppDialog.tsx` rewritten to a four-step flow whose first screen is the directory/artifact step; the app-type step removed outright. Today's dialog still opens on the type tiles, so every assertion here is against a screen that does not exist. |
| `publish-field-validation-v7.spec.ts.pending` | The same v7 dialog, plus the `field*` / `publishMissingFields` message keys in `console-publishing/src/i18n.ts`. `service/publish-field-validation.ts` and `components/FieldFeedback.tsx` are landed but have **no consumer**; they are the helpers this dialog is meant to call. |

## Why parked instead of deleted

The specifications are the most precise statement anyone has written of what the
v7 flow must do, and they are referenced by the `wip/deploy-certificate` work
still in flight. Deleting them throws that away; leaving them running keeps
`vitest` permanently red, which is how a suite stops being read.

## The `.pending` suffix is load-bearing

`vitest` here runs on its defaults — `**/*.{test,spec}.?(c|m)[jt]s?(x)` — and
the app root has no `vitest.config.ts`, so a file merely moved into a
subdirectory would still be collected. Appending `.pending` takes these two out
of both `vitest`'s and `tsc`'s scan, exactly as `tests/pending/` does for the
cargo test targets in `crates/sdkwork-intelligence-deploy-repository-sqlx`.

## Re-activating

When the v7 dialog lands:

```sh
cd apps/sdkwork-deployments-pc/packages/sdkwork-deployments-pc-console-publishing/tests
git mv pending/publish-first-step-v7.spec.ts.pending      publish-first-step-v7.spec.ts
git mv pending/publish-field-validation-v7.spec.ts.pending publish-field-validation-v7.spec.ts
rmdir pending 2>/dev/null || true
```

Then run `pnpm --dir apps/sdkwork-deployments-pc test` — the specs are expected
to go green without edits, because they were written against the target design,
not against the current code.

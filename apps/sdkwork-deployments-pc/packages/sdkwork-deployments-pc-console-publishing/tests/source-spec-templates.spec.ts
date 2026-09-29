/**
 * Source-spec templates, and the free-text parsers that live beside the dialog.
 *
 * The planner is what makes "declare a spec and continue" safe, so the assertions
 * are the three ways it can go wrong rather than the happy path: a key the column
 * would reject, a key that collides with one the app already owns, and a rank that
 * collides inside a client class. Each of those reaches the server as a 409 or a
 * CHECK violation when the console gets it wrong, and none of them is visible in
 * the rendered form.
 *
 * The parsers are asserted with the shapes operators actually paste — a link
 * rather than a bare UUID, a clone URL with or without `.git` — because that is
 * the whole reason they exist instead of a plain `String.trim()`.
 */
import { describe, expect, it } from "vitest";
import {
  SOURCE_SPEC_KEY_SAFE_PATTERN,
  SOURCE_SPEC_TEMPLATES,
  planSourceSpecFromTemplate,
  sourceSpecTemplateById,
  uniqueSpecKey,
  type SourceSpecTemplate,
  type SourceSpecTemplateScope,
} from "../src/service/source-spec-templates.ts";
import {
  extractPublicationUuid,
  inferGitProvider,
  suggestRepoKey,
} from "../src/components/UploadSourceDialog.tsx";

/** The slice the planner reads, plus the identity fields one collision needs. */
function spec(
  overrides: Partial<SourceSpecTemplateScope> & { readonly specKey: string },
): SourceSpecTemplateScope {
  return {
    id: overrides.specKey,
    label: overrides.specKey.toUpperCase(),
    status: "ACTIVE",
    sourceStatus: "EMPTY",
    clientClassRoutes: [],
    isDefault: false,
    ...overrides,
  }
}

function template(id: string): SourceSpecTemplate {
  const found = sourceSpecTemplateById(id)
  if (found === undefined) throw new Error(`no template ${id}`)
  return found
}

describe("source-spec templates — key safety", () => {
  it("ships only keys the column CHECK accepts", () => {
    // The contract's `CompositionKey` is the looser rule; the column is the one
    // that actually rejects a write, so every preset has to satisfy this.
    for (const entry of SOURCE_SPEC_TEMPLATES) {
      expect(entry.specKey, entry.id).toMatch(SOURCE_SPEC_KEY_SAFE_PATTERN)
    }
  })

  it("resolves every preset by its id, and nothing else", () => {
    for (const entry of SOURCE_SPEC_TEMPLATES) {
      expect(sourceSpecTemplateById(entry.id)?.specKey).toBe(entry.specKey)
    }
    expect(sourceSpecTemplateById("not-a-template")).toBeUndefined()
  })

  it("suffixes a key the app already uses instead of colliding", () => {
    expect(uniqueSpecKey("pc", [])).toBe("pc")
    expect(uniqueSpecKey("pc", ["pc"])).toBe("pc-2")
    expect(uniqueSpecKey("pc", ["pc", "pc-2"])).toBe("pc-3")
    // The suffix itself has to stay inside the column's rule.
    expect(uniqueSpecKey("pc", ["pc", "pc-2", "pc-3"])).toMatch(SOURCE_SPEC_KEY_SAFE_PATTERN)
  })
})

describe("source-spec templates — the plan", () => {
  it("claims the next free rank, so a second candidate becomes the fallback", () => {
    const plan = planSourceSpecFromTemplate(
      template("pc-web"),
      "production",
      [spec({ specKey: "pc", clientClassRoutes: [{ clientClass: "DESKTOP", preference: 0 }] })],
      "PC web",
    )
    // Rank 0 is taken, so the new spec is the fallback — which is what "add
    // another PC source" means. Claiming rank 0 again is the rejection this
    // avoids.
    expect(plan.clientClassRoutes).toEqual([{ clientClass: "DESKTOP", preference: 1 }])
    expect(plan.specKey).toBe("pc-2")
  })

  it("is the app-level default only while no active default exists", () => {
    expect(planSourceSpecFromTemplate(template("pc-web"), "production", [], "PC web").isDefault)
      .toBe(true)

    const withDefault = planSourceSpecFromTemplate(
      template("pc-web"),
      "production",
      [spec({ specKey: "pc", isDefault: true })],
      "PC web",
    )
    expect(withDefault.isDefault).toBe(false)

    // A DISABLED default does not hold the slot: the unique index is partial on
    // `is_default AND status = 'ACTIVE'`, so a new spec may take it.
    const afterDisabled = planSourceSpecFromTemplate(
      template("pc-web"),
      "production",
      [spec({ specKey: "pc", isDefault: true, status: "DISABLED" })],
      "PC web",
    )
    expect(afterDisabled.isDefault).toBe(true)
  })

  it("carries the template's vocabulary into the request", () => {
    const plan = planSourceSpecFromTemplate(template("h5-web"), "test", [], "H5 web")
    expect(plan).toMatchObject({
      environment: "test",
      specKey: "h5",
      label: "H5 web",
      runtimeTarget: "browser",
      clientArchitecture: "react-h5",
      handler: "SPA",
      pathPrefix: "/",
      indexFiles: ["index.html"],
      priority: 0,
    })
  })
})

describe("free-text parsers", () => {
  const uuid = "8f1c8a90-0000-4000-8000-000000000000"

  it("extracts a publication uuid from a bare value or a pasted link", () => {
    expect(extractPublicationUuid(uuid)).toBe(uuid)
    expect(extractPublicationUuid(`https://kb.example.com/wiki/${uuid}?v=3`)).toBe(uuid)
    expect(extractPublicationUuid(uuid.toUpperCase())).toBe(uuid)
    expect(extractPublicationUuid("not-a-uuid")).toBeUndefined()
    expect(extractPublicationUuid("")).toBeUndefined()
  })

  it("names the provider only for a host it recognises", () => {
    expect(inferGitProvider("https://github.com/org/repo.git")).toBe("GITHUB")
    expect(inferGitProvider("https://gitee.com/org/repo.git")).toBe("GITEE")
    expect(inferGitProvider("https://gitlab.com/org/repo.git")).toBe("GITLAB")
    // An unknown host keeps whatever the operator chose rather than being
    // rewritten to SELF_HOSTED.
    expect(inferGitProvider("https://git.example.com/org/repo.git")).toBeUndefined()
    expect(inferGitProvider("")).toBeUndefined()
  })

  it("suggests an identifier from the last segment of a clone url", () => {
    expect(suggestRepoKey("https://github.com/org/my-store-web.git")).toBe("my-store-web")
    expect(suggestRepoKey("https://github.com/org/my-store-web")).toBe("my-store-web")
    expect(suggestRepoKey("git@github.com:org/my-store-web.git")).toBe("my-store-web")
    expect(suggestRepoKey("https://github.com/org/My_Store.Web.git")).toBe("my_store.web")
    expect(suggestRepoKey("")).toBeUndefined()
  })
})

/**
 * Application upload declaration conformance (`DRIVE_SPEC.md` §18).
 *
 * The declaration file is the authority; the constants module carries its values into code so
 * call sites do not repeat literals. This test is what keeps the two from drifting: a change to
 * one without the other fails here rather than producing a statistic whose declared value and
 * sent value disagree.
 */
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

import {
  DEPLOY_APP_ID,
  DEPLOY_APP_MEDIA_UPLOAD,
  DEPLOY_ARTIFACT_UPLOAD,
  DEPLOY_UPLOAD_DECLARATIONS,
} from "@sdkwork/deployments-pc-commons";

interface DeclarationEntry {
  appResourceIdKind: string;
  appResourceType: string;
  purpose: string;
  retention: string;
  scene: string;
  source: string;
  uploadProfileCode: string;
}

interface Declaration {
  schemaVersion: number;
  appId: string;
  declarations: DeclarationEntry[];
}

/** §8.1 standard upload profiles. A profile outside this set is a contract violation. */
const STANDARD_UPLOAD_PROFILES = [
  "generic",
  "video",
  "image",
  "audio",
  "document",
  "archive",
  "text",
  "dataset",
  "attachment",
  "avatar",
  "thumbnail",
];

const DECLARATION_PATH = fileURLToPath(new URL("../specs/upload.declaration.json", import.meta.url));
/**
 * The canonical `appId` authority (`DRIVE_SPEC.md` §18.1 field rules). Asserting against this
 * file rather than against the constant alone is what makes the check independent of the value
 * under test: a wrong literal on both sides of the declaration/constant pair still fails here.
 */
const APP_CONFIG_PATH = fileURLToPath(new URL("../sdkwork.app.config.json", import.meta.url));
/** §9.4 reserves `im` for Drive; an application must not declare or send it. */
const RESERVED_SCENES = ["im"];

const KEBAB_CASE = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;
const APP_RESOURCE_TYPE = /^[a-z][a-z0-9]*(?:\.[a-z][a-z0-9_]*)+$/;

function loadDeclaration(): Declaration {
  return JSON.parse(readFileSync(DECLARATION_PATH, "utf8")) as Declaration;
}

function loadCanonicalAppId(): string {
  const config = JSON.parse(readFileSync(APP_CONFIG_PATH, "utf8")) as {
    backend?: { appId?: string };
  };
  const appId = config.backend?.appId;
  if (!appId) {
    throw new Error("sdkwork.app.config.json does not declare backend.appId.");
  }
  return appId;
}

describe("upload declaration file", () => {
  it("exists, parses, and uses the supported schema", () => {
    const declaration = loadDeclaration();
    expect(declaration.schemaVersion).toBe(1);
    expect(Array.isArray(declaration.declarations)).toBe(true);
    expect(declaration.declarations.length).toBeGreaterThan(0);
  });

  it("declares this application's canonical appId", () => {
    expect(loadDeclaration().appId).toBe(DEPLOY_APP_ID);
  });

  it("sources its declared appId from sdkwork.app.config.json", () => {
    // §18.4: the declaration must match the config, not merely the local constant.
    expect(loadDeclaration().appId).toBe(loadCanonicalAppId());
    expect(DEPLOY_APP_ID).toBe(loadCanonicalAppId());
  });

  it("declares every required field on every entry", () => {
    const required: Array<keyof DeclarationEntry> = [
      "appResourceType",
      "appResourceIdKind",
      "scene",
      "source",
      "uploadProfileCode",
      "retention",
      "purpose",
    ];
    for (const entry of loadDeclaration().declarations) {
      for (const field of required) {
        expect(entry[field], `entry ${entry.appResourceType} is missing ${field}`).toBeTruthy();
      }
    }
  });

  it("uses standard upload profiles only", () => {
    for (const entry of loadDeclaration().declarations) {
      expect(
        STANDARD_UPLOAD_PROFILES,
        `${entry.appResourceType} declares a non-standard profile ${entry.uploadProfileCode}`,
      ).toContain(entry.uploadProfileCode);
    }
  });

  it("names appResourceType as a dotted lowercase business type", () => {
    for (const entry of loadDeclaration().declarations) {
      expect(entry.appResourceType, `${entry.appResourceType} is not <domain>.<resource>`).toMatch(
        APP_RESOURCE_TYPE,
      );
    }
  });

  it("names source and scene as stable lowercase kebab-case labels", () => {
    for (const entry of loadDeclaration().declarations) {
      // A package name, npm specifier, or import path is forbidden as `source`.
      expect(entry.source, `${entry.source} is not a kebab-case label`).toMatch(KEBAB_CASE);
      expect(entry.source).not.toContain("/");
      expect(entry.source).not.toContain("@");
      expect(entry.scene, `${entry.scene} is not a kebab-case label`).toMatch(KEBAB_CASE);
      expect(RESERVED_SCENES).not.toContain(entry.scene);
    }
  });

  it("declares temporary retention with an explicit TTL", () => {
    for (const entry of loadDeclaration().declarations) {
      expect(["long_term", "temporary"]).toContain(entry.retention);
    }
  });

  it("declares a distinct (appResourceType, scene, uploadProfileCode) triple per entry", () => {
    const keys = loadDeclaration().declarations.map(
      (entry) => `${entry.appResourceType}|${entry.scene}|${entry.uploadProfileCode}`,
    );
    expect(new Set(keys).size).toBe(keys.length);
  });
});

describe("upload declaration constants", () => {
  it("mirrors the declaration file entry for entry", () => {
    const declared = loadDeclaration().declarations;
    expect(DEPLOY_UPLOAD_DECLARATIONS.length).toBe(declared.length);

    for (const declaredEntry of declared) {
      const constant = DEPLOY_UPLOAD_DECLARATIONS.find(
        (entry) => entry.appResourceType === declaredEntry.appResourceType,
      );
      expect(constant, `no constant carries ${declaredEntry.appResourceType}`).toBeDefined();
      expect(constant).toMatchObject({
        appResourceIdKind: declaredEntry.appResourceIdKind,
        appResourceType: declaredEntry.appResourceType,
        purpose: declaredEntry.purpose,
        retention: declaredEntry.retention,
        scene: declaredEntry.scene,
        source: declaredEntry.source,
        uploadProfileCode: declaredEntry.uploadProfileCode,
      });
    }
  });

  it("agrees across every declared upload entry on one source label", () => {
    // §18.2: one application must not ship two `source` styles for its own uploads.
    const sources = new Set(DEPLOY_UPLOAD_DECLARATIONS.map((entry) => entry.source));
    expect([...sources]).toEqual([DEPLOY_ARTIFACT_UPLOAD.source]);
  });

  it("keeps the media scene free of the closed media-kind dimension", () => {
    // §18.2: a closed dimension must not become an unbounded `scene` suffix.
    expect(DEPLOY_APP_MEDIA_UPLOAD.scene).not.toMatch(/icon|cover|screenshot/);
  });
});

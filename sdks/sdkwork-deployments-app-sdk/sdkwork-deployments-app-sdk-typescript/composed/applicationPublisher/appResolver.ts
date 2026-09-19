import type { AppResponse } from '../../generated/server-openapi/src/types';
import { ApplicationPublishError } from './errors';
import type {
  ApplicationPublishAppEvidence,
  ApplicationPublisherDeployClient,
  ResolveOrCreateApplicationPublishApp,
} from './types';

const APP_LOOKUP_PAGE_SIZE = 50;
/**
 * The retired `deploy_site` list endpoint accepted `keyword`/`status`/`siteType`
 * filters; the replacement `GET /app/v3/api/apps` exposes only `page` and
 * `page_size` (see `apis/app-api/deploy/openapi.yaml`). Exact-match resolution
 * therefore has to page through and filter client-side, so it MUST stay bounded:
 * we refuse to scan past this many pages rather than silently returning a
 * partial view that could look like "no match" when the app actually exists.
 */
const APP_LOOKUP_MAX_PAGES = 20;

export async function retrieveApplicationPublishApp(
  client: ApplicationPublisherDeployClient,
  appId: string,
  signal?: AbortSignal,
): Promise<ApplicationPublishAppEvidence> {
  const value = await client.app.retrieve(appId, requestOptions(signal));
  const responseId = requireAppId(value, 'resolveApp');
  if (responseId !== appId) {
    throw new ApplicationPublishError(
      'APP_ID_MISMATCH',
      'resolveApp',
      `Resolved Application id ${responseId} does not match requested Application id ${appId}.`,
    );
  }
  return { id: responseId, resolution: 'existingById', value };
}

export async function findExactApplicationPublishApp(
  client: ApplicationPublisherDeployClient,
  app: ResolveOrCreateApplicationPublishApp,
  signal?: AbortSignal,
): Promise<ApplicationPublishAppEvidence | undefined> {
  const candidates = await listApplicationPublishAppCandidates(client, signal);

  const slug = normalizedOptionalText(app.slug);
  if (slug) {
    const slugMatches = candidates.filter((candidate) => candidate.slug === slug);
    if (slugMatches.length > 1) {
      throw new ApplicationPublishError(
        'APP_RESOLUTION_AMBIGUOUS',
        'resolveApp',
        `Multiple Applications matched exact slug ${slug}.`,
      );
    }
    const match = slugMatches[0];
    if (match) {
      return evidenceFromMatch(match, 'existingBySlug');
    }
  }

  const name = app.name.trim();
  const nameMatches = candidates.filter((candidate) => candidate.name === name);
  if (nameMatches.length > 1) {
    throw new ApplicationPublishError(
      'APP_RESOLUTION_AMBIGUOUS',
      'resolveApp',
      `Multiple Applications matched exact name ${name}.`,
    );
  }
  const match = nameMatches[0];
  return match ? evidenceFromMatch(match, 'existingByName') : undefined;
}

export function createdApplicationPublishAppEvidence(
  value: AppResponse,
): ApplicationPublishAppEvidence {
  return {
    id: requireAppId(value, 'createApp'),
    resolution: 'created',
    value,
  };
}

function evidenceFromMatch(
  value: AppResponse,
  resolution: 'existingBySlug' | 'existingByName',
): ApplicationPublishAppEvidence {
  return {
    id: requireAppId(value, 'resolveApp'),
    resolution,
    value,
  };
}

async function listApplicationPublishAppCandidates(
  client: ApplicationPublisherDeployClient,
  signal?: AbortSignal,
): Promise<AppResponse[]> {
  const candidates: AppResponse[] = [];
  for (let page = 1; page <= APP_LOOKUP_MAX_PAGES; page += 1) {
    const result = await client.app.list(
      { page, pageSize: APP_LOOKUP_PAGE_SIZE },
      requestOptions(signal),
    );
    candidates.push(...result.items);
    if (!result.pageInfo.hasMore) {
      return candidates;
    }
  }
  throw new ApplicationPublishError(
    'APP_RESOLUTION_AMBIGUOUS',
    'resolveApp',
    `Application lookup exceeded the ${APP_LOOKUP_MAX_PAGES}-page bound without reaching the last page.`,
  );
}

function requireAppId(
  value: AppResponse,
  stage: 'resolveApp' | 'createApp',
): string {
  const id = normalizedOptionalText(value.id);
  if (!id) {
    throw new ApplicationPublishError(
      'APP_RESPONSE_MISSING_ID',
      stage,
      'Deploy Application response did not include an Application id.',
    );
  }
  return id;
}

function normalizedOptionalText(value: string | undefined): string | undefined {
  const normalized = value?.trim();
  return normalized || undefined;
}

function requestOptions(signal: AbortSignal | undefined): { signal?: AbortSignal } {
  return signal !== undefined ? { signal } : {};
}

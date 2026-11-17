/** Which level owns an application and therefore who may reach it: `PLATFORM` (platform-internal, tenant-wide readable), `TENANT` (organizationId "0"), `ORGANIZATION` (organizationId > 0), or `USER` (ownerUserId present). */
export type AppOwnerType = 'PLATFORM' | 'TENANT' | 'ORGANIZATION' | 'USER';

/** Which level owns an application, and therefore who may reach it.
`apps.list` is tenant-wide, so without an explicit level a reader cannot tell a platform-operated app from a tenant's shared app from one person's personal app. This is the same question `DomainZoneResponse.scope` answers for domain zones, and it is modelled the same way — an explicit level rather than a flag derived from a nullable column, because every pre-existing `deploy_app` row carries `user_id IS NULL` and a derived flag would classify the whole inventory as platform-owned.
The vocabulary mirrors the cloud-account scopes this service already ships, so the account centre and the app inventory name the levels alike. */
export type AppOwnerType = 'PLATFORM' | 'TENANT' | 'ORGANIZATION' | 'USER';

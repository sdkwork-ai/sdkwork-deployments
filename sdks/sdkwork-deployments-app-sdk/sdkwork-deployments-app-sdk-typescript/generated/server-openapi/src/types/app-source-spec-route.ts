import type { AppClientClass } from './app-client-class';

/** One client class this source spec serves, with its rank in that class's preference order. A class's default spec is its lowest-ranked spec, with `preference` 0 the conventional way to declare one; lower ranks win, so a class whose rank-0 spec has no source uploaded yet falls through to the next rank. Within one (app, environment) a `(clientClass, preference)` pair belongs to exactly one spec, which is what makes the default unambiguous. Ranks may be sparse: a class may be declared at rank 1 before any spec claims rank 0. */
export interface AppSourceSpecRoute {
  clientClass: AppClientClass;
  preference: number;
}

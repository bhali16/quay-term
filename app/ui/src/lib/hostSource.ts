/**
 * Hosts offered after `ssh ` (input assist): ~/.ssh/config aliases and saved sessions.
 * Read lazily and kept for a short while, so typing never waits on the backend: the popup
 * uses what is cached and is told when a fresher list arrives.
 */
import { libraryLoad, sshConfigHosts } from "./ipc";
import { buildHosts, type HostCandidate } from "./assist";

const FRESH_MS = 30_000;

let list: HostCandidate[] = [];
let loadedAt = 0;
let loading: Promise<void> | null = null;

/** The hosts known now. Starts a refresh when the list is old; `onfresh` runs when it lands. */
export function hosts(onfresh?: () => void): HostCandidate[] {
  if (!loading && Date.now() - loadedAt > FRESH_MS) {
    loading = Promise.all([sshConfigHosts().catch(() => []), libraryLoad().then((l) => l.sessions).catch(() => [])])
      .then(([config, saved]) => {
        list = buildHosts(config, saved);
        loadedAt = Date.now();
        onfresh?.();
      })
      .finally(() => (loading = null));
  }
  return list;
}

/** Forget the cache (sessions were added, edited or imported). */
export function staleHosts() {
  loadedAt = 0;
}

import { writable, type Readable } from 'svelte/store';

export type Eip1193Provider = {
  request: (args: { method: string; params?: unknown[] | object }) => Promise<unknown>;
};

export type Eip6963ProviderInfo = {
  uuid: string;
  name: string;
  icon: string;
  rdns: string;
};

export type Eip6963ProviderDetail = {
  info: Eip6963ProviderInfo;
  provider: Eip1193Provider;
};

type AnnounceEvent = CustomEvent<Eip6963ProviderDetail>;

const store = writable<Eip6963ProviderDetail[]>([]);
export const eip6963Providers: Readable<Eip6963ProviderDetail[]> = store;

let started = false;
export function startEip6963Discovery() {
  if (started || typeof window === 'undefined') return;
  started = true;

  const seen = new Map<string, Eip6963ProviderDetail>();
  window.addEventListener('eip6963:announceProvider', (event: Event) => {
    const detail = (event as AnnounceEvent).detail;
    if (!detail?.info?.rdns) return;
    seen.set(detail.info.rdns, detail);
    store.set(Array.from(seen.values()));
  });

  window.dispatchEvent(new Event('eip6963:requestProvider'));
}

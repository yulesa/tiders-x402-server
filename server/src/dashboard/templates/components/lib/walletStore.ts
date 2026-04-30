import { writable } from 'svelte/store';
import { watchAccount, getAccount } from '@wagmi/core';
import { wagmiConfig } from './wagmi';

export type WalletState = {
  status: 'disconnected' | 'connecting' | 'connected' | 'reconnecting';
  address?: `0x${string}`;
  chainId?: number;
};

function initial(): WalletState {
  return { status: 'disconnected' };
}

export const walletStore = writable<WalletState>(initial());

let started = false;
export function startWalletSync() {
  if (started || typeof window === 'undefined') return;
  started = true;
  const sync = () => {
    const a = getAccount(wagmiConfig);
    walletStore.set({ status: a.status, address: a.address, chainId: a.chainId });
  };
  sync();
  watchAccount(wagmiConfig, { onChange: sync });
}

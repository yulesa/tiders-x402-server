import { writable } from 'svelte/store';
import { watchAccount, getAccount } from '@wagmi/core';
import { wagmiConfig, CHAIN } from './wagmi';

export type WalletState = {
  status: 'disconnected' | 'connecting' | 'connected' | 'reconnecting';
  address?: `0x${string}`;
  chainId?: number;
  isWrongChain: boolean;
};

function initial(): WalletState {
  return { status: 'disconnected', isWrongChain: false };
}

export const walletStore = writable<WalletState>(initial());

let started = false;
export function startWalletSync() {
  if (started || typeof window === 'undefined') return;
  started = true;
  const sync = () => {
    const a = getAccount(wagmiConfig);
    walletStore.set({
      status: a.status,
      address: a.address,
      chainId: a.chainId,
      isWrongChain: a.status === 'connected' && a.chainId !== CHAIN.id,
    });
  };
  sync();
  watchAccount(wagmiConfig, { onChange: sync });
}

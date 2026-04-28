// Browser-only wagmi + viem setup. Do not import at module scope from SSR code.
import { createConfig, http, connect, disconnect, getAccount, getWalletClient, switchChain, reconnect } from '@wagmi/core';
import { injected } from '@wagmi/connectors';
import { baseSepolia } from 'viem/chains';
import { publicActions, type WalletClient } from 'viem';

export const CHAIN = baseSepolia;

export const wagmiConfig = createConfig({
  chains: [CHAIN],
  connectors: [injected()],
  transports: { [CHAIN.id]: http() },
});

export async function connectInjected() {
  // Triggers the wallet popup on first call; resolves immediately if already connected.
  const account = getAccount(wagmiConfig);
  if (account.status === 'connected') return account;
  return await connect(wagmiConfig, { connector: injected() });
}

export async function disconnectWallet() {
  await disconnect(wagmiConfig);
}

export async function ensureCorrectChain(): Promise<void> {
  const account = getAccount(wagmiConfig);
  if (account.chainId && account.chainId !== CHAIN.id) {
    await switchChain(wagmiConfig, { chainId: CHAIN.id });
  }
}

// Returns a viem WalletClient extended with publicActions — the exact shape
// @x402/evm's ClientEvmSigner expects.
export async function getX402Signer() {
  const client = await getWalletClient(wagmiConfig, { chainId: CHAIN.id });
  if (!client) throw new Error('Wallet not connected');
  return (client as WalletClient).extend(publicActions);
}

export function tryReconnect() {
  // Fire-and-forget; wagmi persists the last connector in localStorage.
  return reconnect(wagmiConfig).catch(() => {});
}

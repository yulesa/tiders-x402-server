// Browser-only wagmi + viem setup. Do not import at module scope from SSR code.
import { createConfig, http, connect, disconnect, getAccount, getWalletClient, reconnect, readContract, switchChain } from '@wagmi/core';
import { injected, coinbaseWallet } from '@wagmi/connectors';
import { base, baseSepolia, polygon, polygonAmoy, arbitrum, arbitrumSepolia } from 'viem/chains';
import { publicActions, type WalletClient } from 'viem';
import { writable, type Readable } from 'svelte/store';
import { DEFAULT_STABLECOINS } from '@x402/evm';
import type { Eip1193Provider } from './eip6963';

const SUPPORTED_CHAINS = [base, baseSepolia, polygon, polygonAmoy, arbitrum, arbitrumSepolia] as const;
type SupportedChainId = (typeof SUPPORTED_CHAINS)[number]['id'];

const coinbase = coinbaseWallet({ appName: 'Tiders Dashboard' });

export const wagmiConfig = createConfig({
  chains: SUPPORTED_CHAINS,
  connectors: [injected(), coinbase],
  transports: {
    [base.id]: http(),
    [baseSepolia.id]: http(),
    [polygon.id]: http(),
    [polygonAmoy.id]: http(),
    [arbitrum.id]: http(),
    [arbitrumSepolia.id]: http(),
  },
});

export async function connectInjected() {
  const account = getAccount(wagmiConfig);
  if (account.status === 'connected') return account;
  return await connect(wagmiConfig, { connector: injected() });
}

export async function connectEip6963(detail: { info: { rdns: string; name: string }; provider: Eip1193Provider }) {
  const target = injected({
    target: () => ({
      id: detail.info.rdns,
      name: detail.info.name,
      provider: detail.provider as never,
    }),
  });
  return await connect(wagmiConfig, { connector: target });
}

export async function connectCoinbase() {
  const account = getAccount(wagmiConfig);
  if (account.status === 'connected') return account;
  return await connect(wagmiConfig, { connector: coinbase });
}

export async function disconnectWallet() {
  await disconnect(wagmiConfig);
}

export async function getX402Signer() {
  const account = getAccount(wagmiConfig);
  const chainId = account.chainId;
  const client = await getWalletClient(wagmiConfig, chainId ? { chainId } : undefined);
  if (!client) throw new Error('Wallet not connected');
  if (!client.account?.address) throw new Error('Wallet account has no address');
  const extended = (client as WalletClient).extend(publicActions);
  return Object.assign(extended, { address: client.account.address });
}

export function isConnected(): boolean {
  return getAccount(wagmiConfig).status === 'connected';
}

export function tryReconnect() {
  return reconnect(wagmiConfig).catch(() => {});
}

export async function switchToChain(chainId: number): Promise<void> {
  await switchChain(wagmiConfig, { chainId: chainId as SupportedChainId });
}

const BALANCE_ABI = [{ name: 'balanceOf', type: 'function', stateMutability: 'view', inputs: [{ name: 'account', type: 'address' }], outputs: [{ type: 'uint256' }] }] as const;

export type UsdcBalance = { formatted: string; symbol: string } | null;
const usdcBalanceStore = writable<UsdcBalance>(null);
export const usdcBalance: Readable<UsdcBalance> = usdcBalanceStore;

let unwatchBalance: (() => void) | null = null;

export function startUsdcBalanceWatch(address: `0x${string}`, chainId: number) {
  stopUsdcBalanceWatch();

  const stablecoin = DEFAULT_STABLECOINS[`eip155:${chainId}`];
  if (!stablecoin) return;

  const tokenAddress = stablecoin.address as `0x${string}`;

  async function fetch() {
    try {
      const raw = await readContract(wagmiConfig, {
        address: tokenAddress,
        abi: BALANCE_ABI,
        functionName: 'balanceOf',
        args: [address],
        chainId: chainId as SupportedChainId,
      });
      const formatted = (Number(raw) / 10 ** stablecoin.decimals).toFixed(2);
      usdcBalanceStore.set({ formatted, symbol: stablecoin.name });
    } catch {
      // Silently ignore — RPC errors are transient.
    }
  }

  fetch();
  const timer = setInterval(fetch, 60_000);
  unwatchBalance = () => clearInterval(timer);
}

export function stopUsdcBalanceWatch() {
  unwatchBalance?.();
  unwatchBalance = null;
  usdcBalanceStore.set(null);
}

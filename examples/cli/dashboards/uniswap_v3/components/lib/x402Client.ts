import { wrapFetchWithPayment, x402Client } from '@x402/fetch';
import { ExactEvmScheme } from '@x402/evm';
import { getX402Signer, switchToChain, wagmiConfig } from './wagmi';
import { getAccount } from '@wagmi/core';

async function buildFetchWithPayment() {
  const signer = await getX402Signer();
  const scheme = new ExactEvmScheme(signer as Parameters<typeof ExactEvmScheme>[0]);
  const client = new x402Client();
  const { chainId } = getAccount(wagmiConfig);
  if (chainId) client.register(`eip155:${chainId}`, scheme);
  return wrapFetchWithPayment(globalThis.fetch.bind(globalThis), client);
}

function extractChainId(err: unknown): number | null {
  const msg = err instanceof Error ? err.message : String(err);
  const match = msg.match(/eip155:(\d+)/);
  return match ? parseInt(match[1], 10) : null;
}

export async function fetchWithChainSwitch(
  url: string,
  init: RequestInit,
  onSwitching: () => void,
): Promise<Response> {
  const fetchWithPay = await buildFetchWithPayment();
  try {
    return await fetchWithPay(url, init);
  } catch (err) {
    const chainId = extractChainId(err);
    if (!chainId) throw err;

    onSwitching();
    await switchToChain(chainId);

    const retryFetch = await buildFetchWithPayment();
    return retryFetch(url, { ...init, body: JSON.stringify(JSON.parse(init.body as string)) });
  }
}

import { wrapFetchWithPayment, x402Client } from '@x402/fetch';
import { ExactEvmScheme } from '@x402/evm';
import { getX402Signer, CHAIN } from './wagmi';

export async function makeFetchWithPayment() {
  const signer = await getX402Signer();
  const client = new x402Client().register(
    `eip155:${CHAIN.id}`,
    new ExactEvmScheme(signer as Parameters<typeof ExactEvmScheme>[0]),
  );
  return wrapFetchWithPayment(globalThis.fetch.bind(globalThis), client);
}

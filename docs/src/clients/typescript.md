# TypeScript Client

The `typescript-client/` directory contains an example client that demonstrates querying the server with automatic x402 payment handling.

## Dependencies

- [`viem`](https://viem.sh/) -- EVM wallet management and transaction signing
- [`x402-fetch`](https://www.npmjs.com/package/x402-fetch) -- Wraps `fetch()` to handle 402 responses automatically
- [`apache-arrow`](https://www.npmjs.com/package/apache-arrow) -- Parses Arrow IPC responses

## How It Works

The `x402-fetch` library intercepts 402 responses, signs a payment using the provided wallet, and retries the request with the `X-Payment` header. This makes the payment flow transparent to the application code.

## Example

```typescript
import { createWalletClient, http } from "viem";
import { privateKeyToAccount } from "viem/accounts";
import { baseSepolia } from "viem/chains";
import { wrapFetchWithPayment } from "x402-fetch";
import * as arrow from 'apache-arrow';

async function main() {
  // Set up a wallet
  const account = privateKeyToAccount("0xYOUR_PRIVATE_KEY");
  const client = createWalletClient({
    account,
    transport: http(),
    chain: baseSepolia,
  });

  // Wrap fetch with automatic payment handling
  const fetchWithPay = wrapFetchWithPayment(fetch, client);

  // Query the server -- payment is handled automatically
  const response = await fetchWithPay("http://localhost:4021/query", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      query: "SELECT * FROM my_table LIMIT 2"
    })
  });

  // Parse the Arrow IPC response
  const arrayBuffer = await response.arrayBuffer();
  const table = arrow.tableFromIPC(arrayBuffer);

  for (const row of table) {
    console.log(row.toJSON());
  }
}

main();
```

## What Happens Under the Hood

1. `fetchWithPay` sends the POST request normally.
2. The server returns 402 with payment requirements.
3. `x402-fetch` reads the `accepts` array from the response.
4. It constructs a payment authorization using the wallet.
5. It signs the authorization with EIP-712 typed data.
6. It base64-encodes the payment payload and retries with `X-Payment` header.
7. The server verifies, settles, and returns Arrow IPC data.
8. `fetchWithPay` returns the 200 response to your code.

## Network

The example uses Base Sepolia (testnet). For production, change the chain and use a mainnet USDC deployment. The wallet must have sufficient USDC balance to cover the query cost.

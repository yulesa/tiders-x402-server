import { wrapFetchWithPayment, x402Client } from "@x402/fetch";
import { ExactEvmScheme } from "@x402/evm";
import type { ClientEvmSigner } from "@x402/evm";
import { createWalletClient, http } from "viem";
import { privateKeyToAccount } from "viem/accounts";
import { baseSepolia } from "viem/chains";
import * as arrow from 'apache-arrow';
import { config } from "dotenv";
import { resolve, dirname } from "path";
import { fileURLToPath } from "url";

// Load .env from the same directory as this script
const __dirname = dirname(fileURLToPath(import.meta.url));
config({ path: resolve(__dirname, ".env") });

async function main() {
  // Create a viem wallet client and adapt to ClientEvmSigner interface
  const pk = process.env.PK;
  if (!pk) {
    throw new Error("PK environment variable is not set. Copy .env.example to .env and fill in your private key. You can get test USDC at https://faucet.circle.com/ .");
  }
  const account = privateKeyToAccount(pk as `0x${string}`);
  console.log("Account created: ", account.address);
  const walletClient = createWalletClient({
    account,
    chain: baseSepolia,
    transport: http(),
  });
  const signer: ClientEvmSigner = {
    address: account.address,
    signTypedData: (msg) => walletClient.signTypedData(msg as Parameters<typeof walletClient.signTypedData>[0]),
  };

  // Create x402 V2 client with the EVM exact scheme for Base Sepolia
  const client = new x402Client()
    .register("eip155:84532", new ExactEvmScheme(signer));

  // Wrap fetch with x402 payment handling
  const fetchWithPay = wrapFetchWithPayment(fetch, client);

  try {
    // Discover server capabilities via the root endpoint
    const rootResponse = await fetch("http://localhost:4021/");
    console.log("=== Server Info ===");
    console.log(await rootResponse.text());
    console.log("===================\n");

    const sql = "SELECT * FROM uniswap_v3_pool_swap LIMIT 2;";
    const queryUrl = `http://localhost:4021/api/query?query=${encodeURIComponent(sql)}`;

    // First, make a plain fetch to see the 402 payment request response
    const initialResponse = await fetch(queryUrl);
    console.log("=== Initial 402 Response ===");
    console.log("Status:", initialResponse.status);
    console.log("Headers:", Object.fromEntries(initialResponse.headers.entries()));
    const initialBody = await initialResponse.text();
    console.log("Body:", initialBody);
    console.log("============================\n");

    // Make a GET request with the query — x402-fetch handles the payment retry
    const response = await fetchWithPay(queryUrl);

    if (!response.ok) {
      console.log('Response status:', response.status);
      console.log('Response headers:', Object.fromEntries(response.headers.entries()));
      const responseText = await response.text();
      console.log('Response body:', responseText);
      return;
    }

    // Get the response as ArrayBuffer
    const arrayBuffer = await response.arrayBuffer();

    // Parse the Arrow IPC format
    const table = arrow.tableFromIPC(arrayBuffer);

    // Print the data
    console.log("Query Results:");
    console.log("-------------");
    for (const row of table) {
      console.log(row.toJSON());
    }
  } catch (error: unknown) {
    console.error('Error executing query:', error instanceof Error ? error.message : String(error));
    throw error;
  }
}

main().catch((error: unknown) => {
  console.error('Application error:', error instanceof Error ? error.message : String(error));
  process.exit(1);
});

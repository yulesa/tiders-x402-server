import { createWalletClient, http } from "viem";  // https://viem.sh/
import { privateKeyToAccount } from "viem/accounts";
import { baseSepolia } from "viem/chains";
import { wrapFetchWithPayment } from "x402-fetch"; // https://www.npmjs.com/package/x402-fetch
import * as arrow from 'apache-arrow';

async function main() {
  // Create a wallet client
  const account = privateKeyToAccount("0x9ad184158f40ee42b1c2d4de59cab95b1fd968bfd2b32c17b59f4a009b5a7757");
  const client = createWalletClient({
    account,
    transport: http(),
    chain: baseSepolia,
  });

  // Wrap the fetch function with payment handling
  const fetchWithPay = wrapFetchWithPayment(fetch, client);

  try {
    // Make a POST request with the query
    const response = await fetchWithPay("http://localhost:4021/query", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        query: "SELECT * FROM swaps_df LIMIT 2;"
      })
    });

    if (!response.ok) {
      if (response.status !== 200) {
        console.log('402 Response headers:', Object.fromEntries(response.headers.entries()));
        const responseText = await response.text();
        console.log('402 Response body:', responseText);
        console.log(response);
      }
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
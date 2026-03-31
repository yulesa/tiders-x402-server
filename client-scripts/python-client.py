# /// script
# requires-python = ">=3.12"
# dependencies = [
#     "x402[requests,evm]",
#     "pyarrow",
#     "pandas",
#     "python-dotenv",
# ]
# ///

import os
from pathlib import Path
import requests
import pyarrow as pa
from dotenv import load_dotenv
from eth_account import Account
from x402 import x402ClientSync
from x402.mechanisms.evm.exact import ExactEvmScheme
from x402.mechanisms.evm.signers import EthAccountSigner
from x402.http.clients.requests import wrapRequestsWithPayment

# Load .env from the same directory as this script
load_dotenv(Path(__file__).parent / ".env")


def main():
    # Create a signer from private key
    try:
        pk = os.environ["PK"]
    except KeyError:
        print("PK environment variable is not set. Copy .env.example to .env and fill in your private key. You can get test USDC at https://faucet.circle.com/ .")
        return
    account = Account.from_key(pk)
    print("Accounted created: ", account.address)
    signer = EthAccountSigner(account)

    # Create x402 sync client with the EVM exact scheme for Base Sepolia
    client = x402ClientSync()
    client.register("eip155:84532", ExactEvmScheme(signer=signer))

    # Wrap requests session with x402 payment handling
    session = wrapRequestsWithPayment(requests.Session(), client)

    # Discover server capabilities via the root endpoint
    root_response = requests.get("http://localhost:4021/")
    print("=== Server Info ===")
    print(root_response.text)
    print("===================\n")

    query_payload = {"query": "SELECT * FROM uniswap_v3_pool_swap LIMIT 2;"}

    # First, make a plain request to see the 402 payment required response
    initial_response = requests.post(
        "http://localhost:4021/query",
        json=query_payload,
    )
    print("=== Initial 402 Response ===")
    print("Status:", initial_response.status_code)
    print("Headers:", dict(initial_response.headers))
    print("Body:", initial_response.text)
    print("============================\n")

    # Make a request with automatic payment handling
    response = session.post(
        "http://localhost:4021/query",
        json=query_payload,
    )

    if not response.ok:
        print("Response status:", response.status_code)
        print("Response headers:", dict(response.headers))
        print("Response body:", response.text)
        return

    # Parse the Arrow IPC response
    reader = pa.ipc.open_stream(response.content)
    table = reader.read_all()

    # Print the data
    print("Query Results:")
    print("-------------")
    print(table.to_pandas().to_string())


if __name__ == "__main__":
    main()

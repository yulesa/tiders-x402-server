<script lang="ts">
  import { walletStore } from './lib/walletStore';
  import { connectInjected, ensureCorrectChain } from './lib/wagmi';
  import { makeFetchWithPayment } from './lib/x402Client';
  import { arrowBytesToCsv, triggerDownload } from './lib/arrowToCsv';

  export let open = false;
  export let initialSql = '';
  export let filename = 'download.csv';
  export let serverBase = '';   // same-origin: POST /query

  let sql = initialSql;
  $: if (open) { sql = initialSql; status = 'idle'; error = ''; }

  type Status = 'idle' | 'connecting' | 'requesting' | 'signing' | 'decoding' | 'done' | 'error';
  let status: Status = 'idle';
  let error = '';

  function close() { open = false; }

  async function download() {
    error = '';
    try {
      if ($walletStore.status !== 'connected') {
        status = 'connecting';
        await connectInjected();
      }
      await ensureCorrectChain();

      status = 'requesting';
      const fetchWithPay = await makeFetchWithPayment();

      // First unauthenticated attempt is handled inside fetchWithPay: on 402 it
      // constructs the EIP-3009 typed data, prompts the wallet for a signature,
      // and retries with X-PAYMENT. When it returns, we have the final response.
      status = 'signing';
      const res = await fetchWithPay(`${serverBase}/query`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json', Accept: 'application/vnd.apache.arrow.stream' },
        body: JSON.stringify({ sql }),
      });

      if (!res.ok) {
        const body = await res.text();
        throw new Error(`Server returned ${res.status}: ${body.slice(0, 200)}`);
      }

      status = 'decoding';
      const buf = new Uint8Array(await res.arrayBuffer());
      const csv = await arrowBytesToCsv(buf);
      triggerDownload(csv, filename);
      status = 'done';
    } catch (e) {
      error = (e as Error).message ?? String(e);
      status = 'error';
    }
  }
</script>

{#if open}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
       on:click|self={close}
       role="presentation">
    <div class="bg-slate-900 border border-slate-700 rounded-lg shadow-xl w-[min(720px,92vw)] max-h-[90vh] overflow-auto">
      <div class="flex items-center justify-between px-5 py-3 border-b border-slate-700">
        <h3 class="text-base font-semibold text-slate-100 m-0">Download paid data</h3>
        <button on:click={close} class="text-slate-400 hover:text-slate-100 text-xl leading-none">×</button>
      </div>

      <div class="p-5 space-y-4">
        <p class="text-sm text-slate-300 m-0">
          Edit the query below if you'd like. Clicking Download will prompt your wallet to
          authorize a USDC payment on Base Sepolia.
        </p>

        <label class="block">
          <span class="block text-xs uppercase tracking-wider text-slate-400 mb-1">SQL</span>
          <textarea
            bind:value={sql}
            class="w-full h-44 rounded bg-slate-950 border border-slate-700 p-3 font-mono text-xs text-slate-100 focus:outline-none focus:border-blue-500"
            spellcheck="false"
          />
        </label>

        {#if status === 'error'}
          <div class="px-3 py-2 rounded bg-red-950/50 border border-red-700 text-sm text-red-200">
            {error}
          </div>
        {:else if status === 'done'}
          <div class="px-3 py-2 rounded bg-emerald-950/50 border border-emerald-700 text-sm text-emerald-200">
            Downloaded {filename}.
          </div>
        {:else if status !== 'idle'}
          <div class="px-3 py-2 rounded bg-slate-800 border border-slate-600 text-sm text-slate-200">
            {#if status === 'connecting'}Connecting wallet…
            {:else if status === 'requesting'}Fetching payment requirements…
            {:else if status === 'signing'}Check your wallet for a signature request…
            {:else if status === 'decoding'}Decoding response…
            {/if}
          </div>
        {/if}
      </div>

      <div class="flex justify-end gap-2 px-5 py-3 border-t border-slate-700 bg-slate-900/60">
        <button on:click={close}
                class="px-4 py-2 rounded border border-slate-600 text-slate-200 hover:bg-slate-800">
          Cancel
        </button>
        <button on:click={download}
                disabled={status === 'connecting' || status === 'requesting' || status === 'signing' || status === 'decoding'}
                class="px-4 py-2 rounded bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-50">
          {status === 'done' || status === 'idle' || status === 'error' ? 'Download' : 'Working…'}
        </button>
      </div>
    </div>
  </div>
{/if}

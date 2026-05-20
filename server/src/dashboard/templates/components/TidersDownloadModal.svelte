<script lang="ts">
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  import { walletStore, startWalletSync } from './lib/walletStore';
  import { tryReconnect, isConnected } from './lib/wagmi';
  import { fetchWithChainSwitch } from './lib/x402Client';
  import { arrowBytesToCsv, triggerDownload } from './lib/arrowToCsv';
  import WalletPicker from './WalletPicker.svelte';

  onMount(() => {
    startWalletSync();
    tryReconnect();
  });

  export let open = false;
  export let initialSql = '';
  export let filename = 'download.csv';
  export let serverBase = '';

  let sql = initialSql;
  let helpOpen = false;
  $: if (open) { sql = initialSql; status = 'idle'; error = ''; }

  type Status = 'idle' | 'connecting' | 'requesting' | 'switching' | 'signing' | 'decoding' | 'done' | 'error';
  let status: Status = 'idle';
  let error = '';
  let pickerOpen = false;

  function close() { open = false; }

  function awaitConnection(): Promise<void> {
    return new Promise((resolve, reject) => {
      pickerOpen = true;
      const unsub = walletStore.subscribe((w) => {
        if (w.status === 'connected') {
          unsub();
          resolve();
        }
      });
      const poll = setInterval(() => {
        if (!pickerOpen) {
          clearInterval(poll);
          unsub();
          if (get(walletStore).status !== 'connected') {
            reject(new Error('Wallet connection cancelled'));
          }
        }
      }, 150);
    });
  }

  async function download() {
    error = '';
    try {
      if (!isConnected()) {
        status = 'connecting';
        await awaitConnection();
      }

      status = 'requesting';
      const res = await fetchWithChainSwitch(
        `${serverBase}/api/query?query=${encodeURIComponent(sql)}`,
        {
          method: 'GET',
          headers: { Accept: 'application/vnd.apache.arrow.stream' },
        },
        () => { status = 'switching'; },
      );

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
    <div class="bg-base-100 border border-base-300 rounded-lg shadow-xl w-[min(720px,92vw)] h-[85dvh] sm:h-auto max-h-[85dvh] sm:max-h-[85vh] sm:min-h-[32rem] flex flex-col">
      <div class="flex items-center justify-between px-5 py-3 border-b border-base-300 shrink-0">
        <h3 class="text-sm font-semibold text-base-content m-0">Download data</h3>
        <button on:click={close} class="text-base-content-muted hover:text-base-content text-xl leading-none">×</button>
      </div>

      <div class="p-5 space-y-4 overflow-auto flex-1">
        <p class="text-xs text-base-content-muted m-0">
          Edit the query below if you'd like. Clicking Download will prompt your wallet to
          authorize a USDC payment, if necessary.
        </p>

        <label class="block">
          <span class="block text-xs uppercase tracking-wider text-base-content-muted mb-1">SQL</span>
          <textarea
            bind:value={sql}
            class="w-full h-44 rounded bg-base-200 border border-base-300 p-3 font-mono text-xs text-base-content focus:outline-none focus:border-base-content-muted"
            spellcheck="false"
          />
        </label>

        <div class="rounded border border-base-300 bg-base-200">
          <button
            type="button"
            on:click={() => (helpOpen = !helpOpen)}
            aria-expanded={helpOpen}
            class="w-full flex items-center justify-between px-3 py-2 text-xs font-medium text-base-content hover:bg-base-300/40"
          >
            <span>How do I query?</span>
            <span class="text-base-content-muted">{helpOpen ? '−' : '+'}</span>
          </button>
          {#if helpOpen}
            <div class="px-3 pb-3 pt-1 text-xs text-base-content-muted space-y-2 border-t border-base-300">
              <p class="m-0">
                Queries run against a restricted SQL dialect ("Simplified SQL"). You might be billed in USDC
                based on the estimated number of rows the query scans, so prefer tight <code class="font-mono">WHERE</code>
                clauses and <code class="font-mono">LIMIT</code>.
              </p>
              <div>
                <div class="font-semibold text-base-content mb-1">Supported</div>
                <ul class="list-disc pl-4 space-y-0.5 m-0">
                  <li><code class="font-mono">SELECT *</code> or named columns, with optional <code class="font-mono">AS</code> aliases</li>
                  <li>Single-table <code class="font-mono">FROM</code></li>
                  <li><code class="font-mono">WHERE</code> with <code class="font-mono">=, !=, &lt;, &gt;, &lt;=, &gt;=</code>, <code class="font-mono">AND/OR/NOT</code>, <code class="font-mono">BETWEEN</code>, <code class="font-mono">IN</code>, <code class="font-mono">LIKE/ILIKE</code>, <code class="font-mono">IS NULL</code></li>
                  <li>Casts (<code class="font-mono">CAST</code>, <code class="font-mono">::</code>), <code class="font-mono">EXTRACT</code>, <code class="font-mono">AT TIME ZONE</code></li>
                  <li>String: <code class="font-mono">SUBSTRING</code>, <code class="font-mono">TRIM</code>, <code class="font-mono">POSITION</code>, <code class="font-mono">OVERLAY</code>; math: <code class="font-mono">CEIL</code>, <code class="font-mono">FLOOR</code></li>
                  <li><code class="font-mono">ORDER BY col [ASC|DESC] [NULLS FIRST|LAST]</code></li>
                  <li><code class="font-mono">LIMIT n OFFSET n</code></li>
                </ul>
              </div>
              <div>
                <div class="font-semibold text-base-content mb-1">Not supported</div>
                <ul class="list-disc pl-4 space-y-0.5 m-0">
                  <li>Joins, subqueries, CTEs (<code class="font-mono">WITH</code>)</li>
                  <li>Aggregations (<code class="font-mono">GROUP BY</code>, <code class="font-mono">COUNT</code>, <code class="font-mono">SUM</code>, …) and <code class="font-mono">DISTINCT</code></li>
                  <li>Window functions, <code class="font-mono">UNION/INTERSECT/EXCEPT</code></li>
                  <li>Computed columns or function calls in <code class="font-mono">SELECT</code></li>
                </ul>
              </div>
              <p class="m-0">
                Full reference:
                <a
                  href="https://yulesa.github.io/tiders-x402-server/api/endpoints.html"
                  target="_blank"
                  rel="noopener noreferrer"
                  class="underline hover:text-base-content"
                >API endpoints</a>
                ·
                <a
                  href="https://yulesa.github.io/tiders-x402-server/server/sql-parser.html"
                  target="_blank"
                  rel="noopener noreferrer"
                  class="underline hover:text-base-content"
                >SQL Parser docs</a>.
              </p>
            </div>
          {/if}
        </div>

        {#if status === 'error'}
          <div class="px-3 py-2 rounded border border-red-300 bg-red-50 text-xs text-red-700">
            {error}
          </div>
        {:else if status === 'done'}
          <div class="px-3 py-2 rounded border border-green-300 bg-green-50 text-xs text-green-700">
            Downloaded {filename}.
          </div>
        {:else if status !== 'idle'}
          <div class="px-3 py-2 rounded border border-base-300 bg-base-200 text-xs text-base-content-muted">
            {#if status === 'connecting'}Connecting wallet…
            {:else if status === 'requesting'}Fetching payment requirements…
            {:else if status === 'switching'}Check your wallet to switch chains…
            {:else if status === 'signing'}Check your wallet for a signature request…
            {:else if status === 'decoding'}Decoding response…
            {/if}
          </div>
        {/if}
      </div>

      <div class="flex justify-end gap-2 px-5 py-3 border-t border-base-300 shrink-0">
        <button on:click={close}
                class="rounded-md shadow-sm h-8 border border-base-300 flex items-center px-3 text-xs font-medium bg-base-100 hover:bg-base-200">
          Cancel
        </button>
        <button on:click={download}
                disabled={status === 'connecting' || status === 'requesting' || status === 'switching' || status === 'signing' || status === 'decoding'}
                class="rounded-md shadow-sm h-8 border border-base-300 flex items-center px-3 text-xs font-medium bg-base-100 hover:bg-base-200 disabled:opacity-50">
          {status === 'done' || status === 'idle' || status === 'error' ? 'Download' : 'Working…'}
        </button>
      </div>
    </div>
  </div>

  <WalletPicker bind:open={pickerOpen} />
{/if}

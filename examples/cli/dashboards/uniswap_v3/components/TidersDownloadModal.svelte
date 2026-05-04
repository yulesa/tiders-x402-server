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
  export let serverBase = '';   // same-origin: POST /query

  let sql = initialSql;
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
        `${serverBase}/api/query`,
        {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', Accept: 'application/vnd.apache.arrow.stream' },
          body: JSON.stringify({ query: sql }),
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
    <div class="bg-base-100 border border-base-300 rounded-lg shadow-xl w-[min(720px,92vw)] max-h-[90vh] overflow-auto">
      <div class="flex items-center justify-between px-5 py-3 border-b border-base-300">
        <h3 class="text-sm font-semibold text-base-content m-0">Download data</h3>
        <button on:click={close} class="text-base-content-muted hover:text-base-content text-xl leading-none">×</button>
      </div>

      <div class="p-5 space-y-4">
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

      <div class="flex justify-end gap-2 px-5 py-3 border-t border-base-300">
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

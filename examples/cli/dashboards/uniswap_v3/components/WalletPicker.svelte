<script lang="ts">
  import { onMount } from 'svelte';
  import { eip6963Providers, startEip6963Discovery, type Eip6963ProviderDetail } from './lib/eip6963';
  import { connectEip6963, connectCoinbase, connectInjected } from './lib/wagmi';

  export let open = false;

  let err = '';
  let busy: string | null = null;

  onMount(() => {
    startEip6963Discovery();
  });

  function close() {
    open = false;
    err = '';
    busy = null;
  }

  async function pickEip6963(detail: Eip6963ProviderDetail) {
    err = '';
    busy = detail.info.rdns;
    try {
      await connectEip6963(detail);
      close();
    } catch (e) {
      err = (e as Error).message ?? String(e);
    } finally {
      busy = null;
    }
  }

  async function pickCoinbase() {
    err = '';
    busy = 'coinbase';
    try {
      await connectCoinbase();
      close();
    } catch (e) {
      err = (e as Error).message ?? String(e);
    } finally {
      busy = null;
    }
  }

  async function pickFallbackInjected() {
    err = '';
    busy = 'injected';
    try {
      await connectInjected();
      close();
    } catch (e) {
      err = (e as Error).message ?? String(e);
    } finally {
      busy = null;
    }
  }

  $: hasInjected = typeof window !== 'undefined' && (window as unknown as { ethereum?: unknown }).ethereum;
</script>

{#if open}
  <div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60"
       on:click|self={close}
       role="presentation">
    <div class="bg-base-100 border border-base-300 rounded-lg shadow-xl w-[min(420px,92vw)]">
      <div class="flex items-center justify-between px-5 py-3 border-b border-base-300">
        <h3 class="text-sm font-semibold text-base-content m-0">Connect a wallet</h3>
        <button on:click={close} class="text-base-content-muted hover:text-base-content text-xl leading-none">×</button>
      </div>

      <div class="p-3 space-y-1">
        {#each $eip6963Providers as detail (detail.info.uuid)}
          <button
            on:click={() => pickEip6963(detail)}
            disabled={busy !== null}
            class="w-full flex items-center gap-3 px-3 py-2 rounded border border-base-300 bg-base-100 hover:bg-base-200 disabled:opacity-50 text-left text-xs font-medium text-base-content"
          >
            <img src={detail.info.icon} alt="" class="w-6 h-6 rounded" />
            <span class="flex-1">{detail.info.name}</span>
            {#if busy === detail.info.rdns}
              <span class="text-base-content-muted">Connecting…</span>
            {/if}
          </button>
        {/each}

        <button
          on:click={pickCoinbase}
          disabled={busy !== null}
          class="w-full flex items-center gap-3 px-3 py-2 rounded border border-base-300 bg-base-100 hover:bg-base-200 disabled:opacity-50 text-left text-xs font-medium text-base-content"
        >
          <span class="w-6 h-6 rounded bg-blue-600 flex items-center justify-center text-white text-xs font-bold shrink-0">CB</span>
          <span class="flex-1">Coinbase Wallet</span>
          {#if busy === 'coinbase'}
            <span class="text-base-content-muted">Connecting…</span>
          {/if}
        </button>

        {#if $eip6963Providers.length === 0 && hasInjected}
          <button
            on:click={pickFallbackInjected}
            disabled={busy !== null}
            class="w-full flex items-center gap-3 px-3 py-2 rounded border border-base-300 bg-base-100 hover:bg-base-200 disabled:opacity-50 text-left text-xs font-medium text-base-content"
          >
            <span class="w-6 h-6 rounded border border-base-300 flex items-center justify-center text-xs shrink-0">…</span>
            <span class="flex-1">Browser wallet</span>
          </button>
        {/if}
      </div>

      {#if err}
        <div class="mx-3 mb-3 px-3 py-2 rounded border border-red-300 bg-red-50 text-xs text-red-700">
          {err}
        </div>
      {/if}

      <div class="px-5 py-3 border-t border-base-300 text-xs text-base-content-muted">
        Don't see your wallet? Install its browser extension and reload.
      </div>
    </div>
  </div>
{/if}

<script lang="ts">
  import { onMount } from 'svelte';
  import { eip6963Providers, startEip6963Discovery, type Eip6963ProviderDetail } from './lib/eip6963';
  import { connectEip6963, connectInjected } from './lib/wagmi';

  export let open = false;

  let err = '';
  let busy: string | null = null;

  const SUGGESTED_WALLETS = [
    {
      name: 'Rainbow',
      popular: true,
      url: 'https://rainbow.me/download',
      iconBg: 'conic-gradient(from 180deg, #FF4000, #FF9901, #FFF700, #00FF47, #00FFFF, #0047FF, #8B00FF, #FF4000)',
      iconColor: '#fff',
    },
    {
      name: 'Rabby',
      popular: true,
      url: 'https://rabby.io/',
      iconBg: '#7084FF',
      iconColor: '#fff',
    },
    {
      name: 'MetaMask',
      popular: true,
      url: 'https://metamask.io/download/',
      iconBg: '#E8831D',
      iconColor: '#fff',
    },
    {
      name: 'Coinbase Wallet',
      popular: false,
      url: 'https://www.coinbase.com/wallet/downloads',
      iconBg: '#0052FF',
      iconColor: '#fff',
    },
  ] as const;

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
  $: noWallet = $eip6963Providers.length === 0 && !hasInjected;
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
        {#if noWallet}
          <div class="pb-1 pt-2 flex items-center justify-between px-1">
            <span class="text-xs font-semibold text-base-content">Get a wallet</span>
            <span class="text-[10px] text-base-content-muted">Not sure? <a href="https://ethereum.org/en/wallets/" target="_blank" rel="noopener noreferrer" class="underline hover:text-base-content">Learn more</a></span>
          </div>
          {#each SUGGESTED_WALLETS as wallet}
            <a
              href={wallet.url}
              target="_blank"
              rel="noopener noreferrer"
              class="w-full flex items-center gap-3 px-3 py-2 rounded border border-base-300 bg-base-100 hover:bg-base-200 text-left text-xs font-medium text-base-content no-underline"
            >
              <span
                class="w-7 h-7 rounded-xl shrink-0 flex items-center justify-center text-[11px] font-bold"
                style="background:{wallet.iconBg}; color:{wallet.iconColor};"
              >
                {wallet.name[0]}
              </span>
              <span class="flex-1">{wallet.name}</span>
              {#if wallet.popular}
                <span class="text-[10px] px-1.5 py-0.5 rounded-full bg-base-300 text-base-content-muted font-normal">Popular</span>
              {/if}
              <svg class="w-3.5 h-3.5 text-base-content-muted shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
                <path stroke-linecap="round" stroke-linejoin="round" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
              </svg>
            </a>
          {/each}
        {/if}
      </div>

      {#if err}
        <div class="mx-3 mb-3 px-3 py-2 rounded border border-red-300 bg-red-50 text-xs text-red-700">
          {err}
        </div>
      {/if}

      <div class="px-5 py-3 border-t border-base-300 text-xs text-base-content-muted">
        {#if noWallet}
          Install a wallet extension, reload the page, then connect.
        {:else}
          Don't see your wallet? Install its browser extension and reload.
        {/if}
      </div>
    </div>
  </div>
{/if}

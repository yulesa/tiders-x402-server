<script lang="ts">
  import { onMount } from 'svelte';
  import { walletStore, startWalletSync } from './lib/walletStore';
  import { disconnectWallet, tryReconnect, usdcBalance, startUsdcBalanceWatch, stopUsdcBalanceWatch } from './lib/wagmi';
  import WalletPicker from './WalletPicker.svelte';

  let err = '';
  let pickerOpen = false;
  let menuOpen = false;
  let compactRoot: HTMLDivElement;
  let errTimer: ReturnType<typeof setTimeout> | null = null;

  $: if (err) {
    if (errTimer) clearTimeout(errTimer);
    errTimer = setTimeout(() => { err = ''; errTimer = null; }, 4000);
  }

  onMount(() => {
    startWalletSync();
    tryReconnect();
  });

  $: if ($walletStore.status === 'connected' && $walletStore.address && $walletStore.chainId) {
    startUsdcBalanceWatch($walletStore.address, $walletStore.chainId);
  } else {
    stopUsdcBalanceWatch();
  }

  async function onDisconnect() {
    err = '';
    menuOpen = false;
    try { await disconnectWallet(); } catch (e) { err = (e as Error).message; }
  }

  function short(a?: string) {
    return a ? `${a.slice(0, 6)}…${a.slice(-4)}` : '';
  }

  function onCompactClick() {
    if ($walletStore.status === 'connected') {
      menuOpen = !menuOpen;
    } else {
      pickerOpen = true;
    }
  }

  function onDocClick(e: MouseEvent) {
    if (menuOpen && compactRoot && !compactRoot.contains(e.target as Node)) {
      menuOpen = false;
    }
  }
</script>

<svelte:window on:click={onDocClick} />

<!-- Full variant: shown on sm+ screens -->
<div class="hidden sm:inline-flex items-center gap-2 text-sm">
  {#if $walletStore.status === 'connected'}
    <span class="rounded-md shadow-sm h-8 border border-base-300 flex items-center px-3 text-xs font-medium bg-base-100">
      {short($walletStore.address)}
    </span>
    {#if $usdcBalance}
      <span class="rounded-md shadow-sm h-8 border border-base-300 flex items-center px-3 text-xs font-medium bg-base-100">
        {$usdcBalance.formatted} {$usdcBalance.symbol}
      </span>
    {/if}
    <button
      on:click={onDisconnect}
      class="rounded-md shadow-sm h-8 border border-base-300 flex items-center px-3 text-xs font-medium bg-base-100 hover:bg-base-200"
    >
      Disconnect
    </button>
  {:else}
    <button
      on:click={() => (pickerOpen = true)}
      disabled={$walletStore.status === 'connecting'}
      class="rounded-md shadow-sm h-8 border border-base-300 flex items-center px-3 text-xs font-medium bg-base-100 hover:bg-base-200 disabled:opacity-50"
    >
      {$walletStore.status === 'connecting' ? 'Connecting…' : 'Connect wallet'}
    </button>
  {/if}
</div>

<!-- Compact variant: shown on small screens -->
<div bind:this={compactRoot} class="relative inline-flex sm:hidden">
  <button
    on:click={onCompactClick}
    disabled={$walletStore.status === 'connecting'}
    aria-label={$walletStore.status === 'connected' ? 'Wallet menu' : 'Connect wallet'}
    aria-expanded={menuOpen}
    class="rounded-md shadow-sm h-8 w-8 border border-base-300 flex items-center justify-center bg-base-100 hover:bg-base-200 disabled:opacity-50"
  >
    <svg class="w-4 h-4 text-base-content" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2">
      <path stroke-linecap="round" stroke-linejoin="round" d="M3 7h13a2 2 0 012 2v8a2 2 0 01-2 2H5a2 2 0 01-2-2V7zm0 0V6a2 2 0 012-2h11M17 12h.01" />
    </svg>
  </button>

  {#if menuOpen && $walletStore.status === 'connected'}
    <div class="absolute right-0 top-full mt-1 w-56 rounded-md shadow-lg border border-base-300 bg-base-100 z-50 p-2 space-y-1">
      <div class="px-2 py-1.5 text-[10px] uppercase tracking-wide text-base-content-muted">Address</div>
      <div class="px-2 pb-2 text-xs font-medium text-base-content break-all">{short($walletStore.address)}</div>
      {#if $usdcBalance}
        <div class="px-2 py-1.5 text-[10px] uppercase tracking-wide text-base-content-muted border-t border-base-300">Balance</div>
        <div class="px-2 pb-2 text-xs font-medium text-base-content">{$usdcBalance.formatted} {$usdcBalance.symbol}</div>
      {/if}
      <button
        on:click={onDisconnect}
        class="w-full rounded border border-base-300 px-2 py-1.5 text-xs font-medium bg-base-100 hover:bg-base-200 text-left"
      >
        Disconnect
      </button>
    </div>
  {/if}
</div>

{#if err}
  <div class="fixed top-14 right-4 z-[70] max-w-xs rounded-md border border-red-300 bg-red-50 shadow-lg px-3 py-2 text-xs text-red-700 flex items-start gap-2">
    <span class="flex-1 break-words">{err}</span>
    <button on:click={() => (err = '')} class="text-red-700 hover:text-red-900 leading-none" aria-label="Dismiss">×</button>
  </div>
{/if}

<WalletPicker bind:open={pickerOpen} />

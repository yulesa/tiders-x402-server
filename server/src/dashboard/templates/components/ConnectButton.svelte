<script lang="ts">
  import { onMount } from 'svelte';
  import { walletStore, startWalletSync } from './lib/walletStore';
  import { disconnectWallet, tryReconnect, usdcBalance, startUsdcBalanceWatch, stopUsdcBalanceWatch } from './lib/wagmi';
  import WalletPicker from './WalletPicker.svelte';

  let err = '';
  let pickerOpen = false;

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
    try { await disconnectWallet(); } catch (e) { err = (e as Error).message; }
  }

  function short(a?: string) {
    return a ? `${a.slice(0, 6)}…${a.slice(-4)}` : '';
  }
</script>

<div class="inline-flex items-center gap-2 text-sm">
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

{#if err}
  <p class="text-xs text-red-400 mt-1">{err}</p>
{/if}

<WalletPicker bind:open={pickerOpen} />

<script lang="ts">
  import { onMount } from 'svelte';
  import { walletStore, startWalletSync } from './lib/walletStore';
  import { connectInjected, disconnectWallet, ensureCorrectChain, tryReconnect } from './lib/wagmi';

  let err = '';

  onMount(() => {
    startWalletSync();
    tryReconnect();
  });

  async function onConnect() {
    err = '';
    try {
      await connectInjected();
      await ensureCorrectChain();
    } catch (e) {
      err = (e as Error).message ?? String(e);
    }
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
    {#if $walletStore.isWrongChain}
      <button
        on:click={ensureCorrectChain}
        class="px-3 py-1.5 rounded bg-amber-600 text-white hover:bg-amber-700"
      >
        Switch to Base Sepolia
      </button>
    {:else}
      <span class="px-3 py-1.5 rounded bg-slate-700 text-slate-100 font-mono">
        {short($walletStore.address)}
      </span>
    {/if}
    <button
      on:click={onDisconnect}
      class="px-3 py-1.5 rounded border border-slate-600 text-slate-300 hover:bg-slate-800"
    >
      Disconnect
    </button>
  {:else}
    <button
      on:click={onConnect}
      disabled={$walletStore.status === 'connecting'}
      class="px-4 py-1.5 rounded bg-blue-600 text-white hover:bg-blue-700 disabled:opacity-50"
    >
      {$walletStore.status === 'connecting' ? 'Connecting…' : 'Connect wallet'}
    </button>
  {/if}
</div>

{#if err}
  <p class="text-xs text-red-400 mt-1">{err}</p>
{/if}

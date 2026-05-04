<script lang="ts">
  import TidersDownloadModal from './TidersDownloadModal.svelte';

  export let query: string;
  export let label = 'Download';
  export let filename = 'download.csv';
  export let serverBase = '';

  let open = false;
</script>

<button
  on:click={() => (open = true)}
  class="rounded-md shadow-sm h-8 border border-base-300 flex items-center px-3 text-xs font-medium bg-base-100 hover:bg-base-200"
>
  {label}
</button>

<TidersDownloadModal bind:open initialSql={query} {filename} {serverBase} />

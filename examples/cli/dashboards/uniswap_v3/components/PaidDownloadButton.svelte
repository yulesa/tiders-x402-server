<script lang="ts">
  import PaidDownloadModal from './PaidDownloadModal.svelte';

  export let sql: string;
  export let label = 'Download (paid)';
  export let filename = 'download.csv';
  export let serverBase = '';

  let open = false;
</script>

<button
  on:click={() => (open = true)}
  class="px-4 py-2 rounded bg-blue-600 text-white font-medium hover:bg-blue-700"
>
  {label}
</button>

<PaidDownloadModal bind:open initialSql={sql} {filename} {serverBase} />

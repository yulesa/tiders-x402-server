<script>
  import '@evidence-dev/tailwind/fonts.css';
  import '../app.css';
  import { EvidenceDefaultLayout } from '@evidence-dev/core-components';
  import ConnectButton from '../components/ConnectButton.svelte';
  export let data;
</script>

<EvidenceDefaultLayout {data} title="{{NAME}}">
  <slot slot="content" />
</EvidenceDefaultLayout>

<!-- Floats ConnectButton at the top-right, sitting inside the header strip. -->
<div class="tiders-connect-slot">
  <ConnectButton />
</div>

<style>
  .tiders-connect-slot {
    position: fixed;
    top: 0.6rem;
    right: 1.25rem;
    z-index: 60;
    pointer-events: auto;
  }
</style>

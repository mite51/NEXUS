<script lang="ts">
  export let progress: number = 0; // 0 to 1
  export let label: string = '';
</script>

{#if progress > 0}
  <div class="progress-container">
    {#if label}
      <div class="progress-label">{label}</div>
    {/if}
    <div class="progress-track">
      <div class="progress-fill" style="width: {Math.min(progress * 100, 100)}%"></div>
    </div>
    <div class="progress-pct">{Math.round(progress * 100)}%</div>
  </div>
{/if}

<style>
  .progress-container {
    display: flex; align-items: center; gap: 10px;
    padding: 8px 0;
  }
  .progress-label {
    font-size: 12px; color: var(--text-secondary);
    white-space: nowrap;
  }
  .progress-track {
    flex: 1; height: 6px; background: var(--border);
    border-radius: 3px; overflow: hidden; min-width: 100px;
  }
  .progress-fill {
    height: 100%; background: var(--accent);
    border-radius: 3px;
    transition: width 0.3s ease;
  }
  .progress-pct {
    font-size: 11px; color: var(--text-secondary);
    font-family: 'JetBrains Mono', monospace;
    min-width: 36px; text-align: right;
  }
</style>

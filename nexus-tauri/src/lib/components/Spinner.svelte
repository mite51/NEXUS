<script lang="ts">
  export let size: number = 20;
  export let color: string = 'var(--accent)';
</script>

<svg width={size} height={size} viewBox="0 0 24 24" class="spinner">
  <circle cx="12" cy="12" r="10" fill="none" stroke={color} stroke-width="3"
          stroke-dasharray="31.4 31.4" stroke-linecap="round" />
</svg>

<style>
  .spinner { animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>

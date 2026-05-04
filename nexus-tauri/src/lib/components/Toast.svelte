<script lang="ts">
  export let message: string;
</script>

<div class="toast">{message}</div>

<style>
  .toast {
    position: fixed;
    top: 16px; right: 16px;
    background: var(--surface);
    border: 1px solid var(--success);
    border-radius: 8px;
    padding: 12px 20px;
    font-size: 14px;
    z-index: 1000;
    animation: slideIn 0.2s ease-out;
  }
  @keyframes slideIn {
    from { transform: translateX(100%); opacity: 0; }
    to { transform: translateX(0); opacity: 1; }
  }
</style>

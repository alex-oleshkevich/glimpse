<template>
  <p class="compositors" title="This applet reads workspaces, windows or outputs from the compositor">
    <span class="compositors__label">Works with</span>
    <span class="compositors__name">niri</span>
    <span class="compositors__name">Hyprland</span>
  </p>
</template>

<style scoped>
.compositors {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
  margin: 4px 0 12px;
  font-size: 13px;
}
.compositors__label {
  color: var(--vp-c-text-2);
}
.compositors__name {
  padding: 1px 8px;
  border-radius: 999px;
  background: var(--vp-c-warning-soft);
  color: var(--vp-c-warning-1);
  font-weight: 600;
}
</style>

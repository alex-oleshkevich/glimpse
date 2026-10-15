<script setup lang="ts">
import { data } from "../../screenshots.data";

defineProps<{ name: string }>();
</script>

<template>
  <details v-if="data.includes(name)" class="applet-shot">
    <summary>Screenshot</summary>
    <ThemeImage
      :light="`/screenshots/applets/${name}-light.png`"
      :dark="`/screenshots/applets/${name}-dark.png`"
      :alt="`The ${name} popover`"
    />
  </details>
</template>

<style scoped>
.applet-shot {
  margin: 8px 0 0;
}
.applet-shot summary {
  cursor: pointer;
  color: var(--vp-c-brand-1);
  font-size: 14px;
  font-weight: 500;
}
.applet-shot :deep(.theme-image) {
  margin: 12px 0 0;
  max-width: 420px;
}
.applet-shot :deep(.theme-image img) {
  border: none;
  box-shadow: none;
}
</style>

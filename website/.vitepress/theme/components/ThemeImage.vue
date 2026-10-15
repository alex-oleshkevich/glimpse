<script setup lang="ts">
import { withBase } from "vitepress";

defineProps<{ light: string; dark: string; alt: string }>();
</script>

<template>
  <figure class="theme-image">
    <img class="theme-image__light" :src="withBase(light)" :alt="alt" loading="lazy" />
    <img class="theme-image__dark" :src="withBase(dark)" :alt="alt" loading="lazy" />
    <figcaption v-if="$slots.default"><slot /></figcaption>
  </figure>
</template>

<style scoped>
.theme-image {
  margin: 24px 0;
}
.theme-image img {
  width: 100%;
  border-radius: 12px;
  border: 1px solid var(--vp-c-divider);
  box-shadow: var(--vp-shadow-3);
}
.theme-image__dark {
  display: none;
}
:global(.dark) .theme-image__light {
  display: none;
}
:global(.dark) .theme-image__dark {
  display: block;
}
figcaption {
  margin-top: 8px;
  font-size: 14px;
  color: var(--vp-c-text-2);
  text-align: center;
}
</style>

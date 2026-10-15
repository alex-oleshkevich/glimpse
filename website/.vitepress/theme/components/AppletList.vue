<script setup lang="ts">
import { computed } from "vue";
import { withBase } from "vitepress";
import { data } from "../../schema.data";

const props = defineProps<{ names: string[]; page: string }>();

const applets = computed(() =>
  props.names.map((name) => {
    const found = data.applets.find((applet) => applet.name === name);
    if (!found) throw new Error(`no applet ${name} in the schema`);
    return found;
  }),
);
</script>

<template>
  <div class="applet-list">
    <a
      v-for="applet in applets"
      :key="applet.name"
      class="applet-list__item"
      :href="withBase(`${page}#${applet.name}`)"
    >
      <code>{{ applet.name }}</code>
      <span v-html="applet.description" />
    </a>
  </div>
</template>

<style scoped>
.applet-list {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
  gap: 12px;
  margin: 16px 0 32px;
}
.applet-list__item {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 14px 16px;
  border: 1px solid var(--vp-c-divider);
  border-radius: 12px;
  background: var(--vp-c-bg-soft);
  color: var(--vp-c-text-2);
  font-size: 14px;
  line-height: 1.5;
  text-decoration: none;
  transition: border-color 0.2s;
}
.applet-list__item:hover {
  border-color: var(--vp-c-brand-1);
}
.applet-list__item code {
  align-self: flex-start;
  font-weight: 600;
  color: var(--vp-c-brand-1);
}
.applet-list__item :deep(code) {
  font-size: 12px;
}
</style>

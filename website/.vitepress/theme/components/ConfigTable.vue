<script setup lang="ts">
import { computed } from "vue";
import { data } from "../../schema.data";

const props = defineProps<{ path: string; header?: string; describe?: boolean }>();

const table = computed(() => {
  const found = data.tables[props.path];
  if (!found) throw new Error(`no config table ${props.path}`);
  return found;
});

const headingFor = (path: string) => (data.tables[path]?.array ? `[[${path}]]` : `[${path}]`);

const heading = computed(() => props.header ?? headingFor(props.path));

const long = (value: string) => value.length > 32 || value.includes("\n");

const anchorFor = (path: string) => path.replace(/[^a-z0-9]+/gi, "-");
const anchor = (key: string) => anchorFor(`${props.path}.${key}`);
</script>

<template>
  <div :id="anchorFor(path)" class="config-table">
    <div class="config-table__header"><code>{{ heading }}</code></div>
    <div v-if="describe && table.description" class="config-table__intro" v-html="table.description" />
    <p v-if="table.options.length === 0" class="config-table__empty">No settings of its own.</p>
    <div v-for="option in table.options" :id="anchor(option.key)" :key="option.key" class="config-option">
      <div class="config-option__head">
        <a class="config-option__key" :href="`#${anchor(option.key)}`"><code>{{ option.key }}</code></a>
        <span class="config-option__type">{{ option.type }}</span>
        <span v-if="option.default === 'not set'" class="config-option__default">not set by default</span>
        <span v-else-if="option.default !== undefined && !long(option.default)" class="config-option__default">
          default <code>{{ option.default }}</code>
        </span>
      </div>
      <div class="config-option__body" v-html="option.description" />
      <div v-if="option.defaultToml || (option.default !== undefined && long(option.default))" class="config-option__long-default">
        <span>Default</span>
        <pre><code>{{ option.defaultToml ?? option.default }}</code></pre>
      </div>
      <ul v-if="option.values" class="config-option__values">
        <li v-for="value in option.values" :key="value.value">
          <code>{{ value.value }}</code><span v-if="value.description" v-html="` ${value.description}`" />
        </li>
      </ul>
      <p v-if="option.table" class="config-option__nested">
        Its settings are listed under <a :href="`#${anchorFor(option.table)}`"><code>{{ headingFor(option.table) }}</code></a>.
      </p>
    </div>
  </div>
</template>

<style scoped>
.config-table {
  margin: 16px 0 32px;
  scroll-margin-top: calc(var(--vp-nav-height) + 24px);
  border: 1px solid var(--vp-c-divider);
  border-radius: 12px;
  overflow: hidden;
}
.config-table__header {
  padding: 10px 16px;
  background: var(--vp-c-bg-soft);
  border-bottom: 1px solid var(--vp-c-divider);
  font-weight: 600;
}
.config-table__header code {
  background: none;
  padding: 0;
  font-size: 15px;
  color: var(--vp-c-brand-1);
}
.config-table__intro,
.config-table__empty {
  padding: 12px 16px 0;
  margin: 0;
  color: var(--vp-c-text-2);
}
.config-table__intro :deep(p) {
  margin: 0;
}
.config-table__empty {
  padding-bottom: 12px;
}
.config-option {
  padding: 14px 16px;
  scroll-margin-top: calc(var(--vp-nav-height) + 24px);
}
.config-option + .config-option,
.config-table__intro + .config-option {
  border-top: 1px solid var(--vp-c-divider);
}
.config-table__intro + .config-option {
  margin-top: 12px;
}
.config-option__head {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 4px 12px;
}
.config-option__key {
  text-decoration: none;
}
.config-option__key code {
  font-weight: 600;
  color: var(--vp-c-text-1);
}
.config-option__type {
  font-size: 13px;
  color: var(--vp-c-text-2);
}
.config-option__default {
  margin-left: auto;
  font-size: 13px;
  color: var(--vp-c-text-2);
}
.config-option__default code {
  font-size: 12px;
}
.config-option__body {
  margin-top: 6px;
  font-size: 15px;
  line-height: 1.6;
}
.config-option__body :deep(p) {
  margin: 6px 0 0;
}
.config-option__long-default {
  margin-top: 8px;
  font-size: 13px;
  color: var(--vp-c-text-2);
}
.config-option__long-default pre {
  margin: 4px 0 0;
  padding: 8px 12px;
  border-radius: 8px;
  background: var(--vp-c-bg-soft);
  white-space: pre-wrap;
}
.config-option__long-default code {
  font-size: 13px;
  color: var(--vp-c-text-1);
}
.config-option__values {
  margin: 6px 0 0;
  padding-left: 20px;
  font-size: 14px;
}
.config-option__values li {
  margin: 2px 0;
}
.config-option__nested {
  margin: 6px 0 0;
  font-size: 14px;
  color: var(--vp-c-text-2);
}
</style>

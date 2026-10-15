<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref } from "vue";
import { withBase } from "vitepress";
import ThemeImage from "./ThemeImage.vue";

const props = defineProps<{
  webm: string;
  mp4: string;
  poster: string;
  light: string;
  dark: string;
  alt: string;
  label: string;
}>();

const video = ref<HTMLVideoElement | null>(null);
const still = ref(false);
const playing = ref(false);
let motion: MediaQueryList | null = null;

function follow() {
  still.value = motion?.matches ?? false;
  if (!still.value) {
    video.value?.play().catch(() => {});
  }
}

function toggle() {
  const element = video.value;
  if (!element) {
    return;
  }
  if (element.paused) {
    element.play().catch(() => {});
  } else {
    element.pause();
  }
}

onMounted(() => {
  motion = window.matchMedia("(prefers-reduced-motion: reduce)");
  motion.addEventListener("change", follow);
  follow();
});

onBeforeUnmount(() => motion?.removeEventListener("change", follow));
</script>

<template>
  <ThemeImage v-if="still" :light="props.light" :dark="props.dark" :alt="props.alt" />
  <figure v-else class="demo-video">
    <video
      ref="video"
      :poster="withBase(props.poster)"
      :aria-label="props.label"
      muted
      loop
      playsinline
      preload="metadata"
      @play="playing = true"
      @pause="playing = false"
    >
      <source :src="withBase(props.webm)" type="video/webm" />
      <source :src="withBase(props.mp4)" type="video/mp4" />
    </video>
    <button
      class="demo-video__toggle"
      type="button"
      :aria-label="playing ? 'Pause the demo' : 'Play the demo'"
      @click="toggle"
    >
      <svg v-if="playing" viewBox="0 0 16 16" aria-hidden="true">
        <rect x="3" y="2" width="3.5" height="12" rx="1" />
        <rect x="9.5" y="2" width="3.5" height="12" rx="1" />
      </svg>
      <svg v-else viewBox="0 0 16 16" aria-hidden="true">
        <path d="M4 2.5v11a1 1 0 0 0 1.5.86l9-5.5a1 1 0 0 0 0-1.72l-9-5.5A1 1 0 0 0 4 2.5z" />
      </svg>
    </button>
  </figure>
</template>

<style scoped>
.demo-video {
  position: relative;
  margin: 24px 0;
}
.demo-video video {
  display: block;
  width: 100%;
  aspect-ratio: 16 / 9;
  border-radius: 12px;
  border: 1px solid var(--vp-c-divider);
  box-shadow: var(--vp-shadow-3);
  background: var(--vp-c-bg-soft);
}
.demo-video__toggle {
  position: absolute;
  right: 16px;
  bottom: 16px;
  display: grid;
  place-items: center;
  width: 40px;
  height: 40px;
  border-radius: 50%;
  color: #fff;
  background: rgba(0, 0, 0, 0.55);
  backdrop-filter: blur(8px);
  transition: background 0.2s;
}
.demo-video__toggle:hover {
  background: rgba(0, 0, 0, 0.75);
}
.demo-video__toggle:focus-visible {
  outline: 2px solid var(--vp-c-brand-1);
  outline-offset: 2px;
}
.demo-video__toggle svg {
  width: 16px;
  height: 16px;
  fill: currentColor;
}
</style>

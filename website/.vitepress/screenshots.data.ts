import { readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { defineLoader } from "vitepress";

const dir = fileURLToPath(new URL("../public/screenshots/applets", import.meta.url));

declare const data: string[];
export { data };

export default defineLoader({
  watch: [`${dir}/*.png`],
  load(): string[] {
    const files = new Set(readdirSync(dir));
    return [...files]
      .filter((file) => file.endsWith("-light.png"))
      .map((file) => file.slice(0, -"-light.png".length))
      .filter((name) => files.has(`${name}-dark.png`));
  },
});

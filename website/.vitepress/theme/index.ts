import DefaultTheme from "vitepress/theme";
import type { Theme } from "vitepress";
import AppletList from "./components/AppletList.vue";
import Compositors from "./components/Compositors.vue";
import ConfigTable from "./components/ConfigTable.vue";
import ThemeImage from "./components/ThemeImage.vue";
import "./style.css";

export default {
  extends: DefaultTheme,
  enhanceApp({ app }) {
    app.component("AppletList", AppletList);
    app.component("Compositors", Compositors);
    app.component("ConfigTable", ConfigTable);
    app.component("ThemeImage", ThemeImage);
  },
} satisfies Theme;

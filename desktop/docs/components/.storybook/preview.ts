import type { Preview } from "storybook-solidjs-vite";
import "../../../src/styles.css";
const preview: Preview = {
  tags: ["autodocs"],
  parameters: { layout: "padded", controls: { expanded: true } },
};
export default preview;

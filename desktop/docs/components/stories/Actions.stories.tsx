import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { expect } from "storybook/test";
import { Action } from "../../../src/Controls";
const meta = {
  title: "Controls/Action",
  component: Action,
  args: { icon: "scan", children: "Scanner les dossiers" },
} satisfies Meta<typeof Action>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Ready: Story = {};
export const Loading: Story = { args: { busy: true } };
export const FormAlignment: Story = {
  render: () => (
    <div class="start-controls">
      <input aria-label="Port" value="8787" />
      <Action icon="play" class="primary">
        Démarrer le serveur
      </Action>
      <Action icon="scan" busy>
        Scanner les dossiers
      </Action>
    </div>
  ),
  play: async ({ canvasElement }) => {
    const boxes = [...canvasElement.querySelectorAll("button,input")].map((e) =>
      e.getBoundingClientRect(),
    );
    for (const box of boxes) {
      expect(box.height).toBe(38);
      expect(Math.abs(box.top - boxes[0]!.top)).toBeLessThan(1);
    }
  },
};

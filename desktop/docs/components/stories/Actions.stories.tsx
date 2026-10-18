import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { expect, userEvent, waitFor } from "storybook/test";
import { createSignal } from "solid-js";
import { Action } from "../../../src/Controls";
const meta = {
  title: "Controls/Action",
  component: Action,
  args: { icon: "scan", children: "Scanner les dossiers" },
} satisfies Meta<typeof Action>;
export default meta;
type Story = StoryObj<typeof meta>;
export const Ready: Story = {};
export const Refresh: Story = {
  args: { icon: "refresh", class: "icon-button", children: "", "aria-label": "Actualiser" },
};
/** A fast response must update its result immediately while the refresh glyph completes a turn. */
export const FastRefresh: Story = {
  render: () => {
    const [busy, setBusy] = createSignal(false);
    const [completed, setCompleted] = createSignal(0);
    return (
      <div class="action-group">
        <Action
          icon="refresh"
          class="icon-button"
          aria-label="Actualiser"
          busy={busy()}
          onClick={async () => {
            setBusy(true);
            await new Promise<void>((resolve) => setTimeout(resolve, 40));
            setCompleted((value) => value + 1);
            setBusy(false);
          }}
        />
        <output aria-label="Actualisations terminées">{completed()}</output>
        <output aria-label="Résultat du contrôle">Contrôle en cours</output>
      </div>
    );
  },
  play: async ({ canvasElement }) => {
    const button = canvasElement.querySelector("button")!;
    const outcome = canvasElement.querySelector<HTMLOutputElement>(
      '[aria-label="Résultat du contrôle"]',
    )!;
    try {
      await userEvent.click(button);
      await waitFor(() => expect(canvasElement.querySelector("output")?.textContent).toBe("1"));
      expect(button.disabled).toBe(true);
      await waitFor(() => expect(button.disabled).toBe(false), { timeout: 2500 });
      outcome.textContent = "Contrôle réussi";
    } catch (error) {
      outcome.textContent = "Contrôle échoué";
      throw error;
    }
  },
};
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

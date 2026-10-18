import { Show, type JSX } from "solid-js";
const paths = {
  play: "m8 5 11 7-11 7V5Z",
  stop: "M6 6h12v12H6V6Z",
  plus: "M12 5v14M5 12h14",
  minus: "M5 12h14",
  scan: "M11 3a8 8 0 1 0 0 16 8 8 0 0 0 0-16Zm6 14 5 5",
  download: "M12 3v12m-5-5 5 5 5-5M4 16v5h16v-5",
  eye: "M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12Zm10-3a3 3 0 1 0 0 6 3 3 0 0 0 0-6Z",
  close: "m6 6 12 12M6 18 18 6",
  lock: "M5 10h14v11H5V10ZM8 10V6a4 4 0 0 1 8 0v4",
};
/** Shared 18 px glyphs match Phémios action geometry. */
export function Icon(props: { name: keyof typeof paths }) {
  return (
    <svg
      viewBox="0 0 24 24"
      aria-hidden="true"
      fill="none"
      stroke="currentColor"
      stroke-width="1.7"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <path d={paths[props.name]} />
    </svg>
  );
}
/** Fixed-size loader replaces its icon without changing the button's footprint. */
export function Action(
  props: JSX.ButtonHTMLAttributes<HTMLButtonElement> & { icon: keyof typeof paths; busy?: boolean },
) {
  return (
    <button
      type="button"
      class={props.class}
      disabled={props.disabled || props.busy}
      onClick={props.onClick}
      title={props.title}
      aria-label={props["aria-label"]}
      aria-busy={props.busy}
    >
      <Show when={props.busy} fallback={<Icon name={props.icon} />}>
        <span class="spinner" />
      </Show>
      {props.children}
    </button>
  );
}

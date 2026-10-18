import { Show, createEffect, createSignal, onCleanup, untrack, type JSX } from "solid-js";
const paths = {
  play: "m8 5 11 7-11 7V5Z",
  stop: "M6 6h12v12H6V6Z",
  plus: "M12 5v14M5 12h14",
  minus: "M5 12h14",
  scan: "M11 3a8 8 0 1 0 0 16 8 8 0 0 0 0-16Zm6 14 5 5",
  download: "M12 3v12m-5-5 5 5 5-5M4 16v5h16v-5",
  eye: "M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12Zm10-3a3 3 0 1 0 0 6 3 3 0 0 0 0-6Z",
  close: "m6 6 12 12M6 18 18 6",
  copy: "M9 9h12v12H9V9ZM15 5V3H3v12h2",
  check: "m5 12 4 4L19 6",
  refresh: "M3 11a9 9 0 0 1 15.35-6.36L21 7M21 3v4h-4M21 13a9 9 0 0 1-15.35 6.36L3 17M3 21v-4h4",
  server: "M3 3h18v7H3V3Zm0 11h18v7H3v-7ZM7 6.5h.01M7 17.5h.01",
  activity: "M2 12h4l3-8 6 16 3-8h4",
  list: "M8 5h13M8 12h13M8 19h13M3 5h.01M3 12h.01M3 19h.01",
  trash: "M3 6h18M9 6V3h6v3M5 6l1 15h12l1-15M10 10v7m4-7v7",
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
/** Shared feedback: refresh arrows finish a full turn; other actions retain their fixed-size loader. */
export function Action(
  props: JSX.ButtonHTMLAttributes<HTMLButtonElement> & { icon: keyof typeof paths; busy?: boolean },
) {
  const [rotating, setRotating] = createSignal(false);
  let fallback: ReturnType<typeof setTimeout> | undefined;
  createEffect(() => {
    clearTimeout(fallback);
    if (props.icon !== "refresh") {
      setRotating(false);
      return;
    }
    if (props.busy) setRotating(true);
    else if (untrack(rotating)) {
      // Animation events may be suppressed in hidden views or with reduced motion.
      fallback = setTimeout(() => setRotating(false), 900);
    }
  });
  onCleanup(() => clearTimeout(fallback));
  const busy = () => !!props.busy || rotating();
  return (
    <button
      type="button"
      class={props.class}
      disabled={props.disabled || busy()}
      onClick={props.onClick}
      title={props.title}
      aria-label={props["aria-label"]}
      aria-busy={busy()}
    >
      <Show
        when={props.icon === "refresh"}
        fallback={
          <Show when={props.busy} fallback={<Icon name={props.icon} />}>
            <span class="spinner" />
          </Show>
        }
      >
        <span
          class="refresh-glyph"
          classList={{ "refresh-spinning": rotating() }}
          onAnimationIteration={(event) => {
            if (event.target === event.currentTarget && !props.busy) {
              clearTimeout(fallback);
              setRotating(false);
            }
          }}
        >
          <Icon name="refresh" />
        </span>
      </Show>
      {props.children}
    </button>
  );
}

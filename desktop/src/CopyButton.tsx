import { createSignal, onCleanup } from "solid-js";
import { Action } from "./Controls";
import { copyAddress, copyPassword, copyUsername } from "./platform";
/** Copies only a native-owned value. The password never enters the webview just to copy it. */
export function CopyButton(props: {
  kind: "address" | "password" | "username";
  disabled: boolean;
  onError: (error: string) => void;
}) {
  const [busy, setBusy] = createSignal(false);
  const [copied, setCopied] = createSignal(false);
  let alive = true;
  let timer: ReturnType<typeof setTimeout> | undefined;
  onCleanup(() => {
    alive = false;
    clearTimeout(timer);
  });
  const label = () =>
    copied()
      ? "Copié"
      : props.kind === "address"
        ? "Copier l’adresse HTTPS"
        : props.kind === "username"
          ? "Copier le login"
          : "Copier le mot de passe";
  return (
    <Action
      icon={copied() ? "check" : "copy"}
      class={copied() ? "icon-button copy-button copied" : "icon-button copy-button"}
      busy={busy()}
      disabled={props.disabled}
      title={label()}
      aria-label={label()}
      onClick={async () => {
        setBusy(true);
        try {
          await (props.kind === "address"
            ? copyAddress()
            : props.kind === "username"
              ? copyUsername()
              : copyPassword());
          if (alive) {
            setCopied(true);
            clearTimeout(timer);
            timer = setTimeout(() => setCopied(false), 2500);
          }
        } catch (error) {
          if (alive) props.onError(error instanceof Error ? error.message : String(error));
        } finally {
          if (alive) setBusy(false);
        }
      }}
    />
  );
}

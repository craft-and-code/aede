import { createSignal, createEffect, onMount, onCleanup, For, Show } from "solid-js";
import { Action, Icon } from "./Controls";
import {
  hostDiagnostics,
  hostLogs,
  clearLogs,
  exportLogs,
  native,
  type Diagnostics,
  type HostStatus,
  type LogEntry,
} from "./platform";
import { mergeLogs } from "./policy";

/** Explicit health checks use the pinned native HTTPS client, never browser networking. */
export function StatusView(props: { status: HostStatus | undefined }) {
  const [value, setValue] = createSignal<Diagnostics>();
  const [busy, setBusy] = createSignal(false);
  const [error, setError] = createSignal("");
  let alive = true;
  async function refresh() {
    if (!native() || busy()) return;
    setBusy(true);
    try {
      const next = await hostDiagnostics();
      if (alive) {
        setValue(next);
        setError("");
      }
    } catch (error) {
      if (alive) setError(error instanceof Error ? error.message : String(error));
    } finally {
      if (alive) setBusy(false);
    }
  }
  onMount(() => {
    void refresh();
  });
  onCleanup(() => {
    alive = false;
  });
  return (
    <section class="panel monitoring-panel">
      <div class="section-heading">
        <h1>Statut du serveur</h1>
        <Action
          icon="refresh"
          class="icon-button"
          busy={busy()}
          disabled={!native()}
          aria-label="Vérifier le statut"
          title="Vérifier le statut"
          onClick={() => void refresh()}
        />
      </div>
      <p class="hint">
        Vérification de l’API, de l’authentification et du catalogue via le certificat HTTPS de ce
        serveur.
      </p>
      <Show
        when={value()}
        fallback={
          <div class="empty">
            <Show when={busy()} fallback={<Icon name="activity" />}>
              <span class="spinner" />
            </Show>
            <p>{error() || "Démarrez le serveur pour vérifier son état."}</p>
          </div>
        }
      >
        {(result) => (
          <>
            <div class="health-grid">
              <Health
                label="Processus serveur"
                good={props.status?.running ?? false}
                value={props.status?.running ? "En cours" : "Arrêté"}
              />
              <Health
                label="API HTTPS authentifiée"
                good={result().apiResponding && !!props.status?.running}
                value={
                  props.status?.running
                    ? result().apiResponding
                      ? `Disponible · API v${result().apiVersion}`
                      : "Indisponible"
                    : "Serveur arrêté"
                }
              />
              <Health
                label="Catalogue"
                good={result().catalogLoaded}
                value={result().catalogLoaded ? "Chargé" : "Aucun catalogue chargé"}
              />
              <Health
                label="Dossiers disponibles"
                good={
                  !!props.status?.roots.length && props.status.roots.every((root) => root.available)
                }
                value={`${props.status?.roots.filter((root) => root.available).length ?? 0} / ${props.status?.roots.length ?? 0}`}
              />
            </div>
            <Show when={result().library}>
              {(library) => (
                <dl class="catalog-counts">
                  <div>
                    <dt>Albums</dt>
                    <dd>{library().releases.toLocaleString("fr-FR")}</dd>
                  </div>
                  <div>
                    <dt>Artistes</dt>
                    <dd>{library().artists.toLocaleString("fr-FR")}</dd>
                  </div>
                  <div>
                    <dt>Pistes</dt>
                    <dd>{library().tracks.toLocaleString("fr-FR")}</dd>
                  </div>
                  <div>
                    <dt>Fichiers</dt>
                    <dd>{library().files.toLocaleString("fr-FR")}</dd>
                  </div>
                </dl>
              )}
            </Show>
            <Show when={result().error || error()}>
              <p class="feedback error" role="alert">
                {error() || result().error}
              </p>
            </Show>
            <p class="hint">
              Vérifié à {new Date(result().checkedAt).toLocaleTimeString("fr-FR")} ·{" "}
              {result().latencyMs} ms. Ces contrôles ne lancent ni scan ni analyse.
            </p>
          </>
        )}
      </Show>
    </section>
  );
}
function Health(props: { label: string; good: boolean; value: string }) {
  return (
    <div class="health-item">
      <span class="health-dot" classList={{ good: props.good }} />
      <div>
        <p>{props.label}</p>
        <strong>{props.value}</strong>
      </div>
    </div>
  );
}
/** Incremental native logs stay bounded; new requests do not move a reader away from older entries. */
export function LogsView() {
  const [entries, setEntries] = createSignal<LogEntry[]>([]);
  const [error, setError] = createSignal("");
  const [busy, setBusy] = createSignal<"" | "clear" | "export">("");
  const [exported, setExported] = createSignal(false);
  const [message, setMessage] = createSignal("");
  createEffect(() => {
    if (!exported() && !message()) return;
    const timer = setTimeout(() => {
      setExported(false);
      setMessage("");
    }, 5000);
    onCleanup(() => clearTimeout(timer));
  });
  let revision = 0;
  let cursor = 0,
    alive = true,
    pending = false;
  let box: HTMLDivElement | undefined;
  async function refresh() {
    if (!native() || pending || busy()) return;
    pending = true;
    const expected = revision;
    try {
      const next = await hostLogs(cursor);
      if (!alive || expected !== revision) return;
      cursor = next.nextId;
      const follow = !box || box.scrollHeight - box.scrollTop - box.clientHeight < 48;
      if (next.entries.length) setEntries((previous) => mergeLogs(previous, next.entries));
      if (follow)
        queueMicrotask(() => {
          if (alive && box) box.scrollTop = box.scrollHeight;
        });
    } catch (error) {
      if (alive) setError(error instanceof Error ? error.message : String(error));
    } finally {
      pending = false;
    }
  }
  onMount(() => {
    void refresh();
    const timer = setInterval(() => void refresh(), 1000);
    onCleanup(() => clearInterval(timer));
  });
  onCleanup(() => {
    alive = false;
  });
  return (
    <section class="panel monitoring-panel">
      <div class="section-heading">
        <h1>Journaux</h1>
        <div class="action-group">
          <Action
            icon={exported() ? "check" : "download"}
            class={exported() ? "icon-button copied" : "icon-button"}
            busy={busy() === "export"}
            disabled={!native() || !!busy() || !entries().length}
            aria-label="Télécharger les journaux"
            title="Télécharger les journaux"
            onClick={async () => {
              setBusy("export");
              setError("");
              setMessage("");
              setExported(false);
              try {
                const path = await exportLogs();
                if (alive && path) {
                  setExported(true);
                  setMessage(`Journaux enregistrés : ${path}`);
                }
              } catch (error) {
                if (alive) setError(error instanceof Error ? error.message : String(error));
              } finally {
                if (alive) setBusy("");
              }
            }}
          />
          <Action
            icon="trash"
            class="icon-button"
            busy={busy() === "clear"}
            disabled={!native() || !!busy() || !entries().length}
            aria-label="Effacer les journaux"
            title="Effacer les journaux"
            onClick={async () => {
              revision++;
              setBusy("clear");
              try {
                await clearLogs();
                if (alive) {
                  setEntries([]);
                  setError("");
                }
              } catch (error) {
                if (alive) setError(error instanceof Error ? error.message : String(error));
              } finally {
                if (alive) setBusy("");
              }
            }}
          />
        </div>
      </div>
      <p class="hint">
        Requêtes HTTP et événements du serveur · 500 dernières entrées · conservées jusqu’à la
        fermeture de l’application.
      </p>
      <Show when={error() || message()}>
        <div class="feedback" classList={{ error: !!error() }} role={error() ? "alert" : "status"}>
          <span>{error() || message()}</span>
          <Action
            icon="close"
            class="icon-close"
            aria-label="Fermer le message"
            onClick={() => {
              setError("");
              setMessage("");
            }}
          />
        </div>
      </Show>
      <div
        class="log-list"
        ref={(element) => {
          box = element;
        }}
        role="log"
        aria-label="Journaux du serveur"
        aria-live="off"
        tabindex="0"
      >
        <Show
          when={entries().length}
          fallback={
            <div class="empty">
              <Icon name="list" />
              <p>Aucun événement enregistré.</p>
            </div>
          }
        >
          <For each={entries()}>
            {(entry) => (
              <div class={`log-row ${entry.level}`}>
                <time datetime={new Date(entry.timeMs).toISOString()}>
                  {new Date(entry.timeMs).toLocaleTimeString("fr-FR")}
                </time>
                <span class="log-level">
                  {entry.level === "error"
                    ? "Erreur"
                    : entry.level === "warn"
                      ? "Attention"
                      : "Info"}
                </span>
                <span>{entry.message}</span>
              </div>
            )}
          </For>
        </Show>
      </div>
      <p class="hint">
        Les paramètres, identifiants de ressources, en-têtes et contenus des requêtes ne sont pas
        journalisés. Les résultats détaillés des scans restent dans l’onglet Serveur. En cas de
        saturation, des résumés peuvent être omis. Le téléchargement exporte les entrées conservées
        au format JSON.
      </p>
    </section>
  );
}

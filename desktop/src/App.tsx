import { createSignal, createEffect, onMount, onCleanup, For, Show } from "solid-js";
import { Action, Icon } from "./Controls";
import { CopyButton } from "./CopyButton";
import { StatusView, LogsView } from "./Monitoring";
import { appendRoots, parsePort, taskActive } from "./policy";
import { createLibraryActions } from "./library-actions";
import {
  hostStatus,
  hostStart,
  hostStop,
  hostConfigure,
  chooseFolders,
  hostScan,
  hostTask,
  hostPassword,
  exportConnection,
  native,
  type HostStatus,
  type Task,
} from "./platform";
/** Minimal server console; catalog, accounts and scans remain Aède engine operations. */
export function App() {
  const [page, setPage] = createSignal<"server" | "status" | "logs">("server");
  const [status, setStatus] = createSignal<HostStatus>();
  const [busy, setBusy] = createSignal("");
  const [port, setPort] = createSignal("8787");
  const [error, setError] = createSignal("");
  const [message, setMessage] = createSignal("");
  const [secret, setSecret] = createSignal("");
  const [task, setTask] = createSignal<Task>();
  const active = () => taskActive(task()?.status ?? "");
  const roots = () => status()?.roots.map((row) => row.path) ?? [];
  let alive = true;
  let revision = 0;
  onCleanup(() => {
    alive = false;
    setSecret("");
  });
  const apply = (next: HostStatus) => {
    setStatus(next);
    setPort(String(next.port));
    if (!next.running) setSecret("");
  };
  const library = createLibraryActions({
    status,
    configure: hostConfigure,
    start: hostStart,
    scan: hostScan,
    publish: apply,
    publishTask: setTask,
  });
  async function run(name: string, action: () => Promise<unknown>) {
    if (busy()) return;
    revision++;
    setBusy(name);
    setError("");
    setMessage("");
    try {
      await action();
    } catch (reason) {
      if (alive) setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      if (alive) setBusy("");
    }
  }
  onMount(() => {
    if (native()) void run("status", async () => apply(await hostStatus()));
    else
      setMessage(
        "Aperçu de l’interface. Le serveur et les dossiers nécessitent l’application native.",
      );
  });
  createEffect(() => {
    if (!status()?.running) return;
    let pending = false;
    const timer = setInterval(async () => {
      if (pending || busy()) return;
      pending = true;
      const expected = revision;
      try {
        const next = await hostStatus();
        if (alive && expected === revision) {
          setStatus(next);
          if (!next.running) setSecret("");
        }
      } catch (reason) {
        if (alive) setError(String(reason));
      } finally {
        pending = false;
      }
    }, 5000);
    onCleanup(() => clearInterval(timer));
  });
  createEffect(() => {
    const id = task()?.task_id;
    if (!id || !active()) return;
    let cancelled = false,
      pending = false;
    const timer = setInterval(async () => {
      if (pending) return;
      pending = true;
      try {
        const next = await hostTask(id);
        if (!cancelled && alive) {
          setTask(next);
          if (!taskActive(next.status))
            setMessage(
              next.status === "completed"
                ? "Scan terminé. Le catalogue est disponible pour Phémios."
                : "La tâche s’est arrêtée. Consultez son résultat ci-dessous.",
            );
        }
      } catch (reason) {
        if (!cancelled && alive) {
          setError(String(reason));
          setTask((previous) => (previous ? { ...previous, status: "unavailable" } : undefined));
        }
      } finally {
        pending = false;
      }
    }, 1000);
    onCleanup(() => {
      cancelled = true;
      clearInterval(timer);
    });
  });
  createEffect(() => {
    if (!message()) return;
    const timer = setTimeout(() => setMessage(""), 8000);
    onCleanup(() => clearTimeout(timer));
  });
  return (
    <div class="application" onContextMenu={(event) => event.preventDefault()}>
      <header class="app-header">
        <img src="/wordmark.svg" alt="Aède" />
        <span>Serveur de votre bibliothèque musicale</span>
        <small>{status()?.version ?? "—"}</small>
      </header>
      <main>
        <nav class="view-tabs" aria-label="Console Aède">
          <button
            type="button"
            aria-pressed={page() === "server"}
            onClick={() => setPage("server")}
          >
            <Icon name="server" />
            Serveur
          </button>
          <button
            type="button"
            aria-pressed={page() === "status"}
            onClick={() => setPage("status")}
          >
            <Icon name="activity" />
            Statut
          </button>
          <button type="button" aria-pressed={page() === "logs"} onClick={() => setPage("logs")}>
            <Icon name="list" />
            Journaux
          </button>
        </nav>
        <Show when={page() === "status"}>
          <StatusView status={status()} />
        </Show>
        <Show when={page() === "logs"}>
          <LogsView />
        </Show>
        <Show when={page() === "server"}>
          <section class="server-card">
            <div class="section-heading">
              <div>
                <p class="eyebrow">VOTRE SERVEUR AÈDE</p>
                <h1>Votre musique, à portée de Phémios.</h1>
              </div>
              <span class="server-state" classList={{ running: status()?.running ?? false }}>
                <i />
                {status()?.running ? "En ligne" : "Arrêté"}
              </span>
            </div>
            <p class="muted">
              Ajoutez vos dossiers, démarrez le serveur et connectez Phémios sur ce même ordinateur.
            </p>
            <div class="server-address">
              <Icon name="lock" />
              <div>
                <span>Adresse HTTPS locale</span>
                <strong>{status()?.address ?? `https://127.0.0.1:${port()}`}</strong>
              </div>
              <CopyButton
                kind="address"
                disabled={!!busy() || !status()?.running}
                onError={setError}
              />
            </div>
            <div class="start-controls">
              <label class="port-label">
                Port
                <input
                  inputmode="numeric"
                  value={port()}
                  disabled={!!busy() || status()?.running || !native()}
                  onInput={(event) => setPort(event.currentTarget.value)}
                />
              </label>
              <Action
                icon="play"
                class="primary"
                busy={busy() === "start"}
                disabled={!native() || !!busy() || status()?.running}
                onClick={() => void run("start", () => library.start(parsePort(port()), roots()))}
              >
                Démarrer le serveur
              </Action>
              <Action
                icon="stop"
                busy={busy() === "stop"}
                disabled={!!busy() || !status()?.running}
                onClick={() =>
                  void run("stop", async () => {
                    apply(await hostStop());
                    setTask(undefined);
                  })
                }
              >
                Arrêter
              </Action>
            </div>
            <small class="hint">
              Accès limité à cet ordinateur. Le certificat local est vérifié explicitement ; aucun
              port n’est ouvert sur le réseau.
            </small>
          </section>
          <div class="content-grid">
            <section class="panel library-panel">
              <div class="section-heading">
                <h2>Dossiers de musique</h2>
                <Action
                  class="icon-button"
                  icon="plus"
                  aria-label="Ajouter des dossiers"
                  title="Ajouter des dossiers"
                  disabled={!native() || !!busy() || active()}
                  onClick={() =>
                    void run("folders", async () => {
                      const chosen = await chooseFolders();
                      if (chosen.length)
                        await library.configure(parsePort(port()), appendRoots(roots(), chosen));
                    })
                  }
                />
              </div>
              <Show
                when={status()?.roots.length}
                fallback={
                  <div class="empty">
                    <Icon name="plus" />
                    <p>Ajoutez vos premiers dossiers.</p>
                  </div>
                }
              >
                <ul class="root-list">
                  <For each={status()?.roots}>
                    {(row) => (
                      <li>
                        <div>
                          <span title={row.path}>{row.path}</span>
                          <small classList={{ offline: !row.available }}>
                            {row.available ? "Disponible" : "Volume absent — catalogue conservé"}
                          </small>
                        </div>
                        <Action
                          icon="minus"
                          class="icon-button"
                          aria-label={`Retirer ${row.path}`}
                          title="Retirer de la sélection"
                          disabled={!!busy() || active()}
                          onClick={() =>
                            void run("folders", () =>
                              library.configure(
                                parsePort(port()),
                                roots().filter((path) => path !== row.path),
                              ),
                            )
                          }
                        />
                      </li>
                    )}
                  </For>
                </ul>
              </Show>
              <div class="panel-footer">
                <Action
                  icon="scan"
                  busy={busy() === "scan" || active()}
                  disabled={!!busy() || !status()?.running || !roots().length || active()}
                  onClick={() => void run("scan", async () => setTask(await hostScan()))}
                >
                  Scanner les dossiers
                </Action>
                <Show when={active()}>
                  <Action
                    icon="stop"
                    disabled={!!busy() || task()?.status === "cancel_requested"}
                    onClick={() =>
                      void run("cancel", async () => setTask(await hostTask(task()!.task_id, true)))
                    }
                  >
                    Arrêter le scan
                  </Action>
                </Show>
              </div>
              <p class="hint">
                Le démarrage du serveur et les changements de dossiers déclenchent un scan en
                arrière-plan. Vos fichiers audio ne sont pas modifiés. Les volumes absents sont
                conservés ; une sélection vide conserve le catalogue existant.
              </p>
            </section>
            <section class="panel connection-panel">
              <h2>Connecter Phémios</h2>
              <ol>
                <li>Démarrez le serveur.</li>
                <li>
                  Attendez la fin du scan automatique pour retrouver les albums de vos dossiers.
                </li>
                <li>
                  Exportez sa connexion locale puis importez-la dans les paramètres Serveur Aède de
                  Phémios.
                </li>
                <li>Utilisez le compte ci-dessous et son mot de passe.</li>
                <li>Phémios recharge automatiquement le catalogue après les scans.</li>
              </ol>
              <Action
                icon="download"
                disabled={!!busy() || !status()?.running}
                busy={busy() === "export"}
                onClick={() =>
                  void run("export", async () => {
                    const path = await exportConnection();
                    if (path) setMessage(`Connexion exportée : ${path}`);
                  })
                }
              >
                Exporter la connexion
              </Action>
              <div class="access">
                <span>Compte</span>
                <strong>{status()?.username ?? "aede-desktop"}</strong>
                <CopyButton kind="username" disabled={!native() || !!busy()} onError={setError} />
                <div class="password-actions">
                  <span>Mot de passe</span>
                  <Action
                    class="icon-button"
                    icon="eye"
                    title={secret() ? "Masquer le mot de passe" : "Afficher le mot de passe"}
                    aria-label={secret() ? "Masquer le mot de passe" : "Afficher le mot de passe"}
                    disabled={!!busy() || !status()?.running}
                    onClick={() =>
                      void run("password", async () =>
                        setSecret(secret() ? "" : await hostPassword()),
                      )
                    }
                  />
                  <CopyButton
                    kind="password"
                    disabled={!!busy() || !status()?.running}
                    onError={setError}
                  />
                </div>
                <Show when={secret()}>
                  <input
                    class="secret"
                    aria-label="Mot de passe du serveur"
                    readonly
                    value={secret()}
                  />
                </Show>
              </div>
              <small class="hint">
                L’export contient uniquement l’adresse, le nom du compte et le certificat public. Le
                mot de passe reste dans le profil privé de l’application.
              </small>
            </section>
          </div>
          <Show when={task()}>
            <section class="panel task-panel">
              <div class="section-heading">
                <h2>Scan #{task()?.task_id}</h2>
                <span class="muted">{active() ? "En cours…" : task()?.status}</span>
              </div>
              <Show when={task()?.result}>
                <pre>
                  {task()?.result?.stdout}
                  {task()?.result?.stderr}
                </pre>
              </Show>
            </section>
          </Show>
        </Show>
        <Show when={error() || message()}>
          <div
            class="feedback"
            classList={{ error: !!error() }}
            role={error() ? "alert" : "status"}
          >
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
        <footer>
          <span>
            Aède {status()?.engines.aede ?? "—"} · FlacCompagnon{" "}
            {status()?.engines.flaccompagnon ?? "—"}
          </span>
          <small>Bibliothèque isolée de Phémios et de la CLI · aucun démarrage automatique</small>
          <Show when={status()}>
            <small class="data-path" title={status()?.dataDirectory}>
              {status()?.dataDirectory}
            </small>
          </Show>
        </footer>
      </main>
    </div>
  );
}

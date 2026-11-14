import { useEffect, useState } from "react";

import styles from "./App.module.css";
import { AttentionRail } from "./components/AttentionRail";
import { CardGrid } from "./components/CardGrid";
import { DaemonChips } from "./components/DaemonChips";
import { DaemonDialogs } from "./components/DaemonDialogs";
import { DiagnosticsPanel } from "./components/DiagnosticsPanel";
import { DrillDownPanel } from "./components/DrillDownPanel";
import { Header } from "./components/Header";
import { MenuBar } from "./components/MenuBar";
import { StatusBar } from "./components/StatusBar";
import { StatusFilterChips } from "./components/StatusFilterChips";
import { ToastViewport } from "./components/ToastViewport";
import { findShortcut, isMac } from "./lib/shortcuts";
import { useTheme } from "./lib/theme";
import { useNow } from "./lib/useNow";
import type { Backend, View } from "./lib/types";

const mac = isMac();

export function App({ backend }: { backend: Backend }) {
  // The whole screen is a function of this one value, which the shell owns and sends.
  const [view, setView] = useState<View | null>(null);
  const [failure, setFailure] = useState("");

  useEffect(() => {
    let stop = () => {};
    let gone = false;
    void backend
      .onView((next) => setView(next))
      .then((unlisten) => {
        if (gone) {
          unlisten();
        } else {
          stop = unlisten;
        }
      });
    void backend.control("ui.inspect").then((reply) => {
      if (gone) {
        return;
      }
      if (!reply.ok) {
        setFailure(reply.error);
      } else {
        // A `view` event that arrived first is newer than this answer.
        setView((current) => current ?? (reply.result as View));
      }
    });
    return () => {
      gone = true;
      stop();
    };
  }, [backend]);

  useTheme(view?.ui.theme);
  const now = useNow(view);

  // A command by id with params, through the control protocol (shell and engine commands alike).
  const run = (command: string, params: Record<string, unknown> = {}) => void backend.control("engine.execute", { command, params });

  const menus = view?.menus;
  const dialogOpen = view?.ui.dialog != null;
  useEffect(() => {
    if (!menus) {
      return;
    }
    const onKeydown = (event: KeyboardEvent) => {
      // Esc belongs to an open dialog or an open menu first (each closes itself and marks the event handled).
      if (event.defaultPrevented || (event.key === "Escape" && dialogOpen)) {
        return;
      }
      const id = findShortcut(menus, event, mac);
      if (id) {
        event.preventDefault();
        void backend.invoke(id);
      }
    };
    window.addEventListener("keydown", onKeydown);
    return () => window.removeEventListener("keydown", onKeydown);
  }, [menus, backend, dialogOpen]);

  if (!view) {
    return <p className={styles.loading}>{failure || "Loading Pi Studio Dashboard…"}</p>;
  }
  const document_ = view.document;
  const drill = document_?.drill ?? null;
  const filtered = (document_?.daemonChips.some((chip) => chip.selected) ?? false) || (document_?.statusChips.some((chip) => chip.selected) ?? false);
  const clearFilters = () => {
    run("view.filterDaemons", { serverIds: [] });
    run("view.filterStatuses", { statuses: [] });
  };
  return (
    <div className={styles.app}>
      <MenuBar menus={view.menus} mac={mac} onInvoke={(id) => void backend.invoke(id)} />
      <Header document={document_} />
      <DaemonChips chips={document_?.daemonChips ?? []} menus={view.menus} backend={backend} />
      <StatusFilterChips chips={document_?.statusChips ?? []} filtered={filtered} onChange={(statuses) => run("view.filterStatuses", { statuses })} onClear={clearFilters} />
      <main className={styles.main}>
        <CardGrid
          document={document_}
          now={now}
          onSelect={(card) => run("agent.select", { serverId: card.serverId, agentId: card.agentId })}
          onWindow={(window_) => run("view.setGridWindow", window_)}
          onClearFilters={clearFilters}
          onAddDaemon={() => void backend.invoke("daemon.add")}
        />
        {drill ? <DrillDownPanel drill={drill} now={now} onClose={() => run("agent.deselect")} /> : <AttentionRail collapsed={view.ui.attentionRailCollapsed} />}
      </main>
      <StatusBar document={document_} status={view.ui.status} />
      <ToastViewport toasts={view.ui.toasts} />
      <DaemonDialogs dialog={view.ui.dialog} chips={document_?.daemonChips ?? []} backend={backend} />
      {view.ui.diagnosticsTarget && document_?.diagnostics && <DiagnosticsPanel backend={backend} data={document_.diagnostics} />}
    </div>
  );
}

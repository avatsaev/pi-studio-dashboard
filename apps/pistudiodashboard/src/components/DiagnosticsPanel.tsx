import type { ReactNode } from "react";

import { setDiagnosticsTarget } from "../lib/commands";
import { localTime, NO_OBSERVABILITY_HINT } from "../lib/daemonText";
import type { Backend, Diagnostics } from "../lib/types";
import { Button } from "./Button";
import { Dialog } from "./Dialog";
import styles from "./DiagnosticsPanel.module.css";

const dash = "—";

function num(n: number | null | undefined, unit = ""): string {
  return n === null || n === undefined ? dash : `${n.toLocaleString()}${unit}`;
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <dt>{label}</dt>
      <dd>{children}</dd>
    </>
  );
}

/**
 * Read-only per-daemon diagnostics (`UiState.diagnosticsTarget`), drawn as a modal panel over the
 * main content so the attention rail stays visible. Everything is the view's data: no secret or
 * payload text exists in it (D29). Esc and the Close button clear the target.
 */
export function DiagnosticsPanel({ backend, data }: { backend: Backend; data: Diagnostics }) {
  const close = () => void setDiagnosticsTarget(backend, null);
  const address = data.host ? `${data.tls ? "wss" : "ws"}://${data.host}${data.port === null ? "" : `:${data.port}`}` : dash;
  return (
    <Dialog title={`Diagnostics · ${data.name}`} onClose={close} wide footer={<Button onClick={close}>Close</Button>}>
      <div className={styles.panel}>
        {data.degraded.includes("no_observability") && (
          <output className={styles.degraded}>{NO_OBSERVABILITY_HINT}</output>
        )}
        <section aria-label="Connection">
          <h3>Connection</h3>
          <dl>
            <Row label="Address">{address}</Row>
            <Row label="State">
              {data.conn} since {localTime(data.since)}
            </Row>
            <Row label="Last error">
              {data.lastError ? `${data.lastError.kind}${data.lastError.message ? `: ${data.lastError.message}` : ""} (${localTime(data.lastError.at)})` : "none"}
            </Row>
            <Row label="Reconnects">{num(data.reconnectCount)}</Row>
            <Row label="Connected since">{localTime(data.connectedSince)}</Row>
            <Row label="Health probe">{data.health ? `${data.health.reachable ? "reachable" : "unreachable"} (${localTime(data.health.at)})` : dash}</Row>
          </dl>
        </section>
        <section aria-label="Server">
          <h3>Server</h3>
          <dl>
            <Row label="Version">{data.server.version ?? dash}</Row>
            <Row label="Hostname">{data.server.hostname ?? dash}</Row>
            <Row label="Server id">{data.server.serverId ?? dash}</Row>
            <Row label="Features">{data.server.featuresPresent.length > 0 ? data.server.featuresPresent.join(", ") : dash}</Row>
            <Row label="Missing features">{data.server.featuresMissing.length > 0 ? data.server.featuresMissing.join(", ") : "none"}</Row>
          </dl>
        </section>
        <section aria-label="Counters">
          <h3>Counters</h3>
          <dl>
            <Row label="Messages/s">{num(data.messagesPerSecond)}</Row>
            <Row label="Messages total">{num(data.messagesTotal)}</Row>
            <Row label="RTT last / avg">
              {num(data.rtt.lastMs, " ms")} / {num(data.rtt.averageMs, " ms")}
            </Row>
            <Row label="Malformed">{num(data.malformed)}</Row>
            <Row label="Unknown enum values">{num(data.unknownEnumValues)}</Row>
            <Row label="Oversized frames">{num(data.framesTooLarge)}</Row>
            <Row label="Dropped events">{num(data.droppedEvents)}</Row>
          </dl>
        </section>
        <section aria-label="Unknown message types">
          <h3>Unknown message types ({num(data.unknownTypeCount)})</h3>
          {data.unknownTypes.length === 0 ? (
            <p className={styles.none}>none</p>
          ) : (
            <ol className={styles.types}>
              {data.unknownTypes.map((t) => (
                <li key={t.name}>
                  <span className={styles.typeName} title={t.name}>
                    {t.name}
                  </span>
                  <span>{t.count.toLocaleString()}</span>
                </li>
              ))}
            </ol>
          )}
        </section>
      </div>
    </Dialog>
  );
}

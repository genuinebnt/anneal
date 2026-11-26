import { ViewPlugin, keymap, type ViewUpdate } from "@codemirror/view";
import { setDiagnostics } from "@codemirror/lint";
import { LSPClient, LSPPlugin, type LSPClientExtension, type Transport, hoverTooltips, jumpToDefinitionKeymap, signatureHelp, signatureKeymap } from "@codemirror/lsp-client";
import type { Diagnostic, Level, Span } from "../api";
import { diagCard } from "./diagcard";
import { pinDiagnostic } from "./pinned";

/** The server shows rust-analyzer this virtual root instead of the real path. */
export const ROOT = "file:///workspace";
export const LIB_URI = `${ROOT}/src/lib.rs`;

export type RaStatus = "off" | "connecting" | "indexing" | "ready" | "error";

export interface RaCallbacks {
  status: (s: RaStatus, detail?: string) => void;
  /** Error and warning counts for src/lib.rs, from rust-analyzer and its cargo check. */
  diagnostics: (errors: number, warnings: number) => void;
  /** The connection failed or dropped without us closing it; the session is no use any more. */
  lost: () => void;
}

export interface RaSession {
  client: LSPClient;
  uri: string;
  /** Resolves once the server has answered `initialize`. */
  ready: Promise<void>;
  /** Writes the buffer server-side and triggers rust-analyzer's cargo check. */
  save: (text: string) => void;
  close: () => void;
}

/** Opens a rust-analyzer session for a problem over /api/lsp/{id}. */
/** `clippy`: rust-analyzer checks with clippy rather than `cargo check` (the "Live clippy" setting). */
export function connectRa(problemId: string, on: RaCallbacks, clippy: boolean): RaSession {
  const ws = new WebSocket(`${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/api/lsp/${problemId}?clippy=${clippy}`);
  const handlers = new Set<(m: string) => void>();
  let closed = false;
  ws.onmessage = (e) => handlers.forEach((h) => h(String(e.data)));
  const transport: Transport = {
    send: (m) => ws.readyState === WebSocket.OPEN && ws.send(m),
    subscribe: (h) => handlers.add(h),
    unsubscribe: (h) => handlers.delete(h),
  };

  const client = new LSPClient({
    rootUri: ROOT,
    timeout: 15_000,
    extensions: [
      richDiagnostics(),
      hoverTooltips({ hoverTime: 350 }),
      signatureHelp(),
      keymap.of([...jumpToDefinitionKeymap, ...signatureKeymap]),
      // rust-analyzer reports when indexing is done through this experimental notification.
      // colorDiagnosticOutput: rust-analyzer passes rustc's rendered message with ANSI colours in `data.rendered`.
      { clientCapabilities: { experimental: { serverStatusNotification: true, colorDiagnosticOutput: true } } },
    ],
    notificationHandlers: {
      "experimental/serverStatus": (_c, params: { quiescent: boolean; health: string; message?: string }) => {
        if (!closed) on.status(params.health === "error" ? "error" : params.quiescent ? "ready" : "indexing", params.message);
        return true;
      },
      // Count, then let richDiagnostics draw them too.
      "textDocument/publishDiagnostics": (_c, params: { uri: string; diagnostics: { severity?: number }[] }) => {
        if (params.uri === LIB_URI && !closed) {
          on.diagnostics(params.diagnostics.filter((d) => d.severity === 1).length, params.diagnostics.filter((d) => d.severity === 2).length);
        }
        return false;
      },
    },
  });

  on.status("connecting");
  const ready = new Promise<void>((resolve, reject) => {
    ws.onopen = () => {
      client.connect(transport);
      client.initializing.then(() => {
        if (!closed) on.status("indexing");
        resolve();
      }, reject);
    };
    ws.onerror = () => reject(new Error("rust-analyzer connection failed"));
  });
  // A failed start also closes the socket, so this one handler reports every loss, with the server's reason.
  ws.onclose = (e) => {
    if (closed) return;
    on.status("error", e.reason || "connection lost");
    on.lost();
  };
  ready.catch(() => {});

  return {
    client,
    uri: LIB_URI,
    ready,
    save: (text) => client.connected && client.notification("anneal/save", { text }),
    close: () => {
      closed = true;
      if (client.connected) client.disconnect();
      ws.close();
    },
  };
}

/* ---------- diagnostics ---------- */

interface LspPosition {
  line: number;
  character: number;
}
interface LspRange {
  start: LspPosition;
  end: LspPosition;
}
interface LspDiagnostic {
  range: LspRange;
  severity?: number;
  code?: string | number;
  /** "rustc", "clippy", "rust-analyzer", … */
  source?: string;
  message: string;
  relatedInformation?: { location: { uri: string; range: LspRange }; message: string }[];
  /** rust-analyzer's copy of rustc's full rendered message (coloured, as asked for in the client capabilities). */
  data?: { rendered?: string };
}

const LEVELS: Level[] = ["error", "warning", "note", "help"];

/** An LSP diagnostic in the shape rustc's JSON gives the runner, so it renders with the same card. */
function toDiagnostic(item: LspDiagnostic, uri: string): Diagnostic {
  const span = (r: LspRange, primary: boolean, label: string | null): Span => ({
    file: "src/lib.rs",
    line_start: r.start.line + 1,
    line_end: r.end.line + 1,
    col_start: r.start.character + 1,
    col_end: r.end.character + 1,
    primary,
    label,
  });
  // rust-analyzer puts the primary span's label on the lines after the message.
  const [message, ...rest] = item.message.split("\n");
  const related = (item.relatedInformation ?? []).filter((r) => r.message !== "original diagnostic");
  const raw = item.code === undefined ? null : String(item.code);
  // Clippy's lints arrive as a bare name ("map_clone"); label them the way run output does ("clippy::map_clone").
  const code = raw && item.source === "clippy" && !raw.startsWith("clippy::") ? `clippy::${raw}` : raw;
  return {
    level: LEVELS[(item.severity ?? 1) - 1] ?? "error",
    code: code && /^\d{4}$/.test(code) ? `E${code}` : code,
    message: message ?? item.message,
    rendered: item.data?.rendered ?? item.message,
    spans: [span(item.range, true, rest.join(" ").trim() || null), ...related.filter((r) => r.location.uri === uri).map((r) => span(r.location.range, false, r.message))],
    notes: related.filter((r) => r.location.uri !== uri).map((r) => r.message),
  };
}

/** Pushes edits to the server half a second after typing stops (serverDiagnostics' own sync, which we replace). */
const autoSync = ViewPlugin.fromClass(
  class {
    pending = -1;
    update(u: ViewUpdate) {
      if (!u.docChanged) return;
      if (this.pending > -1) clearTimeout(this.pending);
      this.pending = window.setTimeout(() => {
        this.pending = -1;
        LSPPlugin.get(u.view)?.client.sync();
      }, 500);
    }
    destroy() {
      if (this.pending > -1) clearTimeout(this.pending);
    }
  },
);

/** Like lsp-client's serverDiagnostics, but hovering a squiggle shows the rustc-style card the run lens uses. */
function richDiagnostics(): LSPClientExtension {
  return {
    clientCapabilities: { textDocument: { publishDiagnostics: { versionSupport: true, relatedInformation: true } } },
    notificationHandlers: {
      "textDocument/publishDiagnostics": (client, params: { uri: string; version?: number; diagnostics: LspDiagnostic[] }) => {
        const file = client.workspace.getFile(params.uri);
        if (!file || (params.version != null && params.version != file.version)) return false;
        const view = file.getView();
        const plugin = view && LSPPlugin.get(view);
        if (!view || !plugin) return false;
        // cargo check reads the copy saved on the server, which can be longer than the buffer it is reported against.
        // A position past the end would throw and lose the whole update, leaving the old errors on screen.
        const at = (p: LspPosition) => {
          const doc = plugin.syncedDoc;
          const line = doc.line(Math.min(p.line + 1, doc.lines));
          return plugin.unsyncedChanges.mapPos(Math.min(line.from + p.character, line.to));
        };
        view.dispatch(
          setDiagnostics(
            view.state,
            params.diagnostics.map((item) => {
              const d = toDiagnostic(item, params.uri);
              return {
                from: at(item.range.start),
                to: at(item.range.end),
                severity: d.level === "error" ? "error" : d.level === "warning" ? "warning" : d.level === "note" ? "info" : "hint",
                message: item.message,
                renderMessage: (v) => diagCard(d, v.state.doc.toString().split("\n"), { onFull: () => pinDiagnostic(v, at(item.range.start), d) }),
              };
            }),
          ),
        );
        return true;
      },
    },
    editorExtension: autoSync,
  };
}

"use client";

import { useEffect, useState } from "react";
import { Check, Copy, Plug, Power, RefreshCw } from "lucide-react";
import {
  mcpStatus,
  startMcpServer,
  stopMcpServer,
} from "@/lib/tauri/integrations";
import { mediaToolsStatus } from "@/lib/tauri/media";
import type { McpStatus, MediaToolsStatus } from "@/types/vault";

export function Integrations() {
  const [status, setStatus] = useState<McpStatus | null>(null);
  const [tools, setTools] = useState<MediaToolsStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [copied, setCopied] = useState<string | null>(null);

  const refresh = async () => {
    try {
      const [s, t] = await Promise.all([mcpStatus(), mediaToolsStatus()]);
      setStatus(s);
      setTools(t);
    } catch (e) {
      setError(messageOf(e));
    }
  };

  useEffect(() => {
    void refresh();
  }, []);

  const start = async () => {
    setBusy(true);
    setError(null);
    try {
      setStatus(await startMcpServer());
    } catch (e) {
      setError(messageOf(e));
    } finally {
      setBusy(false);
    }
  };

  const stop = async () => {
    setBusy(true);
    setError(null);
    try {
      setStatus(await stopMcpServer());
    } catch (e) {
      setError(messageOf(e));
    } finally {
      setBusy(false);
    }
  };

  const copy = async (label: string, value: string) => {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(label);
      setTimeout(() => setCopied(null), 1500);
    } catch {
      // Clipboard API may be unavailable in some webviews — silently ignore.
    }
  };

  return (
    <div className="h-full w-full overflow-auto bg-[var(--color-bg)]">
      <div className="max-w-3xl mx-auto px-8 py-6 space-y-6">
        <header>
          <h1 className="text-xl font-medium text-[var(--color-text)] flex items-center gap-2">
            <Plug size={16} className="text-[var(--color-accent)]" />
            Integrations
          </h1>
          <p className="text-[var(--color-text-faint)] text-[12px] mt-1">
            Aura exposes an MCP endpoint on localhost so external tools (Claude
            Code, custom agents) can search, read, and write your vault.
          </p>
        </header>

        {error && (
          <div className="rounded border border-red-700 bg-red-950/40 px-4 py-2 text-red-300 text-[12px]">
            {error}
          </div>
        )}

        <section className="rounded-lg border border-[var(--color-border)] overflow-hidden">
          <div className="px-4 py-3 border-b border-[var(--color-border)] flex items-center gap-3">
            <h2 className="font-medium text-[var(--color-text)] text-[13px]">
              MCP server
            </h2>
            <span
              className={
                "text-[11px] px-2 py-0.5 rounded " +
                (status?.running
                  ? "bg-emerald-900/40 text-emerald-400"
                  : "bg-[var(--color-surface)] text-[var(--color-text-faint)]")
              }
            >
              {status?.running ? "running" : "stopped"}
            </span>
            <button
              type="button"
              onClick={() => void refresh()}
              title="Refresh status"
              className="ml-auto inline-flex items-center gap-1 text-[var(--color-text-faint)] hover:text-[var(--color-text)] text-[12px]"
            >
              <RefreshCw size={12} />
              Refresh
            </button>
            {status?.running ? (
              <button
                type="button"
                onClick={() => void stop()}
                disabled={busy}
                className="inline-flex items-center gap-1 px-2 py-1 rounded border border-[var(--color-border)] text-[12px] text-red-400 hover:bg-[var(--color-surface-hover)] disabled:opacity-50"
              >
                <Power size={12} />
                Stop
              </button>
            ) : (
              <button
                type="button"
                onClick={() => void start()}
                disabled={busy}
                className="inline-flex items-center gap-1 px-2 py-1 rounded bg-[var(--color-accent)] text-[var(--color-bg)] text-[12px] hover:bg-[var(--color-accent-hover)] disabled:opacity-50"
              >
                <Power size={12} />
                Start
              </button>
            )}
          </div>
          <div className="px-4 py-3 space-y-3 text-[12px]">
            {status?.running ? (
              <>
                <Field label="Endpoint">
                  <CopyableValue
                    value={status.url ?? ""}
                    copied={copied === "url"}
                    onCopy={() => void copy("url", status.url ?? "")}
                  />
                </Field>
                <Field label="Auth token (Bearer)">
                  <CopyableValue
                    value={status.auth_token ?? ""}
                    copied={copied === "token"}
                    onCopy={() => void copy("token", status.auth_token ?? "")}
                  />
                </Field>
                <Field label="Requests served">
                  <span className="text-[var(--color-text)] tabular-nums">
                    {status.request_count}
                  </span>
                </Field>
                <details className="text-[11px] text-[var(--color-text-dim)]">
                  <summary className="cursor-pointer hover:text-[var(--color-text)]">
                    Example curl
                  </summary>
                  <pre className="mt-2 bg-[var(--color-surface)] border border-[var(--color-border)] rounded p-3 overflow-auto text-[var(--color-text)]">
{`curl ${status.url} \\
  -H "Authorization: Bearer ${status.auth_token}" \\
  -H "Content-Type: application/json" \\
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'`}
                  </pre>
                </details>
              </>
            ) : (
              <p className="text-[var(--color-text-faint)]">
                The server isn't running. Start it to expose the MCP endpoint
                on <code>127.0.0.1:47820</code>. A fresh auth token is
                generated on every start, so stale tokens never work.
              </p>
            )}
          </div>
        </section>

        <section className="rounded-lg border border-[var(--color-border)] overflow-hidden">
          <div className="px-4 py-3 border-b border-[var(--color-border)]">
            <h2 className="font-medium text-[var(--color-text)] text-[13px]">
              Multimedia tools
            </h2>
            <p className="text-[var(--color-text-faint)] text-[11px] mt-1">
              Detected at startup. URL ingestion (YouTube, podcast feeds)
              requires both <code>yt-dlp</code> and <code>ffmpeg</code> to be
              on PATH.
            </p>
          </div>
          <div className="px-4 py-3 space-y-2 text-[12px]">
            <ToolRow name="yt-dlp" value={tools?.yt_dlp ?? null} />
            <ToolRow name="ffmpeg" value={tools?.ffmpeg ?? null} />
            <ToolRow name="ffprobe" value={tools?.ffprobe ?? null} />
          </div>
        </section>
      </div>
    </div>
  );
}

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-baseline gap-3">
      <span className="text-[var(--color-text-faint)] w-32 shrink-0 text-[11px] uppercase tracking-wider">
        {label}
      </span>
      <div className="flex-1 min-w-0">{children}</div>
    </div>
  );
}

function CopyableValue({
  value,
  copied,
  onCopy,
}: {
  value: string;
  copied: boolean;
  onCopy: () => void;
}) {
  return (
    <div className="flex items-center gap-2">
      <code className="flex-1 truncate text-[var(--color-text)] bg-[var(--color-surface)] border border-[var(--color-border)] rounded px-2 py-1">
        {value || "(empty)"}
      </code>
      <button
        type="button"
        onClick={onCopy}
        className="inline-flex items-center gap-1 px-2 py-1 rounded border border-[var(--color-border)] hover:bg-[var(--color-surface-hover)] text-[var(--color-text-dim)] text-[11px]"
      >
        {copied ? <Check size={11} /> : <Copy size={11} />}
        {copied ? "Copied" : "Copy"}
      </button>
    </div>
  );
}

function ToolRow({ name, value }: { name: string; value: string | null }) {
  const present = !!value;
  return (
    <div className="flex items-center gap-2">
      <span className="text-[var(--color-text)] w-20">{name}</span>
      <span
        className={
          "text-[10px] px-1.5 py-0.5 rounded uppercase tracking-wider " +
          (present
            ? "bg-emerald-900/40 text-emerald-400"
            : "bg-[var(--color-surface)] text-[var(--color-text-faint)]")
        }
      >
        {present ? "found" : "missing"}
      </span>
      <span className="text-[var(--color-text-faint)] text-[11px] truncate">
        {value ?? "not on PATH"}
      </span>
    </div>
  );
}

function messageOf(e: unknown): string {
  if (e && typeof e === "object" && "message" in e) {
    return String((e as { message: unknown }).message);
  }
  return String(e);
}

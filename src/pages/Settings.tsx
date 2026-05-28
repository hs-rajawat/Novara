import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "@/lib/ipc";

export function Settings() {
  const [paths, setPaths] = useState<string[]>([]);
  const [settings, setSettings] = useState<Record<string, unknown>>({});
  const [appInfo, setAppInfo] = useState<{
    version: string;
    data_dir: string;
  } | null>(null);

  async function load() {
    setPaths(await api.listScanPaths());
    setSettings(await api.getSettings());
    setAppInfo(await api.appInfo());
  }
  useEffect(() => {
    load();
  }, []);

  async function pick() {
    const picked = await open({ directory: true, multiple: true });
    const arr = Array.isArray(picked) ? picked : picked ? [picked] : [];
    for (const p of arr) {
      await api.addScanPath(p);
    }
    await load();
  }

  async function remove(p: string) {
    await api.removeScanPath(p);
    await load();
  }

  async function setSetting(key: string, value: unknown) {
    await api.setSetting(key, value);
    await load();
  }

  return (
    <>
      <div className="section-header">
        <h2>Scan paths</h2>
        <span className="sub">
          Folders GameVault will check for manually-installed games.
        </span>
      </div>

      <div className="list" style={{ marginBottom: 12 }}>
        {paths.length === 0 ? (
          <div className="list-row muted">No paths configured</div>
        ) : (
          paths.map((p) => (
            <div key={p} className="list-row">
              <div
                style={{
                  flex: 1,
                  fontFamily: "var(--font-mono)",
                  fontSize: 12,
                }}
              >
                {p}
              </div>
              <button className="btn btn-ghost" onClick={() => remove(p)}>
                Remove
              </button>
            </div>
          ))
        )}
      </div>
      <button className="btn btn-primary" onClick={pick}>
        Add folder…
      </button>

      <div className="section-header" style={{ marginTop: 32 }}>
        <h2>Privacy</h2>
      </div>
      <div className="list">
        <Toggle
          label="Telemetry"
          desc="Off by default. GameVault never sends data anywhere."
          value={!!settings.telemetry_enabled}
          onChange={(v) => setSetting("telemetry_enabled", v)}
        />
        <Toggle
          label="Offline mode"
          desc="Skip all network requests (metadata, recommendations, etc.)."
          value={!!settings.offline_mode}
          onChange={(v) => setSetting("offline_mode", v)}
        />
      </div>

      <div className="section-header" style={{ marginTop: 32 }}>
        <h2>About</h2>
      </div>
      <div className="list">
        <div className="list-row">
          <div style={{ flex: 1 }}>Version</div>
          <div className="muted small">{appInfo?.version ?? "—"}</div>
        </div>
        <div className="list-row">
          <div style={{ flex: 1 }}>Data directory</div>
          <div
            className="muted small"
            style={{ fontFamily: "var(--font-mono)" }}
          >
            {appInfo?.data_dir ?? "—"}
          </div>
        </div>
      </div>
    </>
  );
}

function Toggle({
  label,
  desc,
  value,
  onChange,
}: {
  label: string;
  desc: string;
  value: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <div className="list-row">
      <div style={{ flex: 1 }}>
        <div style={{ fontWeight: 600 }}>{label}</div>
        <div className="muted small">{desc}</div>
      </div>
      <button
        className={`check ${value ? "on" : ""}`}
        onClick={() => onChange(!value)}
      >
        {value ? "✓" : ""}
      </button>
    </div>
  );
}

import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { open } from "@tauri-apps/plugin-dialog";
import clsx from "clsx";
import { api } from "@/lib/ipc";
import { Icon } from "@/components/Icon";

export function Settings() {
  const navigate = useNavigate();
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

  async function importExe() {
    const picked = await open({
      multiple: false,
      filters: [
        { name: "Executables", extensions: ["exe", "bat", "sh", "AppImage"] },
      ],
    });
    if (!picked || Array.isArray(picked)) return;
    const gameId = await api.importExecutable(picked);
    navigate(`/library/${gameId}`);
  }

  return (
    <>
      <div className="page-head fade-up">
        <h2 className="page-title">Settings</h2>
        <div className="page-sub">Scanning, privacy, and app information.</div>
      </div>

      <div className="section-header">
        <h2>
          <Icon name="folder" size={15} />
          Scan paths
        </h2>
        <span className="sub">
          Folders GameVault will check for manually-installed games.
        </span>
      </div>

      <div className="list fade-up" style={{ marginBottom: 12 }}>
        {paths.length === 0 ? (
          <div className="list-row muted">No paths configured</div>
        ) : (
          paths.map((p) => (
            <div key={p} className="list-row">
              <Icon
                name="folder"
                size={15}
                style={{ color: "var(--text-tertiary)", flex: "0 0 auto" }}
              />
              <div className="mono" style={{ flex: 1, minWidth: 0 }}>
                {p}
              </div>
              <button
                className="btn btn-ghost btn-sm"
                onClick={() => remove(p)}
                title="Remove this scan path"
              >
                <Icon name="trash" size={13} />
                Remove
              </button>
            </div>
          ))
        )}
      </div>
      <div className="row" style={{ marginTop: 12, marginBottom: 8 }}>
        <button className="btn btn-primary" onClick={pick}>
          <Icon name="plus" size={14} />
          Add folder
        </button>
        <button
          className="btn"
          onClick={importExe}
          title="Import a single game by selecting its executable directly"
        >
          <Icon name="gamepad" size={14} />
          Import executable
        </button>
      </div>

      <div className="section-header" style={{ marginTop: 32 }}>
        <h2>
          <Icon name="shield" size={15} />
          Privacy
        </h2>
      </div>
      <div className="list fade-up" style={{ animationDelay: "60ms" }}>
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
        <h2>
          <Icon name="info" size={15} />
          About
        </h2>
      </div>
      <div className="list fade-up" style={{ animationDelay: "120ms" }}>
        <div className="list-row">
          <div style={{ flex: 1 }}>Version</div>
          <span className="chip">{appInfo?.version ?? "—"}</span>
        </div>
        <div className="list-row">
          <div style={{ flex: 1 }}>Data directory</div>
          <div className="mono muted">{appInfo?.data_dir ?? "—"}</div>
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
        className={clsx("switch", value && "on")}
        role="switch"
        aria-checked={value}
        aria-label={label}
        onClick={() => onChange(!value)}
      />
    </div>
  );
}

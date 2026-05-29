import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "@/lib/ipc";
import type { DetectedSavePath, SaveBackup, SaveProfile } from "@/types";
import { formatBytes, formatRelative } from "@/lib/format";

export function SaveManager() {
  const { id = "" } = useParams();
  const [profiles, setProfiles] = useState<SaveProfile[]>([]);
  const [active, setActive] = useState<string | null>(null);
  const [backups, setBackups] = useState<SaveBackup[]>([]);
  const [label, setLabel] = useState("");
  const [busy, setBusy] = useState(false);
  const [detected, setDetected] = useState<DetectedSavePath[] | null>(null);
  const [detecting, setDetecting] = useState(false);

  async function load() {
    const ps = await api.listSaveProfiles(id);
    setProfiles(ps);
    // If active profile was deleted or is no longer in list, clear selection.
    setActive((prev) => {
      if (prev && ps.some((p) => p.id === prev)) return prev;
      return ps.length > 0 ? ps[0].id : null;
    });
  }

  useEffect(() => {
    load();
  }, [id]);

  useEffect(() => {
    if (active) {
      api.listBackups(active).then(setBackups);
    } else {
      setBackups([]);
    }
  }, [active]);

  async function addManualProfile() {
    if (!label.trim()) return;
    const picked = await open({ directory: true, multiple: false });
    const dir = typeof picked === "string" ? picked : null;
    if (!dir) return;
    await api.createSaveProfile({
      game_id: id,
      label,
      source_dir: dir,
      is_manual_override: true,
    });
    setLabel("");
    await load();
  }

  async function addDetectedProfile(path: string) {
    const profileLabel = path.split(/[\\/]/).pop() ?? path;
    await api.createSaveProfile({
      game_id: id,
      label: profileLabel,
      source_dir: path,
      is_manual_override: false,
    });
    setDetected((prev) => prev?.filter((d) => d.path !== path) ?? null);
    await load();
  }

  async function deleteProfile(profileId: string) {
    if (!confirm("Delete this save profile? Existing backup files will not be removed.")) return;
    setBusy(true);
    try {
      await api.deleteSaveProfile(profileId);
      await load();
    } finally {
      setBusy(false);
    }
  }

  async function backupNow() {
    if (!active) return;
    setBusy(true);
    try {
      await api.backupNow(active);
      setBackups(await api.listBackups(active));
    } finally {
      setBusy(false);
    }
  }

  async function restore(b: SaveBackup) {
    if (!confirm(`Restore backup from ${b.created_at}? Current state will be archived first.`)) return;
    setBusy(true);
    try {
      await api.restoreBackup(b.id);
    } finally {
      setBusy(false);
    }
  }

  async function runDetection() {
    setDetecting(true);
    try {
      const found = await api.detectSavePaths(id);
      setDetected(found);
    } finally {
      setDetecting(false);
    }
  }

  return (
    <>
      <div className="row spread" style={{ marginBottom: 16 }}>
        <div>
          <Link to={`/library/${id}`} className="muted small">
            ← Game
          </Link>
          <h2 style={{ margin: "6px 0 2px" }}>Save Manager</h2>
          <div className="muted small">
            Versioned backups of your save folders. Restores are
            non-destructive — the current state is archived first.
          </div>
        </div>
        <button
          className="btn"
          onClick={runDetection}
          disabled={detecting}
          title="Search common OS locations for this game's save folder"
        >
          {detecting ? "Detecting…" : "Detect save paths"}
        </button>
      </div>

      {/* Save-path detection results */}
      {detected !== null && (
        <div className="save-detection" style={{ marginBottom: 18 }}>
          <div className="save-detection-header">
            <span style={{ fontWeight: 600, fontSize: 13 }}>Detected paths</span>
            <button
              className="btn btn-ghost small"
              style={{ fontSize: 11 }}
              onClick={() => setDetected(null)}
            >
              Dismiss
            </button>
          </div>
          {detected.length === 0 ? (
            <div className="save-detection-empty">
              No save paths found for this game in common locations.
            </div>
          ) : (
            detected.map((d) => (
              <div key={d.path} className="save-detection-row">
                <div style={{ flex: 1 }}>
                  <div
                    style={{
                      fontFamily: "var(--font-mono)",
                      fontSize: 12,
                      marginBottom: 2,
                    }}
                  >
                    {d.path}
                  </div>
                  <div className="muted small">{d.hint}</div>
                </div>
                <div className="row" style={{ gap: 6 }}>
                  <span className="save-badge auto">Auto</span>
                  <button
                    className="btn btn-primary small"
                    style={{ fontSize: 11, padding: "4px 10px" }}
                    onClick={() => addDetectedProfile(d.path)}
                  >
                    Use this path
                  </button>
                </div>
              </div>
            ))
          )}
        </div>
      )}

      {/* Add profile manually */}
      <div
        className="list"
        style={{ padding: 14, marginBottom: 18, display: "grid", gap: 8 }}
      >
        <div className="row" style={{ gap: 8 }}>
          <input
            placeholder="Profile label (e.g., Documents/MyGame/…)"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            style={{ flex: 1 }}
          />
          <button className="btn btn-primary" onClick={addManualProfile}>
            Pick folder & add
          </button>
        </div>
      </div>

      {profiles.length === 0 ? (
        <div className="empty">
          <h3>No save profiles yet</h3>
          <div>
            Click "Detect save paths" to auto-find the save folder, or pick
            one manually with "Pick folder &amp; add".
          </div>
        </div>
      ) : (
        <>
          <div className="tabs">
            {profiles.map((p) => (
              <button
                key={p.id}
                className={`tab ${active === p.id ? "active" : ""}`}
                onClick={() => setActive(p.id)}
              >
                {p.label}
                <span
                  className={`save-badge ${p.is_manual_override ? "manual" : "auto"}`}
                >
                  {p.is_manual_override ? "Manual" : "Auto"}
                </span>
              </button>
            ))}
          </div>

          {active && (
            <>
              <div className="row spread" style={{ marginBottom: 18 }}>
                <div className="row">
                  <button
                    className="btn btn-primary"
                    onClick={backupNow}
                    disabled={busy}
                  >
                    {busy ? "Working…" : "Backup now"}
                  </button>
                </div>
                <button
                  className="btn btn-ghost"
                  style={{ color: "var(--danger)" }}
                  onClick={() => deleteProfile(active)}
                  disabled={busy}
                  title="Remove this save profile (backups are kept)"
                >
                  Delete profile
                </button>
              </div>

              {backups.length === 0 ? (
                <div className="empty">No backups yet.</div>
              ) : (
                <div className="list">
                  {backups.map((b) => (
                    <div key={b.id} className="list-row">
                      <div style={{ flex: 1 }}>
                        <div style={{ fontWeight: 600 }}>
                          {formatRelative(b.created_at)}
                        </div>
                        <div className="muted small">
                          {b.file_count} files · {formatBytes(b.size_bytes)}
                          {b.note ? ` · ${b.note}` : ""}
                        </div>
                      </div>
                      <button
                        className="btn"
                        onClick={() => restore(b)}
                        disabled={busy}
                      >
                        Restore
                      </button>
                    </div>
                  ))}
                </div>
              )}
            </>
          )}
        </>
      )}
    </>
  );
}

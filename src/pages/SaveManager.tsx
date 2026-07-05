import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { open } from "@tauri-apps/plugin-dialog";
import clsx from "clsx";
import { api } from "@/lib/ipc";
import type { DetectedSavePath, SaveBackup, SaveProfile } from "@/types";
import { formatBytes, formatRelative } from "@/lib/format";
import { Icon } from "@/components/Icon";
import { EmptyState } from "@/components/EmptyState";

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
    if (
      !confirm(
        "Delete this save profile? Existing backup files will not be removed."
      )
    )
      return;
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
    if (
      !confirm(
        `Restore backup from ${b.created_at}? Current state will be archived first.`
      )
    )
      return;
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
      <div className="row spread fade-up" style={{ marginBottom: 18, alignItems: "flex-start" }}>
        <div className="page-head" style={{ marginBottom: 0 }}>
          <Link to={`/library/${id}`} className="back-link">
            <Icon name="arrow-left" size={14} />
            Game
          </Link>
          <h2 className="page-title" style={{ marginTop: 8 }}>
            Save Manager
          </h2>
          <div className="page-sub">
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
          <Icon name="search" size={14} className={detecting ? "spin" : undefined} />
          {detecting ? "Detecting…" : "Detect save paths"}
        </button>
      </div>

      {/* Save-path detection results */}
      {detected !== null && (
        <div className="save-detection fade-up" style={{ marginBottom: 20 }}>
          <div className="save-detection-header">
            <span
              style={{
                fontWeight: 600,
                fontSize: 13,
                display: "flex",
                alignItems: "center",
                gap: 7,
              }}
            >
              <Icon name="sparkles" size={14} style={{ color: "var(--accent-2)" }} />
              Detected paths
            </span>
            <button className="btn btn-ghost btn-sm" onClick={() => setDetected(null)}>
              <Icon name="x" size={13} />
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
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div className="mono" style={{ marginBottom: 2 }}>
                    {d.path}
                  </div>
                  <div className="muted small">{d.hint}</div>
                </div>
                <div className="confidence" title="Detection confidence">
                  <div className="confidence-bar">
                    <div
                      className="confidence-fill"
                      style={{ width: `${Math.round(d.confidence * 100)}%` }}
                    />
                  </div>
                  {Math.round(d.confidence * 100)}%
                </div>
                <button
                  className="btn btn-primary btn-sm"
                  onClick={() => addDetectedProfile(d.path)}
                >
                  <Icon name="plus" size={12} />
                  Use this path
                </button>
              </div>
            ))
          )}
        </div>
      )}

      {/* Add profile manually */}
      <div className="panel fade-up" style={{ marginBottom: 22, animationDelay: "60ms" }}>
        <div className="row" style={{ gap: 8 }}>
          <input
            placeholder="Profile label (e.g., Documents/MyGame/…)"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            style={{ flex: 1 }}
          />
          <button className="btn btn-primary" onClick={addManualProfile}>
            <Icon name="folder" size={14} />
            Pick folder &amp; add
          </button>
        </div>
      </div>

      {profiles.length === 0 ? (
        <EmptyState icon="save" title="No save profiles yet">
          Click "Detect save paths" to auto-find the save folder, or pick one
          manually with "Pick folder &amp; add".
        </EmptyState>
      ) : (
        <>
          <div className="seg-tabs" style={{ marginBottom: 18 }}>
            {profiles.map((p) => (
              <button
                key={p.id}
                className={clsx("seg-tab", active === p.id && "active")}
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
                <button
                  className="btn btn-primary"
                  onClick={backupNow}
                  disabled={busy}
                >
                  <Icon name="download" size={14} />
                  {busy ? "Working…" : "Backup now"}
                </button>
                <button
                  className="btn btn-danger btn-sm"
                  onClick={() => deleteProfile(active)}
                  disabled={busy}
                  title="Remove this save profile (backups are kept)"
                >
                  <Icon name="trash" size={13} />
                  Delete profile
                </button>
              </div>

              {backups.length === 0 ? (
                <EmptyState icon="clock" title="No backups yet">
                  Press "Backup now" to create the first snapshot of this save
                  folder.
                </EmptyState>
              ) : (
                <div className="list">
                  {backups.map((b) => (
                    <div key={b.id} className="list-row">
                      <div className="session-icon">
                        <Icon name="save" size={15} />
                      </div>
                      <div style={{ flex: 1, minWidth: 0 }}>
                        <div style={{ fontWeight: 600 }}>
                          {formatRelative(b.created_at)}
                        </div>
                        <div className="muted small">
                          {b.file_count} files · {formatBytes(b.size_bytes)}
                          {b.note ? ` · ${b.note}` : ""}
                        </div>
                      </div>
                      <button
                        className="btn btn-sm"
                        onClick={() => restore(b)}
                        disabled={busy}
                      >
                        <Icon name="rotate-ccw" size={13} />
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

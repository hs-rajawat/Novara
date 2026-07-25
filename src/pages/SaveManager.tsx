import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { open } from "@tauri-apps/plugin-dialog";
import clsx from "clsx";
import { api } from "@/lib/ipc";
import { notify, reportError } from "@/lib/toast";
import { useConfirm } from "@/components/ConfirmDialog";
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
  const { confirm, dialog } = useConfirm();

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
    load().catch((e) => reportError(e, "load save profiles"));
  }, [id]);

  useEffect(() => {
    if (active) {
      api
        .listBackups(active)
        .then(setBackups)
        .catch((e) => reportError(e, "load save backups"));
    } else {
      setBackups([]);
    }
  }, [active]);

  async function addManualProfile() {
    // Previously a bare `return`: clicking the button with an empty label did
    // nothing at all — the folder picker never opened and no reason was given.
    if (!label.trim()) {
      notify("Give the save profile a name first", "warning");
      return;
    }
    try {
      const picked = await open({ directory: true, multiple: false });
      const dir = typeof picked === "string" ? picked : null;
      // Cancelling the picker is a deliberate choice, not an error.
      if (!dir) return;
      await api.createSaveProfile({
        game_id: id,
        label,
        source_dir: dir,
        is_manual_override: true,
      });
      setLabel("");
      await load();
    } catch (e) {
      reportError(e, "add this save profile");
    }
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
      !(await confirm({
        title: "Delete this save profile?",
        description: "Existing backup files will not be removed.",
        confirmLabel: "Delete profile",
        tone: "danger",
        icon: "trash",
      }))
    )
      return;
    setBusy(true);
    try {
      await api.deleteSaveProfile(profileId);
      await load();
    } catch (e) {
      reportError(e, "delete this save profile");
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
    } catch (e) {
      reportError(e, "create a backup");
    } finally {
      setBusy(false);
    }
  }

  async function restore(b: SaveBackup) {
    if (
      !(await confirm({
        title: "Restore this backup?",
        description: `Your current saves will be archived first, so this can be undone. Backup taken ${formatRelative(
          b.created_at
        )}.`,
        confirmLabel: "Restore",
        tone: "danger",
        icon: "rotate-ccw",
      }))
    )
      return;
    setBusy(true);
    try {
      await api.restoreBackup(b.id);
      notify("Saves restored from backup", "success");
      setBackups(await api.listBackups(active!));
    } catch (e) {
      reportError(e, "restore this backup");
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
      {dialog}
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

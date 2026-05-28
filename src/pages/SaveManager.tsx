import { useEffect, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "@/lib/ipc";
import type { SaveBackup, SaveProfile } from "@/types";
import { formatBytes, formatRelative } from "@/lib/format";

export function SaveManager() {
  const { id = "" } = useParams();
  const [profiles, setProfiles] = useState<SaveProfile[]>([]);
  const [active, setActive] = useState<string | null>(null);
  const [backups, setBackups] = useState<SaveBackup[]>([]);
  const [label, setLabel] = useState("");
  const [busy, setBusy] = useState(false);

  async function load() {
    const ps = await api.listSaveProfiles(id);
    setProfiles(ps);
    if (ps.length && !active) setActive(ps[0].id);
  }
  useEffect(() => {
    load();
  }, [id]);

  useEffect(() => {
    if (active) {
      api.listBackups(active).then(setBackups);
    }
  }, [active]);

  async function chooseDir(): Promise<string | null> {
    const picked = await open({ directory: true, multiple: false });
    return typeof picked === "string" ? picked : null;
  }

  async function addProfile() {
    if (!label.trim()) return;
    const dir = await chooseDir();
    if (!dir) return;
    await api.createSaveProfile({
      game_id: id,
      label,
      source_dir: dir,
    });
    setLabel("");
    await load();
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
      </div>

      <div
        className="list"
        style={{ padding: 14, marginBottom: 18, display: "grid", gap: 8 }}
      >
        <div className="row" style={{ gap: 8 }}>
          <input
            placeholder="Profile label (e.g., Documents/MyGames/...)"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
            style={{ flex: 1 }}
          />
          <button className="btn btn-primary" onClick={addProfile}>
            Pick folder & add
          </button>
        </div>
      </div>

      {profiles.length === 0 ? (
        <div className="empty">
          <h3>No save profiles yet</h3>
          <div>Pick the folder where this game keeps its saves.</div>
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
              </button>
            ))}
          </div>

          <div className="row" style={{ marginBottom: 18 }}>
            <button
              className="btn btn-primary"
              onClick={backupNow}
              disabled={busy}
            >
              {busy ? "Working…" : "Backup now"}
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
                  <button className="btn" onClick={() => restore(b)} disabled={busy}>
                    Restore
                  </button>
                </div>
              ))}
            </div>
          )}
        </>
      )}
    </>
  );
}

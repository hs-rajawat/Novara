import { useState } from "react";
import { Link, useNavigate } from "react-router-dom";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "@/lib/ipc";
import { useLibrary } from "@/stores/library";
import { Icon } from "@/components/Icon";

/**
 * Full-page empty state for a library with zero games, shared by Dashboard
 * and Library so a fresh install always has a clear next step.
 */
export function EmptyLibrary() {
  const navigate = useNavigate();
  const load = useLibrary((s) => s.load);
  const [scanning, setScanning] = useState(false);
  const [importing, setImporting] = useState(false);

  async function scanLibrary() {
    setScanning(true);
    try {
      await api.scanNow();
      await load();
    } finally {
      setScanning(false);
    }
  }

  async function importGame() {
    const picked = await open({
      multiple: false,
      filters: [{ name: "Executables", extensions: ["exe", "bat", "sh", "AppImage"] }],
    });
    if (!picked || Array.isArray(picked)) return;
    setImporting(true);
    try {
      const gameId = await api.importExecutable(picked);
      await load();
      navigate(`/library/${gameId}`);
    } finally {
      setImporting(false);
    }
  }

  return (
    <div className="empty-library fade-up">
      <div className="empty-library-icon">
        <Icon name="gamepad" size={30} />
      </div>
      <h2>Your library is empty</h2>
      <p>
        Scan your configured folders for installed games, or import a single
        game directly from its executable.
      </p>
      <div className="empty-library-actions">
        <button
          type="button"
          className="btn btn-primary btn-lg"
          onClick={scanLibrary}
          disabled={scanning}
        >
          <Icon
            name={scanning ? "refresh" : "search"}
            size={15}
            className={scanning ? "spin" : undefined}
          />
          {scanning ? "Scanning…" : "Scan Library"}
        </button>
        <button
          type="button"
          className="btn btn-lg"
          onClick={importGame}
          disabled={importing}
        >
          <Icon name="package" size={15} />
          {importing ? "Importing…" : "Import Game"}
        </button>
      </div>
      <Link to="/settings" className="sub back-link">
        Configure scan paths
        <Icon name="chevron-right" size={12} />
      </Link>
    </div>
  );
}

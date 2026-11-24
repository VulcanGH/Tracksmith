import { useState } from "react";
import { FolderPlus } from "lucide-react";
import { setLocations } from "../api/client";
import { Button } from "./ui/button";

export function MediaLocations({
  roots,
  onSaved,
}: {
  roots: string[];
  onSaved: () => void;
}) {
  const [path, setPath] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  async function update(next: string[]) {
    setBusy(true);
    setError("");
    try {
      await setLocations(next);
      setPath("");
      onSaved();
    } catch (error) {
      setError(
        error instanceof Error ? error.message : "Could not update locations",
      );
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="locations-settings">
      <h2>Optional media locations</h2>
      <p className="muted">
        Open file works anywhere without adding a folder. These locations also
        allow opening files by pasted path. Symbolic links and reparse points
        are rejected. Changing locations clears open inspections, staged plans
        and unsaved editor changes.
      </p>
      <ul className="location-list">
        {roots.map((root) => (
          <li key={root}>
            <code>{root}</code>
            <Button
              variant="ghost"
              disabled={busy}
              onClick={() => void update(roots.filter((r) => r !== root))}
            >
              Remove
            </Button>
          </li>
        ))}
      </ul>
      <form
        className="path-form"
        onSubmit={(event) => {
          event.preventDefault();
          if (path.trim()) void update([...roots, path.trim()]);
        }}
      >
        <input
          aria-label="Media folder path"
          className="field"
          value={path}
          placeholder="Absolute folder path, e.g. D:\Media"
          onChange={(event) => setPath(event.target.value)}
          disabled={busy}
        />
        <Button variant="outline" disabled={busy || !path.trim()}>
          <FolderPlus />
          Add folder
        </Button>
      </form>
      {error && (
        <p role="alert" className="form-error">
          {error}
        </p>
      )}
    </section>
  );
}

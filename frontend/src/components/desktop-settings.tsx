import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import {
  getIntegration,
  saveIntegration,
  stopApplication,
} from "../api/client";
import { Button } from "./ui/button";
export function DesktopSettings() {
  const settings = useQuery({
    queryKey: ["desktop-settings"],
    queryFn: getIntegration,
  });
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  async function change(key: "startup" | "explorer", value: boolean) {
    if (!settings.data) return;
    setBusy(true);
    setMessage("");
    try {
      await saveIntegration({ ...settings.data, [key]: value });
      await settings.refetch();
    } catch (e) {
      setMessage(e instanceof Error ? e.message : "Windows setting failed");
      await settings.refetch();
    } finally {
      setBusy(false);
    }
  }
  return (
    <>
      <h2 className="tools-heading">Windows</h2>
      <div className="editor-toolbar">
        <label>
          <input
            type="checkbox"
            checked={settings.data?.startup ?? false}
            disabled={busy || !settings.data}
            onChange={(e) => void change("startup", e.target.checked)}
          />{" "}
          Start with Windows
        </label>
        <label>
          <input
            type="checkbox"
            checked={settings.data?.explorer ?? false}
            disabled={busy || !settings.data}
            onChange={(e) => void change("explorer", e.target.checked)}
          />{" "}
          Explorer: Open in Tracksmith
        </label>
      </div>
      <p className="muted">
        Applies to your Windows account using this executable’s current
        location. Keep portable copies in a stable folder before enabling.
      </p>
      <Button
        variant="outline"
        disabled={busy}
        onClick={() => {
          setBusy(true);
          void stopApplication().then(
            () =>
              setMessage(
                "Tracksmith is exiting after the current file. You can close this tab.",
              ),
            (e) => {
              setMessage(String(e));
              setBusy(false);
            },
          );
        }}
      >
        Exit after current file
      </Button>
      {(message || settings.error) && (
        <p role="status">{message || settings.error?.message}</p>
      )}
    </>
  );
}

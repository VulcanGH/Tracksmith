import { useEffect, useRef, useState } from "react";
import {
  ArrowLeft,
  ChevronLeft,
  ChevronRight,
  FileVideo,
  Folder,
  FolderOpen,
  HardDrive,
  Home,
  Search,
  Star,
  X,
} from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import {
  browseDirectory,
  chooseFolder,
  openLocation,
  setFavourite,
  type DirectoryListing,
} from "../api/client";
import { Button } from "./ui/button";
import { size } from "./media-details";
import { prettyPath } from "../lib/paths";

export function FileBrowser({
  onClose,
  onOpen,
}: {
  onClose: () => void;
  onOpen: (path: string, recent?: boolean) => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const opened = useRef(false);
  const [listing, setListing] = useState<DirectoryListing>();
  const [search, setSearch] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const client = useQueryClient();
  async function load(task: () => Promise<DirectoryListing | null>) {
    setBusy(true);
    setError("");
    try {
      const next = await task();
      if (next) setListing(next);
    } catch (error) {
      setError(
        error instanceof Error ? error.message : "Could not browse this folder",
      );
    } finally {
      setBusy(false);
    }
  }
  useEffect(() => {
    if (opened.current) return;
    opened.current = true;
    dialog.current?.showModal();
    void load(() => browseDirectory());
  }, []);
  const favourite =
    listing?.locations.some(
      (location) => location.path === listing.path && location.favourite,
    ) ?? false;
  return (
    <dialog
      ref={dialog}
      className="file-browser"
      onCancel={(event) => {
        event.preventDefault();
        onClose();
      }}
      aria-labelledby="browser-title"
    >
      <header className="dialog-heading">
        <div>
          <h2 id="browser-title">Browse media</h2>
          <span className="muted">Local files · MKV / MKA</span>
        </div>
        <Button
          variant="ghost"
          size="icon"
          onClick={onClose}
          aria-label="Close browser"
        >
          <X />
        </Button>
      </header>
      <div className="browser-toolbar">
        <Button
          variant="ghost"
          size="icon"
          title="Locations"
          aria-label="Locations"
          disabled={busy}
          onClick={() => {
            setSearch("");
            void load(() => browseDirectory());
          }}
        >
          <Home />
        </Button>
        <Button
          variant="ghost"
          size="icon"
          title="Parent folder"
          aria-label="Parent folder"
          disabled={busy || !listing?.parent}
          onClick={() => {
            setSearch("");
            void load(() => browseDirectory(listing?.parent));
          }}
        >
          <ArrowLeft />
        </Button>
        <span className="browser-path" title={listing?.path ?? "Locations"}>
          {listing?.path ? prettyPath(listing.path) : "Locations"}
        </span>
        {listing?.path && (
          <Button
            variant="ghost"
            size="icon"
            aria-label={
              favourite
                ? "Remove favourite"
                : "Favourite and keep this folder available"
            }
            title={
              favourite
                ? "Remove favourite"
                : "Favourite (keeps this folder available)"
            }
            disabled={busy}
            onClick={() =>
              void load(async () => {
                await setFavourite(listing.path!, !favourite);
                void client.invalidateQueries({ queryKey: ["status"] });
                return browseDirectory(listing.path);
              })
            }
          >
            <Star fill={favourite ? "currentColor" : "none"} />
          </Button>
        )}
        <Button
          variant="outline"
          disabled={busy}
          onClick={() => {
            setSearch("");
            void load(chooseFolder);
          }}
        >
          <FolderOpen />
          Choose folder
        </Button>
      </div>
      {error && (
        <p role="alert" className="form-error browser-error">
          {error}
        </p>
      )}
      <div className="browser-body" aria-busy={busy}>
        <aside className="browser-locations">
          {listing?.locations
            .filter(
              (location) => location.kind !== "drive" || location.authorized,
            )
            .map((location, index) => (
              <button
                disabled={busy}
                key={`${location.kind}:${location.path}:${index}`}
                title={location.path}
                onClick={() => {
                  setSearch("");
                  void load(() =>
                    location.authorized
                      ? browseDirectory(location.path)
                      : openLocation(location.path),
                  );
                }}
              >
                {location.favourite ? (
                  <Star size={13} />
                ) : location.kind === "drive" ? (
                  <HardDrive size={13} />
                ) : (
                  <Folder size={13} />
                )}
                <span>{prettyPath(location.path)}</span>
              </button>
            ))}
          {!listing?.locations.some(
            (location) => location.kind !== "drive",
          ) && (
            <p className="muted">
              Choose a folder to browse its contents. Open file remains
              available anywhere.
            </p>
          )}
          <div className="drive-summary">
            {listing?.locations
              .filter((location) => location.kind === "drive")
              .map((location) => (
                <span key={location.path}>
                  <HardDrive size={12} />
                  {location.path}
                </span>
              ))}
          </div>
        </aside>
        <section className="browser-files">
          {listing?.path ? (
            <>
              <form
                className="browser-search"
                onSubmit={(event) => {
                  event.preventDefault();
                  void load(() => browseDirectory(listing.path, search));
                }}
              >
                <Search size={14} />
                <input
                  className="field"
                  aria-label="Filter media names"
                  placeholder="Filter files and folders"
                  value={search}
                  disabled={busy}
                  onChange={(event) => setSearch(event.target.value)}
                />
                <Button variant="ghost" disabled={busy}>
                  Filter
                </Button>
              </form>
              <div className="browser-entry-list">
                {listing.entries.map((entry) => (
                  <button
                    key={entry.path}
                    disabled={busy}
                    onClick={() =>
                      entry.kind === "folder"
                        ? (setSearch(""),
                          void load(() => browseDirectory(entry.path)))
                        : onOpen(entry.path)
                    }
                  >
                    {entry.kind === "folder" ? (
                      <Folder size={16} />
                    ) : (
                      <FileVideo size={16} />
                    )}
                    <span>{entry.name}</span>
                    <small>
                      {entry.kind === "folder"
                        ? "Folder"
                        : entry.size_bytes
                          ? size(entry.size_bytes)
                          : ""}
                    </small>
                    {entry.kind === "folder" && <ChevronRight size={13} />}
                  </button>
                ))}
                {!listing.entries.length && (
                  <p className="muted">
                    {busy
                      ? "Reading folder…"
                      : "No matching folders or MKV/MKA files."}
                  </p>
                )}
              </div>
            </>
          ) : (
            <div className="recent-files">
              <h3>Recent files</h3>
              {listing?.recent_files.length ? (
                listing.recent_files.map((file) => (
                  <button
                    key={file.path}
                    disabled={busy}
                    title={file.path}
                    onClick={() => onOpen(file.path, true)}
                  >
                    <FileVideo size={16} />
                    <span>
                      {file.path.split(/[\\/]/).pop()}
                      <small>{file.path}</small>
                    </span>
                  </button>
                ))
              ) : (
                <p className="muted">Files you open will appear here.</p>
              )}
            </div>
          )}
        </section>
      </div>
      <footer className="browser-footer">
        <span>
          {busy
            ? "Working…"
            : listing?.path
              ? `${listing.total_entries} matching entries${listing.truncated ? " · scan limited" : ""}`
              : "Choose a folder or reopen a recent file"}
        </span>
        <div>
          {listing?.path && (
            <>
              <Button
                variant="ghost"
                disabled={busy || listing.offset === 0}
                onClick={() =>
                  void load(() =>
                    browseDirectory(
                      listing.path,
                      search,
                      Math.max(0, listing.offset - 200),
                    ),
                  )
                }
              >
                <ChevronLeft />
                Previous
              </Button>
              <Button
                variant="ghost"
                disabled={busy || listing.next_offset == null}
                onClick={() =>
                  void load(() =>
                    browseDirectory(listing.path, search, listing.next_offset!),
                  )
                }
              >
                Next
                <ChevronRight />
              </Button>
            </>
          )}
        </div>
      </footer>
      {listing?.notes.map((note) => (
        <p className="browser-note" key={note}>
          {note}
        </p>
      ))}
    </dialog>
  );
}

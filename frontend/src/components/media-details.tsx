import type { Inspection, Track } from "../api/client";
import type { components } from "../api/schema";

type XmlNode = components["schemas"]["XmlElement"];
export function object(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}
export function field(value: unknown, key: string): string {
  const found = object(value)[key];
  return typeof found === "string" || typeof found === "number"
    ? String(found)
    : "";
}
export function show(value: unknown) {
  return value === null || value === undefined
    ? "Absent"
    : typeof value === "object"
      ? JSON.stringify(value)
      : String(value);
}
export function size(value: string) {
  const bytes = Number(value);
  if (!Number.isFinite(bytes) || bytes < 0) return "Unknown";
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(2)} GiB`;
  if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(1)} MiB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${bytes} B`;
}
export function duration(doc: Inspection) {
  const ns = Number(field(object(doc.container_info).properties, "duration"));
  if (!Number.isFinite(ns) || ns <= 0) return "Duration unknown";
  const seconds = Math.round(ns / 1e9);
  return `${Math.floor(seconds / 3600)}:${String(Math.floor(seconds / 60) % 60).padStart(2, "0")}:${String(seconds % 60).padStart(2, "0")}`;
}
export function trackDetails(track: Track) {
  const mi = track.mediainfo;
  if (track.kind === "video")
    return [
      field(track.technical, "pixel_dimensions"),
      field(mi, "HDR_Format_Commercial") || field(mi, "HDR_Format"),
    ]
      .filter(Boolean)
      .join(" · ");
  if (track.kind === "audio")
    return [
      field(mi, "Format_Commercial") || field(mi, "Format_AdditionalFeatures"),
      `${field(track.technical, "audio_channels") || "?"} ch`,
    ]
      .filter(Boolean)
      .join(" · ");
  if (track.kind === "subtitles")
    return object(track.technical).text_subtitles === true
      ? "Text"
      : object(track.technical).text_subtitles === false
        ? "Bitmap"
        : "Unclassified";
  return "Other track";
}
export function PropertyList({ value }: { value: unknown }) {
  return (
    <dl className="technical-list">
      {Object.entries(object(value)).map(([key, item]) => (
        <div key={key}>
          <dt>{key.replaceAll("_", " ")}</dt>
          <dd>{show(item)}</dd>
        </div>
      ))}
    </dl>
  );
}
export function TrackTechnical({ track }: { track: Track }) {
  const fields =
    track.kind === "video"
      ? [
          ["Profile", "Format_Profile"],
          ["Level", "Format_Level"],
          ["Frame rate", "FrameRate"],
          ["Bit depth", "BitDepth"],
          ["HDR format", "HDR_Format"],
          ["Dolby Vision profile", "HDR_Format_Profile"],
          ["HDR compatibility", "HDR_Format_Compatibility"],
          ["Colour primaries", "colour_primaries"],
          ["Transfer", "transfer_characteristics"],
          ["Colour space", "ColorSpace"],
          ["Chroma", "ChromaSubsampling"],
          ["Bitrate", "BitRate_String"],
        ]
      : track.kind === "audio"
        ? [
            ["Commercial format", "Format_Commercial"],
            ["Profile", "Format_Profile"],
            ["Extended format", "Format_AdditionalFeatures"],
            ["Channels", "Channels"],
            ["Channel layout", "ChannelLayout"],
            ["Sample rate", "SamplingRate_String"],
            ["Bit depth", "BitDepth"],
            ["Bitrate", "BitRate_String"],
            ["Compression", "Compression_Mode"],
          ]
        : [
            ["Format", "Format"],
            ["Profile", "Format_Profile"],
            ["Encoding", "CodecID_Info"],
          ];
  return (
    <>
      <dl className="track-facts">
        {fields.map(([label, key]) => (
          <div key={key}>
            <dt>{label}</dt>
            <dd>{field(track.mediainfo, key) || "Unknown"}</dd>
          </div>
        ))}
      </dl>
      <div className="technical-columns">
        <details>
          <summary>All container track properties</summary>
          <PropertyList value={track.technical} />
        </details>
        <details>
          <summary>All MediaInfo properties</summary>
          {track.mediainfo ? (
            <PropertyList value={track.mediainfo} />
          ) : (
            <p className="muted">
              Mapping unconfirmed. Raw MediaInfo remains under Container
              information.
            </p>
          )}
        </details>
      </div>
    </>
  );
}
function child(node: XmlNode, name: string) {
  return node.children.find((item) => item.name === name);
}
function text(node: XmlNode, name: string) {
  return child(node, name)?.text ?? "";
}
function descendants(
  node: XmlNode | null | undefined,
  name: string,
): XmlNode[] {
  return node
    ? [
        ...(node.name === name ? [node] : []),
        ...node.children.flatMap((item) => descendants(item, name)),
      ]
    : [];
}
export function sectionCount(
  doc: Inspection,
  name: string,
): number | undefined {
  if (name === "Tracks") return doc.tracks.length;
  if (name === "Attachments")
    return Array.isArray(doc.attachments) ? doc.attachments.length : 0;
  if (name === "Chapters")
    return doc.chapters.available
      ? descendants(doc.chapters.tree, "ChapterAtom").length
      : undefined;
  if (name === "Tags")
    return doc.tags.available
      ? descendants(doc.tags.tree, "Simple").length
      : undefined;
  return undefined;
}
export function ContainerSection({
  doc,
  tab,
}: {
  doc: Inspection;
  tab: string;
}) {
  if (tab === "Attachments")
    return (
      <div className="metadata-content">
        <table className="component-table">
          <thead>
            <tr>
              <th>Filename</th>
              <th>MIME type</th>
              <th>Size</th>
              <th>Description / UID</th>
            </tr>
          </thead>
          <tbody>
            {(Array.isArray(doc.attachments) ? doc.attachments : []).map(
              (attachment, index) => (
                <tr key={index}>
                  <td>{field(attachment, "file_name")}</td>
                  <td>{field(attachment, "content_type")}</td>
                  <td>
                    {field(attachment, "size")
                      ? size(field(attachment, "size"))
                      : "Unknown"}
                  </td>
                  <td>
                    {field(attachment, "description") || "—"}
                    <code>
                      {field(object(attachment).properties, "uid") ||
                        field(attachment, "id")}
                    </code>
                  </td>
                </tr>
              ),
            )}
          </tbody>
        </table>
        {!sectionCount(doc, tab) && (
          <p className="muted">No attachments discovered.</p>
        )}
        <details>
          <summary>All attachment metadata</summary>
          <pre>{JSON.stringify(doc.attachments, null, 2)}</pre>
        </details>
      </div>
    );
  if (tab === "Container information")
    return (
      <div className="metadata-content">
        <PropertyList value={object(doc.container_info).properties} />
        <details>
          <summary>
            All identification data, including unknown components
          </summary>
          <pre>{JSON.stringify(doc.identification, null, 2)}</pre>
        </details>
        <details>
          <summary>All MediaInfo data</summary>
          <pre>{JSON.stringify(doc.mediainfo, null, 2)}</pre>
        </details>
      </div>
    );
  const section = tab === "Chapters" ? doc.chapters : doc.tags;
  if (!section.available)
    return (
      <p className="metadata-content" role="alert">
        Inspection unavailable: {section.error}
      </p>
    );
  return (
    <div className="metadata-content">
      {tab === "Chapters" ? (
        descendants(section.tree, "EditionEntry").map((edition, index) => (
          <section className="edition" key={index}>
            <h3>
              Edition {text(edition, "EditionUID") || index + 1}
              <span className="muted">
                {text(edition, "EditionFlagDefault") === "1"
                  ? " · Default"
                  : ""}
                {text(edition, "EditionFlagHidden") === "1" ? " · Hidden" : ""}
                {text(edition, "EditionFlagOrdered") === "1"
                  ? " · Ordered"
                  : ""}
              </span>
            </h3>
            <table className="component-table">
              <thead>
                <tr>
                  <th>Start</th>
                  <th>End</th>
                  <th>Chapter</th>
                  <th>Language</th>
                  <th>Flags</th>
                </tr>
              </thead>
              <tbody>
                {descendants(edition, "ChapterAtom").map((chapter, i) => (
                  <tr key={i}>
                    <td>{text(chapter, "ChapterTimeStart") || "Unknown"}</td>
                    <td>{text(chapter, "ChapterTimeEnd") || "—"}</td>
                    <td>
                      {chapter.children
                        .filter((n) => n.name === "ChapterDisplay")
                        .map((display) => text(display, "ChapterString"))
                        .join(" / ") || "Unnamed"}
                      <code>{text(chapter, "ChapterUID")}</code>
                    </td>
                    <td>
                      {chapter.children
                        .filter((n) => n.name === "ChapterDisplay")
                        .flatMap((display) =>
                          display.children
                            .filter((n) =>
                              [
                                "ChapterLanguage",
                                "ChapLanguage",
                                "ChapterLanguageIETF",
                                "ChapLanguageIETF",
                              ].includes(n.name),
                            )
                            .map((n) => n.text),
                        )
                        .filter(Boolean)
                        .join(", ") || "Unknown"}
                    </td>
                    <td>
                      {text(chapter, "ChapterFlagHidden") === "1"
                        ? "Hidden"
                        : "—"}
                      {text(chapter, "ChapterFlagEnabled") === "0"
                        ? " · Disabled"
                        : ""}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
        ))
      ) : (
        <table className="component-table">
          <thead>
            <tr>
              <th>Tag</th>
              <th>Value</th>
              <th>Language</th>
              <th>Target</th>
            </tr>
          </thead>
          <tbody>
            {descendants(section.tree, "Tag").flatMap((tag, index) =>
              descendants(tag, "Simple").map((simple, i) => (
                <tr key={`${index}:${i}`}>
                  <td>{text(simple, "Name")}</td>
                  <td>
                    {text(simple, "String") ||
                      text(simple, "Binary") ||
                      "Empty"}
                  </td>
                  <td>
                    {text(simple, "TagLanguageIETF") ||
                      text(simple, "TagLanguage") ||
                      "Unspecified"}
                  </td>
                  <td>
                    {child(tag, "Targets")
                      ?.children.map(
                        (target) => `${target.name}: ${target.text ?? ""}`,
                      )
                      .join(" · ") || "Global"}
                  </td>
                </tr>
              )),
            )}
          </tbody>
        </table>
      )}
      {!sectionCount(doc, tab) && (
        <p className="muted">No {tab.toLowerCase()} discovered.</p>
      )}
      <details>
        <summary>
          Complete {tab.toLowerCase()} XML, including additional properties
        </summary>
        <pre>{section.xml}</pre>
      </details>
    </div>
  );
}

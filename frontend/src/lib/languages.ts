import codes from "../../../data/iso639-2.txt?raw";
const names = new Intl.DisplayNames(["en"], { type: "language" });
const rows = codes
  .trim()
  .split(/\r?\n/)
  .map((line) => line.split("|"));
export function normalizeLanguageChoice(value: string): string {
  if (!value) return value;
  const row = rows.find((row) =>
    row.some((code) => code && code === value.toLowerCase()),
  );
  return row ? row[2] || row[1] || row[0] : value;
}
const entries = rows
  .map(([bibliographic, terminological, twoLetter]) => {
    const code = twoLetter || terminological || bibliographic;
    let name: string;
    try {
      name = names.of(code) ?? code;
    } catch {
      name = code;
    }
    return { code, name };
  })
  .filter((entry) => /^[a-z]{2,3}$/.test(entry.code));
export const languages = [
  ...entries,
  { code: "en-GB", name: "English (United Kingdom)" },
  { code: "en-US", name: "English (United States)" },
  { code: "pt-BR", name: "Portuguese (Brazil)" },
].sort((a, b) => a.name.localeCompare(b.name));

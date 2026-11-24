export function prettyPath(value: string): string {
  const unc = "\\\\?\\UNC\\";
  const local = "\\\\?\\";
  if (value.startsWith(unc)) return "\\\\" + value.slice(unc.length);
  return value.startsWith(local) ? value.slice(local.length) : value;
}

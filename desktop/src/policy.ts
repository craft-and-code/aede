/** Valid ports never silently fall back to a different listener. */
export function parsePort(value: string): number {
  if (!/^[1-9]\d{0,4}$/.test(value) || Number(value) > 65535)
    throw new Error("Choisissez un port de 1 à 65535.");
  return Number(value);
}
/** Administrative task states retain the API's terminal failure distinction. */
export const taskActive = (status: string) =>
  ["queued", "running", "cancel_requested"].includes(status);
/** Deduplicate exact native folder spellings, preserving selection order. */
export const appendRoots = (roots: readonly string[], added: readonly string[]) => [
  ...new Set([...roots, ...added]),
];

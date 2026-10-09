/** Launch targets use the same normalization as Axonkey. */
export function normalizeApplicationPath(value: string): string | null {
  const path = value.trim().replace(/^"(.*)"$/, "$1");
  if (/[\u0000-\u001f\u007f]/.test(path)) return null;
  if (!/^(?:[a-z]:[\\/]|\\\\[^\\]+\\|\/)/i.test(path)) return null;
  return /\.(?:exe|com|lnk|app)$/i.test(path) ? path : null;
}

export function normalizeWebsiteUrl(value: string): string | null {
  const input = value.trim();
  if (!input || /[\u0000-\u0020\u007f]/.test(input)) return null;
  try {
    const hasScheme = /^[a-z][a-z\d+.-]*:/i.test(input) && !/^[^/?#:\s]+:\d+(?:[/?#]|$)/.test(input);
    const url = new URL(hasScheme ? input : `https://${input}`);
    return (url.protocol === "https:" || url.protocol === "http:") && url.hostname && !url.username && !url.password ? url.href : null;
  } catch {
    return null;
  }
}

export function validLaunchTargetLength(value: string): boolean {
  return new TextEncoder().encode(value).length <= 2_048;
}

// Clipboard writes that report failure (insecure context, denied permission) instead of failing silently.

/** Copies `text`; rejects when the clipboard is unavailable or the browser refuses. */
export async function copyText(text: string): Promise<void> {
  if (!navigator.clipboard) throw new Error('The clipboard is not available here.');
  await navigator.clipboard.writeText(text);
}

export const COPY_FAILED = 'Could not copy to the clipboard. Select the text and copy it instead.';

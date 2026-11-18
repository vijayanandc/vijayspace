// Sets the browser tab's title for the page being shown ("Flows · Weft"), so tabs and history entries are named.

import { useEffect } from 'react';

export function useDocumentTitle(title: string | null | undefined): void {
  useEffect(() => {
    document.title = title ? `${title} · Weft` : 'Weft';
  }, [title]);
}

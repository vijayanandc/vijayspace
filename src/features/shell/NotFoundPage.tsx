// Shown for an address the app does not know (a mistyped or old link), instead of silently going elsewhere.

import { LinkButton } from '../../components/Button';
import { EmptyState } from '../../components/Empty';
import { useDocumentTitle } from '../../lib/useDocumentTitle';

export function NotFoundPage() {
  useDocumentTitle('Page not found');
  return (
    <div className="page">
      <section className="list-card">
        <EmptyState
          title="Page not found"
          action={
            <LinkButton to="/flows" variant="primary">
              Go to flows
            </LinkButton>
          }
        >
          This address does not match any page. The link may be old, or the item may have been deleted.
        </EmptyState>
      </section>
    </div>
  );
}

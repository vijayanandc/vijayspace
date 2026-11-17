import { Navigate, Route, Routes } from 'react-router-dom';

import { BuilderPage } from './features/builder/BuilderPage';
import { ConnectionCallbackPage } from './features/connections/ConnectionCallbackPage';
import { ConnectionsPage } from './features/connections/ConnectionsPage';
import { FlowsPage } from './features/flows/FlowsPage';
import { ExecutionPage } from './features/history/ExecutionPage';
import { HistoryPage } from './features/history/HistoryPage';
import { RecipesPage } from './features/recipes/RecipesPage';
import { Layout } from './features/shell/Layout';

export function App() {
  return (
    <Routes>
      <Route element={<Layout />}>
        <Route index element={<Navigate to="/flows" replace />} />
        <Route path="/flows" element={<FlowsPage />} />
        <Route path="/flows/:flowId" element={<BuilderPage />} />
        <Route path="/recipes" element={<RecipesPage />} />
        <Route path="/connections" element={<ConnectionsPage />} />
        <Route path="/connections/callback" element={<ConnectionCallbackPage />} />
        <Route path="/history" element={<HistoryPage />} />
        <Route path="/history/:executionId" element={<ExecutionPage />} />
        <Route path="*" element={<Navigate to="/flows" replace />} />
      </Route>
    </Routes>
  );
}

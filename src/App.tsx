// Routes: every page of the app inside the sign-in root and the shell. Pages come from each feature's index.ts.

import { Navigate, Route, Routes } from 'react-router-dom';

import { AuthRoot } from './features/auth';
import { BuilderPage } from './features/builder';
import { ConnectionCallbackPage, ConnectionsPage } from './features/connections';
import { FlowsPage } from './features/flows';
import { ExecutionPage, HistoryPage } from './features/history';
import { RecipesPage } from './features/recipes';
import { SettingsPage } from './features/settings';
import { Layout } from './features/shell';

export function App() {
  return (
    <AuthRoot>
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
        <Route path="/settings" element={<SettingsPage />} />
        <Route path="*" element={<Navigate to="/flows" replace />} />
      </Route>
      </Routes>
    </AuthRoot>
  );
}

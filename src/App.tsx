// Routes: every page of the app inside the sign-in root and the shell. Pages come from each feature's index.ts and
// load on first visit, so the first screen does not download the builder, its canvas and its code editor.

import { lazy } from 'react';
import { Navigate, Route, Routes } from 'react-router-dom';

import { AuthRoot } from './features/auth';
import { Layout, NotFoundPage } from './features/shell';

const BuilderPage = lazy(() => import('./features/builder').then((m) => ({ default: m.BuilderPage })));
const ConnectionCallbackPage = lazy(() => import('./features/connections').then((m) => ({ default: m.ConnectionCallbackPage })));
const ConnectionsPage = lazy(() => import('./features/connections').then((m) => ({ default: m.ConnectionsPage })));
const FlowsPage = lazy(() => import('./features/flows').then((m) => ({ default: m.FlowsPage })));
const ExecutionPage = lazy(() => import('./features/history').then((m) => ({ default: m.ExecutionPage })));
const HistoryPage = lazy(() => import('./features/history').then((m) => ({ default: m.HistoryPage })));
const PlatformPage = lazy(() => import('./features/platform').then((m) => ({ default: m.PlatformPage })));
const RecipesPage = lazy(() => import('./features/recipes').then((m) => ({ default: m.RecipesPage })));
const SettingsPage = lazy(() => import('./features/settings').then((m) => ({ default: m.SettingsPage })));

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
          <Route path="/settings/platform" element={<PlatformPage />} />
          <Route path="*" element={<NotFoundPage />} />
        </Route>
      </Routes>
    </AuthRoot>
  );
}

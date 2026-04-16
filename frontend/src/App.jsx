import { useEffect } from "react";
import { BrowserRouter, Routes, Route, Navigate } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import Login from "./pages/Login.jsx";
import Register from "./pages/Register.jsx";
import Planner from "./Planner.jsx";
import Routines from "./pages/Routines.jsx";
import RoutineDetail from "./pages/RoutineDetail.jsx";
import ProtectedRoute from "./components/auth/ProtectedRoute.jsx";
import AdminRoute from "./components/admin/AdminRoute.jsx";
import AdminShell from "./components/admin/AdminShell.jsx";
import AdminDashboard from "./pages/admin/AdminDashboard.jsx";
import { useAuthStore } from "./stores/authStore.js";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      refetchOnWindowFocus: false,
    },
  },
});

/**
 * AuthLoader — runs once on mount (and whenever the token changes).
 * If the user has a persisted token but role is still null (e.g. page reload
 * or fresh login where setAuth doesn't set role), call loadMe so AdminRoute
 * never spins forever.
 */
function AuthLoader() {
  const token = useAuthStore((s) => s.token);
  const role = useAuthStore((s) => s.role);
  const loadMe = useAuthStore((s) => s.loadMe);

  useEffect(() => {
    if (token && role === null) {
      loadMe();
    }
  }, [token, role, loadMe]);

  return null;
}

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <AuthLoader />
        <Routes>
          <Route path="/login" element={<Login />} />
          <Route path="/register" element={<Register />} />
          <Route
            path="/"
            element={
              <ProtectedRoute>
                <Planner />
              </ProtectedRoute>
            }
          />
          <Route
            path="/routines"
            element={
              <ProtectedRoute>
                <Routines />
              </ProtectedRoute>
            }
          />
          <Route
            path="/routines/:id"
            element={
              <ProtectedRoute>
                <RoutineDetail />
              </ProtectedRoute>
            }
          />
          <Route
            path="/admin"
            element={
              <AdminRoute>
                <AdminShell />
              </AdminRoute>
            }
          >
            <Route index element={<Navigate to="dashboard" replace />} />
            <Route path="dashboard" element={<AdminDashboard />} />
            <Route
              path="providers"
              element={<div>Providers — coming soon</div>}
            />
            <Route path="users" element={<div>Users — coming soon</div>} />
            <Route path="audit" element={<div>Audit — coming soon</div>} />
          </Route>
          <Route path="*" element={<Navigate to="/" replace />} />
        </Routes>
      </BrowserRouter>
    </QueryClientProvider>
  );
}

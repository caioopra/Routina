import { Navigate } from "react-router-dom";
import { useAuth } from "../../stores/authStore";
import useAuthStore from "../../stores/authStore";

export default function AdminRoute({ children }) {
  const { isAuthenticated } = useAuth();
  const role = useAuthStore((s) => s.role);

  if (!isAuthenticated) {
    return <Navigate to="/login" replace />;
  }

  if (role === null) {
    return (
      <div
        style={{ backgroundColor: "#08060f" }}
        className="flex min-h-screen items-center justify-center"
      >
        <span className="font-mono text-sm text-purple-400">Loading...</span>
      </div>
    );
  }

  if (role !== "admin") {
    return <Navigate to="/" replace />;
  }

  return children;
}

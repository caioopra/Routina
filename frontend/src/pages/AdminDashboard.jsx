export default function AdminDashboard() {
  return (
    <div
      style={{ backgroundColor: "#08060f" }}
      className="flex min-h-screen flex-col items-center justify-center gap-4 px-6"
    >
      <h1
        className="font-display text-3xl font-bold tracking-tight text-purple-400"
        style={{ fontFamily: "Outfit, sans-serif" }}
      >
        Admin Dashboard
      </h1>
      <p
        className="text-base text-neutral-300"
        style={{ fontFamily: "DM Sans, sans-serif" }}
      >
        Dashboard content coming in Slice D.
      </p>
    </div>
  );
}

import { useAuthStore } from "../../stores/authStore";

/**
 * ProviderToggle — inline segmented control for switching LLM providers.
 *
 * Reads from authStore.providers and dispatches selectProvider on click.
 */
export default function ProviderToggle() {
  const providers = useAuthStore((s) => s.providers);
  const selectProvider = useAuthStore((s) => s.selectProvider);

  const { available = [], selected } = providers;

  if (available.length === 0) return null;

  const onlyOne = available.length === 1;

  return (
    <div
      role="group"
      aria-label="LLM provider"
      className="flex items-center rounded-lg overflow-hidden"
      style={{ border: "1px solid rgba(139,92,246,0.2)" }}
    >
      {available.map((name) => {
        const isActive = name === selected;
        return (
          <button
            key={name}
            type="button"
            onClick={() => !isActive && selectProvider(name)}
            disabled={onlyOne}
            aria-pressed={isActive}
            className="px-2 py-0.5 text-xs capitalize transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[#8b5cf6]"
            style={{
              background: isActive
                ? "rgba(139,92,246,0.25)"
                : "rgba(139,92,246,0.04)",
              color: isActive ? "#c4b5fd" : "#6e6890",
              fontWeight: isActive ? 600 : 400,
              cursor: onlyOne ? "default" : isActive ? "default" : "pointer",
              border: "none",
            }}
          >
            {name}
          </button>
        );
      })}
    </div>
  );
}

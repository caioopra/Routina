import { useState, useEffect } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { getSettings, updateSetting } from "../../api/admin";
import StepUpModal from "../../components/admin/StepUpModal";
import KillSwitchToggle from "../../components/admin/KillSwitchToggle";

const LLM_SETTINGS = [
  {
    key: "llm_default_provider",
    label: "Default Provider",
    type: "select",
    options: ["gemini", "claude"],
  },
  {
    key: "llm_gemini_model",
    label: "Gemini Model",
    type: "text",
    placeholder: "e.g. gemini-2.0-flash",
  },
  {
    key: "llm_claude_model",
    label: "Claude Model",
    type: "text",
    placeholder: "e.g. claude-3-5-haiku-20241022",
  },
];

function getSettingValue(settings, key) {
  return settings?.find((s) => s.key === key)?.value ?? "";
}

export default function AdminProviders() {
  const queryClient = useQueryClient();

  const {
    data: settings,
    isLoading,
    isError,
  } = useQuery({
    queryKey: ["admin", "settings"],
    queryFn: getSettings,
  });

  // Local form state mirrors the settings
  const [form, setForm] = useState({
    llm_default_provider: "",
    llm_gemini_model: "",
    llm_claude_model: "",
  });

  useEffect(() => {
    if (settings) {
      setForm({
        llm_default_provider: getSettingValue(settings, "llm_default_provider"),
        llm_gemini_model: getSettingValue(settings, "llm_gemini_model"),
        llm_claude_model: getSettingValue(settings, "llm_claude_model"),
      });
    }
  }, [settings]);

  const [modalOpen, setModalOpen] = useState(false);
  const [saveError, setSaveError] = useState("");
  const [saveSuccess, setSaveSuccess] = useState(false);

  const mutation = useMutation({
    mutationFn: async (confirmToken) => {
      // Only persist keys that differ from current settings
      const updates = LLM_SETTINGS.filter(({ key }) => {
        const current = getSettingValue(settings, key);
        return form[key] !== current;
      });
      if (updates.length === 0) return;
      await Promise.all(
        updates.map(({ key }) => updateSetting(key, form[key], confirmToken)),
      );
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["admin", "settings"] });
      setSaveSuccess(true);
      setSaveError("");
      setTimeout(() => setSaveSuccess(false), 3000);
    },
    onError: (err) => {
      setSaveError(
        err?.response?.data?.error ?? err?.message ?? "Failed to save settings",
      );
    },
  });

  const hasChanges = settings
    ? LLM_SETTINGS.some(
        ({ key }) => form[key] !== getSettingValue(settings, key),
      )
    : false;

  function handleSave(e) {
    e.preventDefault();
    setSaveError("");
    setModalOpen(true);
  }

  function handleConfirm(confirmToken) {
    mutation.mutate(confirmToken);
  }

  function handleChange(key, value) {
    setForm((prev) => ({ ...prev, [key]: value }));
  }

  return (
    <div>
      <h1
        className="mb-6 text-2xl font-bold tracking-tight text-[#f1eff8]"
        style={{ fontFamily: "Outfit, sans-serif" }}
      >
        Providers
      </h1>

      {isError && (
        <div
          role="alert"
          className="mb-4 rounded-lg border border-red-500/40 bg-red-900/20 px-4 py-3 text-sm text-red-400"
        >
          Failed to load settings.
        </div>
      )}

      {isLoading ? (
        <p className="font-mono text-sm text-purple-400">Loading settings…</p>
      ) : (
        <div className="space-y-6">
          {/* Chat kill-switch */}
          <section className="rounded-xl border border-purple-500/20 bg-[#161227] p-5">
            <h2
              className="mb-3 text-base font-semibold"
              style={{ fontFamily: "Outfit, sans-serif", color: "#f1eff8" }}
            >
              Chat Feature
            </h2>
            <KillSwitchToggle settings={settings} />
          </section>

          {/* LLM settings form */}
          <section className="rounded-xl border border-purple-500/20 bg-[#161227] p-5">
            <h2
              className="mb-4 text-base font-semibold"
              style={{ fontFamily: "Outfit, sans-serif", color: "#f1eff8" }}
            >
              LLM Configuration
            </h2>

            <form onSubmit={handleSave} noValidate>
              <div className="space-y-4">
                {LLM_SETTINGS.map(
                  ({ key, label, type, options, placeholder }) => (
                    <div key={key}>
                      <label
                        htmlFor={`setting-${key}`}
                        className="mb-1 block text-xs font-medium uppercase tracking-widest text-purple-300/70"
                      >
                        {label}
                      </label>
                      {type === "select" ? (
                        <select
                          id={`setting-${key}`}
                          value={form[key]}
                          onChange={(e) => handleChange(key, e.target.value)}
                          className="w-full rounded-lg border border-purple-500/30 bg-[#1e1836] px-3 py-2 text-sm text-[#f1eff8] focus:outline-none focus:ring-2 focus:ring-purple-500/60"
                        >
                          <option value="">— select —</option>
                          {options.map((o) => (
                            <option key={o} value={o}>
                              {o}
                            </option>
                          ))}
                        </select>
                      ) : (
                        <input
                          id={`setting-${key}`}
                          type="text"
                          value={form[key]}
                          onChange={(e) => handleChange(key, e.target.value)}
                          placeholder={placeholder}
                          className="w-full rounded-lg border border-purple-500/30 bg-[#1e1836] px-3 py-2 text-sm text-[#f1eff8] placeholder-neutral-500 focus:outline-none focus:ring-2 focus:ring-purple-500/60"
                        />
                      )}
                    </div>
                  ),
                )}
              </div>

              {saveError && (
                <p
                  role="alert"
                  className="mt-4 rounded-lg border border-red-500/30 bg-red-900/20 px-3 py-2 text-sm text-red-400"
                >
                  {saveError}
                </p>
              )}

              {saveSuccess && (
                <p className="mt-4 text-sm text-green-400">
                  Settings saved successfully.
                </p>
              )}

              <div className="mt-5 flex justify-end">
                <button
                  type="submit"
                  disabled={mutation.isPending || !hasChanges}
                  className="rounded-lg bg-purple-600 px-5 py-2 text-sm font-semibold text-white transition-colors hover:bg-purple-500 disabled:opacity-50"
                  style={{ boxShadow: "0 0 8px 0 rgba(139,92,246,0.40)" }}
                >
                  {mutation.isPending ? "Saving…" : "Save Settings"}
                </button>
              </div>
            </form>
          </section>
        </div>
      )}

      <StepUpModal
        open={modalOpen}
        onClose={() => setModalOpen(false)}
        action="settings.update"
        onSuccess={handleConfirm}
      />
    </div>
  );
}

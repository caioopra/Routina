import { useState } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { updateSetting } from "../../api/admin";
import StepUpModal from "./StepUpModal";

/**
 * KillSwitchToggle — toggle for the chat_enabled admin setting.
 *
 * Props:
 *   settings  {Array<{ key: string, value: string }>}  — all settings from the
 *             parent React Query result
 */
export default function KillSwitchToggle({ settings }) {
  const [modalOpen, setModalOpen] = useState(false);
  const queryClient = useQueryClient();

  const entry = settings?.find((s) => s.key === "chat_enabled");
  const enabled = entry?.value === "true";

  const mutation = useMutation({
    mutationFn: ({ confirmToken }) => {
      const currentEntry = settings?.find((s) => s.key === "chat_enabled");
      const next = currentEntry?.value === "true" ? "false" : "true";
      return updateSetting("chat_enabled", next, confirmToken);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["admin", "settings"] });
    },
  });

  function handleToggleClick() {
    setModalOpen(true);
  }

  function handleConfirm(confirmToken) {
    mutation.mutate({ confirmToken });
  }

  return (
    <>
      <div className="flex items-center gap-3">
        {/* Status indicator */}
        <span
          className={`inline-flex h-2.5 w-2.5 rounded-full ${
            enabled ? "bg-green-400" : "bg-red-500"
          }`}
          aria-hidden="true"
        />
        <span className="text-sm text-neutral-300">
          Chat is{" "}
          <span
            className={`font-semibold ${
              enabled ? "text-green-400" : "text-red-400"
            }`}
          >
            {enabled ? "enabled" : "disabled"}
          </span>
        </span>

        <button
          type="button"
          onClick={handleToggleClick}
          disabled={mutation.isPending}
          aria-label={enabled ? "Disable chat" : "Enable chat"}
          className="ml-2 rounded-lg border border-purple-500/30 bg-[#1e1836] px-3 py-1.5 text-sm text-purple-300 transition-colors hover:border-purple-400 hover:text-purple-200 disabled:opacity-50"
        >
          {mutation.isPending ? "Saving…" : enabled ? "Disable" : "Enable"}
        </button>

        {mutation.isError && (
          <span className="text-sm text-red-400" role="alert">
            Failed to update
          </span>
        )}
      </div>

      <StepUpModal
        open={modalOpen}
        onClose={() => setModalOpen(false)}
        action="settings.update"
        onSuccess={handleConfirm}
      />
    </>
  );
}

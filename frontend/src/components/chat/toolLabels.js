/**
 * toolLabels — human-readable labels for each tool name used by the LLM.
 */
export const TOOL_LABELS = {
  create_block: "Created a block",
  update_block: "Updated a block",
  delete_block: "Deleted a block",
  create_rule: "Created a rule",
  update_rule: "Updated a rule",
  delete_rule: "Deleted a rule",
  create_label: "Created a label",
  update_label: "Updated a label",
  delete_label: "Deleted a label",
  undo_last_action: "Undone last action",
};

/**
 * getToolLabel — returns a human-readable label for a tool name.
 * Falls back to a title-cased version of the raw name if not found.
 */
export function getToolLabel(toolName) {
  return (
    TOOL_LABELS[toolName] ??
    toolName.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase())
  );
}

import { http, HttpResponse } from "msw";

let users = new Map();
let tokenCounter = 0;
const refreshTokens = new Map();

let routines = [];
let routineCounter = 0;

let blocks = [];
let blockCounter = 0;

let labels = [];
let labelCounter = 0;

let rules = [];
let ruleCounter = 0;

let conversations = [];
let conversationCounter = 0;

let chatMessages = [];
let chatMessageCounter = 0;

let providersState = { available: ["gemini", "claude"], selected: "gemini" };

export function seedRoutines(initial) {
  routines = initial.map((r) => ({ ...r }));
  routineCounter = routines.length;
}

export function seedBlocks(initial) {
  blocks = initial.map((b) => ({ ...b }));
  blockCounter = blocks.length;
}

export function seedLabels(initial) {
  labels = initial.map((l) => ({ ...l }));
  labelCounter = labels.length;
}

export function seedRules(initial) {
  rules = initial.map((r) => ({ ...r }));
  ruleCounter = rules.length;
}

function issueTokens(user) {
  tokenCounter += 1;
  const token = `token-${tokenCounter}`;
  const refresh_token = `refresh-${tokenCounter}`;
  refreshTokens.set(refresh_token, user.id);
  return { token, refresh_token };
}

export function seedConversations(initial) {
  conversations = initial.map((c) => ({ ...c }));
  conversationCounter = conversations.length;
}

export function seedChatMessages(initial) {
  chatMessages = initial.map((m) => ({ ...m }));
  chatMessageCounter = chatMessages.length;
}

export function resetMockState() {
  users = new Map();
  tokenCounter = 0;
  refreshTokens.clear();
  routines = [];
  routineCounter = 0;
  blocks = [];
  blockCounter = 0;
  labels = [];
  labelCounter = 0;
  rules = [];
  ruleCounter = 0;
  conversations = [];
  conversationCounter = 0;
  chatMessages = [];
  chatMessageCounter = 0;
  providersState = { available: ["gemini", "claude"], selected: "gemini" };
}

function requireAuth(request) {
  const auth = request.headers.get("Authorization") || "";
  return auth.startsWith("Bearer ") && auth.slice(7).length > 0;
}

export const handlers = [
  http.post("/api/auth/register", async ({ request }) => {
    const body = await request.json();
    const { email, name, password } = body || {};

    if (!email || !name || !password) {
      return HttpResponse.json({ error: "Missing fields" }, { status: 422 });
    }
    if (password.length < 8) {
      return HttpResponse.json(
        { error: "Password must be at least 8 characters" },
        { status: 422 },
      );
    }
    if (users.has(email)) {
      return HttpResponse.json(
        { error: "Email already exists" },
        { status: 409 },
      );
    }

    const user = {
      id: `user-${users.size + 1}`,
      email,
      name,
      password,
    };
    users.set(email, user);
    const { token, refresh_token } = issueTokens(user);

    return HttpResponse.json({
      user: { id: user.id, email: user.email, name: user.name },
      token,
      refresh_token,
    });
  }),

  http.post("/api/auth/login", async ({ request }) => {
    const body = await request.json();
    const { email, password } = body || {};
    const user = users.get(email);
    if (!user || user.password !== password) {
      return HttpResponse.json(
        { error: "Invalid credentials" },
        { status: 401 },
      );
    }
    const { token, refresh_token } = issueTokens(user);
    return HttpResponse.json({
      user: { id: user.id, email: user.email, name: user.name },
      token,
      refresh_token,
    });
  }),

  http.post("/api/auth/refresh", async ({ request }) => {
    const body = await request.json();
    const { refresh_token } = body || {};
    const userId = refreshTokens.get(refresh_token);
    if (!userId) {
      return HttpResponse.json(
        { error: "Invalid refresh token" },
        { status: 401 },
      );
    }
    refreshTokens.delete(refresh_token);
    const user = Array.from(users.values()).find((u) => u.id === userId);
    if (!user) {
      return HttpResponse.json(
        { error: "Invalid refresh token" },
        { status: 401 },
      );
    }
    const tokens = issueTokens(user);
    return HttpResponse.json(tokens);
  }),

  http.get("/api/routines", ({ request }) => {
    if (!requireAuth(request)) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    return HttpResponse.json(routines);
  }),

  http.post("/api/routines", async ({ request }) => {
    if (!requireAuth(request)) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    const body = (await request.json()) || {};
    if (!body.name) {
      return HttpResponse.json({ error: "Missing name" }, { status: 422 });
    }
    routineCounter += 1;
    const routine = {
      id: `routine-${routineCounter}`,
      name: body.name,
      period: body.period ?? "weekly",
      meta: body.meta ?? {},
      is_active: routines.length === 0,
    };
    routines.push(routine);
    return HttpResponse.json(routine, { status: 201 });
  }),

  http.get("/api/routines/:id", ({ request, params }) => {
    if (!requireAuth(request)) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    const routine = routines.find((r) => r.id === params.id);
    if (!routine) {
      return HttpResponse.json({ error: "Not found" }, { status: 404 });
    }
    return HttpResponse.json({
      ...routine,
      blocks: [],
      rules: [],
      summary: [],
    });
  }),

  http.put("/api/routines/:id", async ({ request, params }) => {
    if (!requireAuth(request)) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    const routine = routines.find((r) => r.id === params.id);
    if (!routine) {
      return HttpResponse.json({ error: "Not found" }, { status: 404 });
    }
    const body = (await request.json()) || {};
    if (body.name !== undefined) routine.name = body.name;
    if (body.period !== undefined) routine.period = body.period;
    if (body.meta !== undefined) routine.meta = body.meta;
    return HttpResponse.json(routine);
  }),

  http.post("/api/routines/:id/activate", ({ request, params }) => {
    if (!requireAuth(request)) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    const routine = routines.find((r) => r.id === params.id);
    if (!routine) {
      return HttpResponse.json({ error: "Not found" }, { status: 404 });
    }
    routines = routines.map((r) => ({ ...r, is_active: r.id === params.id }));
    return HttpResponse.json(routines.find((r) => r.id === params.id));
  }),

  http.delete("/api/routines/:id", ({ request, params }) => {
    if (!requireAuth(request)) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    const idx = routines.findIndex((r) => r.id === params.id);
    if (idx === -1) {
      return HttpResponse.json({ error: "Not found" }, { status: 404 });
    }
    routines.splice(idx, 1);
    return new HttpResponse(null, { status: 204 });
  }),

  http.get("/api/auth/me", ({ request }) => {
    const auth = request.headers.get("Authorization") || "";
    if (!auth.startsWith("Bearer ")) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    const token = auth.slice(7);
    if (!token || !token.startsWith("token-")) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    const user = Array.from(users.values())[0];
    if (!user) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    return HttpResponse.json({
      id: user.id,
      email: user.email,
      name: user.name,
      planner_context: user.planner_context ?? null,
      preferences: {},
    });
  }),

  // ── Blocks ──

  http.get("/api/routines/:routineId/blocks", ({ request, params }) => {
    if (!requireAuth(request)) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    const url = new URL(request.url);
    const day = url.searchParams.get("day");
    let result = blocks.filter((b) => b.routine_id === params.routineId);
    if (day !== null) {
      result = result.filter((b) => b.day_of_week === Number(day));
    }
    return HttpResponse.json(result);
  }),

  http.post("/api/routines/:routineId/blocks", async ({ request, params }) => {
    if (!requireAuth(request)) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    const body = (await request.json()) || {};
    if (body.title === undefined || body.day_of_week === undefined) {
      return HttpResponse.json(
        { error: "Missing required fields" },
        { status: 422 },
      );
    }
    blockCounter += 1;
    const block = {
      id: `block-${blockCounter}`,
      routine_id: params.routineId,
      day_of_week: body.day_of_week,
      start_time: body.start_time ?? null,
      end_time: body.end_time ?? null,
      title: body.title,
      type: body.type ?? "trabalho",
      note: body.note ?? null,
      sort_order: body.sort_order ?? 0,
      labels: [],
      subtasks: [],
    };
    blocks.push(block);
    return HttpResponse.json(block, { status: 201 });
  }),

  http.put("/api/blocks/:id", async ({ request, params }) => {
    if (!requireAuth(request)) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    const block = blocks.find((b) => b.id === params.id);
    if (!block) {
      return HttpResponse.json({ error: "Not found" }, { status: 404 });
    }
    const body = (await request.json()) || {};
    Object.assign(block, body);
    return HttpResponse.json(block);
  }),

  http.delete("/api/blocks/:id", ({ request, params }) => {
    if (!requireAuth(request)) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    const idx = blocks.findIndex((b) => b.id === params.id);
    if (idx === -1) {
      return HttpResponse.json({ error: "Not found" }, { status: 404 });
    }
    blocks.splice(idx, 1);
    return new HttpResponse(null, { status: 204 });
  }),

  // ── Labels ──

  http.get("/api/labels", ({ request }) => {
    if (!requireAuth(request)) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    return HttpResponse.json(labels);
  }),

  http.post("/api/labels", async ({ request }) => {
    if (!requireAuth(request)) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    const body = (await request.json()) || {};
    if (!body.name) {
      return HttpResponse.json({ error: "Missing name" }, { status: 422 });
    }
    labelCounter += 1;
    const label = {
      id: `label-${labelCounter}`,
      name: body.name,
      color_bg: body.color_bg ?? "#1e3a5f",
      color_text: body.color_text ?? "#93c5fd",
      color_border: body.color_border ?? "#2563eb",
      icon: body.icon ?? null,
      is_default: false,
    };
    labels.push(label);
    return HttpResponse.json(label, { status: 201 });
  }),

  http.put("/api/labels/:id", async ({ request, params }) => {
    if (!requireAuth(request)) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    const label = labels.find((l) => l.id === params.id);
    if (!label) {
      return HttpResponse.json({ error: "Not found" }, { status: 404 });
    }
    const body = (await request.json()) || {};
    Object.assign(label, body);
    return HttpResponse.json(label);
  }),

  http.delete("/api/labels/:id", ({ request, params }) => {
    if (!requireAuth(request)) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    const label = labels.find((l) => l.id === params.id);
    if (!label) {
      return HttpResponse.json({ error: "Not found" }, { status: 404 });
    }
    if (label.is_default) {
      return HttpResponse.json(
        { error: "Cannot delete default label" },
        { status: 403 },
      );
    }
    const idx = labels.findIndex((l) => l.id === params.id);
    labels.splice(idx, 1);
    return new HttpResponse(null, { status: 204 });
  }),

  // ── Rules ──

  http.get("/api/routines/:routineId/rules", ({ request, params }) => {
    if (!requireAuth(request)) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    return HttpResponse.json(
      rules.filter((r) => r.routine_id === params.routineId),
    );
  }),

  http.post("/api/routines/:routineId/rules", async ({ request, params }) => {
    if (!requireAuth(request)) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    const body = (await request.json()) || {};
    if (!body.text) {
      return HttpResponse.json({ error: "Missing text" }, { status: 422 });
    }
    ruleCounter += 1;
    const rule = {
      id: `rule-${ruleCounter}`,
      routine_id: params.routineId,
      text: body.text,
      sort_order: body.sort_order ?? 0,
    };
    rules.push(rule);
    return HttpResponse.json(rule, { status: 201 });
  }),

  http.put("/api/rules/:id", async ({ request, params }) => {
    if (!requireAuth(request)) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    const rule = rules.find((r) => r.id === params.id);
    if (!rule) {
      return HttpResponse.json({ error: "Not found" }, { status: 404 });
    }
    const body = (await request.json()) || {};
    Object.assign(rule, body);
    return HttpResponse.json(rule);
  }),

  http.delete("/api/rules/:id", ({ request, params }) => {
    if (!requireAuth(request)) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    const idx = rules.findIndex((r) => r.id === params.id);
    if (idx === -1) {
      return HttpResponse.json({ error: "Not found" }, { status: 404 });
    }
    rules.splice(idx, 1);
    return new HttpResponse(null, { status: 204 });
  }),

  // ── Conversations ──

  http.get("/api/conversations", ({ request }) => {
    if (!requireAuth(request)) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    return HttpResponse.json([...conversations].reverse());
  }),

  http.post("/api/conversations", async ({ request }) => {
    if (!requireAuth(request)) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    const body = (await request.json()) || {};
    conversationCounter += 1;
    const conv = {
      id: `conv-${conversationCounter}`,
      routine_id: body.routine_id ?? null,
      title: body.title ?? null,
      created_at: new Date().toISOString(),
    };
    conversations.push(conv);
    return HttpResponse.json(conv, { status: 201 });
  }),

  http.get("/api/conversations/:id/messages", ({ request, params }) => {
    if (!requireAuth(request)) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    const conv = conversations.find((c) => c.id === params.id);
    if (!conv) {
      return HttpResponse.json({ error: "Not found" }, { status: 404 });
    }
    const msgs = chatMessages.filter((m) => m.conversation_id === params.id);
    return HttpResponse.json(msgs);
  }),

  // ── Chat SSE ──

  http.post("/api/chat/message", async ({ request }) => {
    if (!requireAuth(request)) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }

    const body = (await request.json()) || {};
    const userText = body.message ?? "Hello";

    // Persist user message
    chatMessageCounter += 1;
    const convId = body.conversation_id;
    const routineId = body.routine_id ?? "routine-1";
    if (convId) {
      chatMessages.push({
        id: `msg-${chatMessageCounter}`,
        conversation_id: convId,
        role: "user",
        content: userText,
        created_at: new Date().toISOString(),
      });
    }

    // Detect whether the message should trigger a tool-call sequence.
    // Keywords: "create a block", "create block", "add a block" → create_block tool call.
    const lowerText = userText.toLowerCase();
    const triggerToolCall =
      lowerText.includes("create a block") ||
      lowerText.includes("create block") ||
      lowerText.includes("add a block") ||
      lowerText.includes("add block");

    const lines = [];

    // provider event
    lines.push(
      "event: provider\ndata: " + JSON.stringify({ name: "gemini" }) + "\n",
    );

    if (triggerToolCall) {
      // tokens → tool_call → tool_result → routine_updated → tokens → done
      const beforeTokens = ["Sure! ", "Let me create that block for you. "];
      for (const t of beforeTokens) {
        lines.push("event: token\ndata: " + JSON.stringify(t) + "\n");
      }

      const toolCallId = "tc-mock-1";
      const toolCallArgs = {
        routine_id: routineId,
        title: "Morning Block",
        day_of_week: 1,
        start_time: "07:00",
        end_time: "08:00",
        type: "trabalho",
      };

      lines.push(
        "event: tool_call\ndata: " +
          JSON.stringify({
            id: toolCallId,
            name: "create_block",
            args: toolCallArgs,
          }) +
          "\n",
      );

      lines.push(
        "event: tool_result\ndata: " +
          JSON.stringify({
            id: toolCallId,
            success: true,
            data: { id: "block-mock-1", ...toolCallArgs },
          }) +
          "\n",
      );

      lines.push(
        "event: routine_updated\ndata: " +
          JSON.stringify({ routine_id: routineId }) +
          "\n",
      );

      const afterTokens = ["Done! ", "The block has been added."];
      for (const t of afterTokens) {
        lines.push("event: token\ndata: " + JSON.stringify(t) + "\n");
      }
    } else {
      // Plain reply without tool calls
      const replyTokens = [
        "Sure! ",
        "I can help you ",
        "with that routine. ",
        "Let me know what changes ",
        "you would like to make.",
      ];
      for (const t of replyTokens) {
        lines.push("event: token\ndata: " + JSON.stringify(t) + "\n");
      }
    }

    lines.push(
      "event: done\ndata: " +
        JSON.stringify({
          conversation_id: convId,
          message_id: `msg-${chatMessageCounter}`,
        }) +
        "\n",
    );

    const sseBody = lines.join("\n") + "\n";

    // Persist assistant message
    if (convId) {
      chatMessageCounter += 1;
      chatMessages.push({
        id: `msg-${chatMessageCounter}`,
        conversation_id: convId,
        role: "assistant",
        content: triggerToolCall
          ? "Sure! Let me create that block for you. Done! The block has been added."
          : "Sure! I can help you with that routine. Let me know what changes you would like to make.",
        created_at: new Date().toISOString(),
      });
    }

    return new HttpResponse(sseBody, {
      status: 200,
      headers: {
        "Content-Type": "text/event-stream",
        "Cache-Control": "no-cache",
      },
    });
  }),

  // ── Settings / providers ──

  http.get("/api/settings/providers", ({ request }) => {
    if (!requireAuth(request)) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    return HttpResponse.json(providersState);
  }),

  http.post("/api/settings/llm-provider", async ({ request }) => {
    if (!requireAuth(request)) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    const body = (await request.json()) || {};
    const { provider } = body;
    if (!provider || !providersState.available.includes(provider)) {
      return HttpResponse.json({ error: "Invalid provider" }, { status: 422 });
    }
    providersState = { ...providersState, selected: provider };
    return HttpResponse.json(providersState);
  }),

  // ── Me / planner-context ──

  http.put("/api/me/planner-context", async ({ request }) => {
    if (!requireAuth(request)) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    const body = (await request.json()) || {};
    const user = Array.from(users.values())[0];
    if (!user) {
      return HttpResponse.json({ error: "Unauthorized" }, { status: 401 });
    }
    user.planner_context = body.planner_context ?? "";
    return HttpResponse.json({
      id: user.id,
      email: user.email,
      name: user.name,
      planner_context: user.planner_context,
      preferences: {},
    });
  }),
];

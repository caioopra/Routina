import { http, HttpResponse } from "msw";

let users = new Map();
let tokenCounter = 0;
const refreshTokens = new Map();

let routines = [];
let routineCounter = 0;

export function seedRoutines(initial) {
  routines = initial.map((r) => ({ ...r }));
  routineCounter = routines.length;
}

function issueTokens(user) {
  tokenCounter += 1;
  const token = `token-${tokenCounter}`;
  const refresh_token = `refresh-${tokenCounter}`;
  refreshTokens.set(refresh_token, user.id);
  return { token, refresh_token };
}

export function resetMockState() {
  users = new Map();
  tokenCounter = 0;
  refreshTokens.clear();
  routines = [];
  routineCounter = 0;
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
      preferences: {},
    });
  }),
];

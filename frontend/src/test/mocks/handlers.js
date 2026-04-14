import { http, HttpResponse } from "msw";

let users = new Map();
let tokenCounter = 0;
const refreshTokens = new Map();

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

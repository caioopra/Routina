#!/usr/bin/env bash
# Build a self-contained HTML preview of frontend/src/rotina.json under temp/.
# Opens with a double-click — no dev server, no backend, no frontend needed.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="$ROOT/frontend/src/rotina.json"
OUT_DIR="$ROOT/temp"
OUT="$OUT_DIR/index.html"

if [[ ! -f "$SRC" ]]; then
  echo "Source JSON not found: $SRC" >&2
  exit 1
fi

mkdir -p "$OUT_DIR"

JSON_ESCAPED="$(sed 's|</script|<\\/script|g' "$SRC")"

cat > "$OUT" <<HTML_HEAD
<!doctype html>
<html lang="pt-BR">
<head>
<meta charset="utf-8">
<title>Rotina preview</title>
<style>
  :root {
    --bg: #08060f;
    --surface: #0f0c1a;
    --raised: #161227;
    --overlay: #1e1836;
    --accent: #8b5cf6;
    --text: #e4e1f0;
    --muted: #8b86a3;
  }
  * { box-sizing: border-box; }
  body {
    margin: 0;
    padding: 24px;
    font-family: 'DM Sans', system-ui, sans-serif;
    background: var(--bg);
    color: var(--text);
  }
  header { margin-bottom: 24px; }
  h1 {
    font-family: 'Outfit', system-ui, sans-serif;
    margin: 0 0 4px 0;
    color: var(--accent);
  }
  .sub { color: var(--muted); font-size: 14px; }
  .meta { margin-top: 8px; font-size: 13px; color: var(--muted); }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
    gap: 16px;
  }
  .day {
    background: var(--surface);
    border: 1px solid var(--overlay);
    border-radius: 12px;
    padding: 16px;
  }
  .day h2 {
    margin: 0 0 12px 0;
    font-size: 14px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--accent);
  }
  .block {
    background: var(--raised);
    border-left: 3px solid var(--accent);
    border-radius: 6px;
    padding: 10px 12px;
    margin-bottom: 8px;
    font-size: 13px;
  }
  .block .time { color: var(--muted); font-family: 'JetBrains Mono', monospace; font-size: 12px; }
  .block .title { font-weight: 600; margin: 2px 0; }
  .block .type { font-size: 11px; padding: 2px 6px; border-radius: 4px; background: var(--overlay); display: inline-block; }
  .block .note { color: var(--muted); font-size: 12px; margin-top: 4px; font-style: italic; }
  .type-trabalho   { border-left-color: #60a5fa; }
  .type-mestrado   { border-left-color: #a78bfa; }
  .type-aula       { border-left-color: #f472b6; }
  .type-exercicio  { border-left-color: #34d399; }
  .type-slides     { border-left-color: #fbbf24; }
  .type-viagem     { border-left-color: #fb923c; }
  .type-livre      { border-left-color: #94a3b8; }
  footer { margin-top: 32px; color: var(--muted); font-size: 12px; }
</style>
</head>
<body>
<header id="header"></header>
<div class="grid" id="grid"></div>
<footer>Generated from <code>frontend/src/rotina.json</code> — regenerate with <code>make preview</code>.</footer>
<script type="application/json" id="data">
HTML_HEAD

printf '%s\n' "$JSON_ESCAPED" >> "$OUT"

cat >> "$OUT" <<'HTML_TAIL'
</script>
<script>
  const data = JSON.parse(document.getElementById('data').textContent);
  const dayNames = {
    seg: 'Segunda', ter: 'Terça', qua: 'Quarta',
    qui: 'Quinta', sex: 'Sexta', sab: 'Sábado', dom: 'Domingo'
  };

  const header = document.getElementById('header');
  header.innerHTML = `
    <h1>${data.meta?.titulo ?? 'Rotina'}</h1>
    <div class="sub">${data.meta?.subtitulo ?? ''}</div>
    <div class="meta">${data.meta?.periodo ?? ''} — ${data.meta?.metaDoMes ?? ''}</div>
  `;

  const grid = document.getElementById('grid');
  const order = ['seg','ter','qua','qui','sex','sab','dom'];
  for (const key of order) {
    const blocks = data.dias?.[key];
    if (!blocks) continue;
    const dayEl = document.createElement('div');
    dayEl.className = 'day';
    dayEl.innerHTML = `<h2>${dayNames[key] ?? key}</h2>` + blocks.map(b => `
      <div class="block type-${b.type}">
        <div class="time">${b.time ?? ''}</div>
        <div class="title">${b.title ?? ''}</div>
        <span class="type">${b.type ?? ''}</span>
        ${b.note ? `<div class="note">${b.note}</div>` : ''}
      </div>
    `).join('');
    grid.appendChild(dayEl);
  }
</script>
</body>
</html>
HTML_TAIL

echo "Preview written to $OUT"
echo "Open with: xdg-open $OUT   (or just double-click it)"

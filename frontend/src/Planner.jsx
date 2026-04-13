import { useState, useEffect } from "react";
import data from "./rotina.json";

/* ── palette ── */
const COLORS = {
  trabalho: { bg: "#1e3a5f", text: "#93c5fd", border: "#2563eb" },
  mestrado: { bg: "#3b1f4a", text: "#d8b4fe", border: "#7c3aed" },
  aula: { bg: "#4a2c1b", text: "#fdba74", border: "#ea580c" },
  exercicio: { bg: "#1a3a2a", text: "#86efac", border: "#16a34a" },
  slides: { bg: "#4a3f1b", text: "#fde68a", border: "#ca8a04" },
  viagem: { bg: "#3b3b3b", text: "#d4d4d4", border: "#737373" },
  livre: { bg: "#1e2d3d", text: "#7dd3fc", border: "#0284c7" },
};

/* surface depth tokens */
const S = {
  base: "#08060f",
  surface: "#0f0c1a",
  raised: "#161227",
  overlay: "#1e1836",
  border: "#2a2242",
  borderSubtle: "#1c1733",
  textPrimary: "#eeedf5",
  textSecondary: "#a8a3c0",
  textMuted: "#6e6890",
  accent: "#8b5cf6",
  accentDim: "#6d45d9",
  accentGlow: "rgba(139, 92, 246, 0.15)",
};

const DAYS_LABELS = {
  seg: "Segunda",
  ter: "Terça",
  qua: "Quarta",
  qui: "Quinta",
  sex: "Sexta",
  sab: "Sábado",
  dom: "Domingo",
};

const DAYS_SHORT = {
  seg: "Seg",
  ter: "Ter",
  qua: "Qua",
  qui: "Qui",
  sex: "Sex",
  sab: "Sáb",
  dom: "Dom",
};

const FONT_DISPLAY = "'Outfit', sans-serif";
const FONT_BODY = "'DM Sans', sans-serif";
const FONT_MONO = "'JetBrains Mono', monospace";

/* ── responsive hook ── */
function useMediaQuery(query) {
  const [matches, setMatches] = useState(() =>
    typeof window !== "undefined" ? window.matchMedia(query).matches : false,
  );
  useEffect(() => {
    const mql = window.matchMedia(query);
    const handler = (e) => setMatches(e.matches);
    mql.addEventListener("change", handler);
    return () => mql.removeEventListener("change", handler);
  }, [query]);
  return matches;
}

/* ── inject global styles once ── */
const STYLE_ID = "planner-styles";
function injectStyles() {
  if (document.getElementById(STYLE_ID)) return;
  const style = document.createElement("style");
  style.id = STYLE_ID;
  style.textContent = `
    @keyframes p-fadeUp {
      from { opacity: 0; transform: translateY(16px); }
      to   { opacity: 1; transform: translateY(0); }
    }
    @keyframes p-fadeIn {
      from { opacity: 0; }
      to   { opacity: 1; }
    }
    @keyframes p-slideIn {
      from { opacity: 0; transform: translateX(-10px); }
      to   { opacity: 1; transform: translateX(0); }
    }
    @keyframes p-scaleIn {
      from { opacity: 0; transform: scale(0.96); }
      to   { opacity: 1; transform: scale(1); }
    }

    .p-block {
      transition: transform 0.2s cubic-bezier(0.22, 1, 0.36, 1),
                  box-shadow 0.2s ease,
                  background 0.2s ease;
    }
    .p-block:hover {
      transform: translateY(-2px) !important;
      box-shadow: 0 6px 24px rgba(139, 92, 246, 0.12);
    }

    .p-day-col {
      transition: background 0.3s ease;
    }
    .p-day-col:hover {
      background: ${S.raised} !important;
    }

    .p-toggle-btn {
      transition: all 0.2s ease !important;
    }
    .p-toggle-btn:hover {
      background: ${S.overlay} !important;
      color: ${S.textSecondary} !important;
    }

    .p-day-pill {
      transition: all 0.2s ease !important;
    }
    .p-day-pill:hover {
      border-color: ${S.accent} !important;
      color: #c4b5fd !important;
      background: ${S.raised} !important;
    }

    .p-summary-card {
      transition: transform 0.25s cubic-bezier(0.22, 1, 0.36, 1),
                  box-shadow 0.25s ease;
    }
    .p-summary-card:hover {
      transform: translateY(-3px);
      box-shadow: 0 12px 40px rgba(139, 92, 246, 0.12);
    }

    .p-rule {
      transition: border-color 0.2s ease, background 0.2s ease;
    }
    .p-rule:hover {
      border-left-color: ${S.accent} !important;
      background: ${S.overlay} !important;
    }

    .p-meta-footer {
      transition: box-shadow 0.3s ease;
    }
    .p-meta-footer:hover {
      box-shadow: 0 0 40px rgba(139, 92, 246, 0.08), inset 0 0 40px rgba(139, 92, 246, 0.03);
    }

    /* scrollbar */
    ::-webkit-scrollbar { width: 6px; height: 6px; }
    ::-webkit-scrollbar-track { background: transparent; }
    ::-webkit-scrollbar-thumb { background: ${S.border}; border-radius: 3px; }
    ::-webkit-scrollbar-thumb:hover { background: ${S.accent}44; }

    /* noise overlay via pseudo-element on body */
    body::after {
      content: "";
      position: fixed;
      inset: 0;
      pointer-events: none;
      z-index: 9999;
      opacity: 0.025;
      background-image: url("data:image/svg+xml,%3Csvg viewBox='0 0 256 256' xmlns='http://www.w3.org/2000/svg'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='4' stitchTiles='stitch'/%3E%3C/filter%3E%3Crect width='100%25' height='100%25' filter='url(%23n)'/%3E%3C/svg%3E");
      background-repeat: repeat;
      background-size: 128px 128px;
    }
  `;
  document.head.appendChild(style);
}

/* ── components ── */

function Tag({ type, children }) {
  const c = COLORS[type] || COLORS.trabalho;
  return (
    <span
      style={{
        display: "inline-block",
        fontSize: 9,
        fontWeight: 700,
        letterSpacing: "0.08em",
        textTransform: "uppercase",
        fontFamily: FONT_BODY,
        color: c.text,
        background: `${c.bg}bb`,
        border: `1px solid ${c.border}55`,
        borderRadius: 4,
        padding: "2px 6px",
      }}
    >
      {children}
    </span>
  );
}

function Block({ time, title, type, note, index = 0, compact = false }) {
  const c = COLORS[type] || COLORS.trabalho;
  return (
    <div
      className="p-block"
      style={{
        borderLeft: `3px solid ${c.border}`,
        padding: compact ? "6px 9px" : "10px 14px",
        marginBottom: compact ? 5 : 8,
        background: `${c.bg}33`,
        borderRadius: "0 8px 8px 0",
        animation: "p-slideIn 0.35s cubic-bezier(0.22, 1, 0.36, 1) both",
        animationDelay: `${index * 45}ms`,
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 6,
          flexWrap: "wrap",
        }}
      >
        <span
          style={{
            fontSize: compact ? 10 : 11,
            color: S.textSecondary,
            fontVariantNumeric: "tabular-nums",
            fontFamily: FONT_MONO,
            whiteSpace: "nowrap",
            fontWeight: 500,
          }}
        >
          {time}
        </span>
        <Tag type={type}>{type === "exercicio" ? "exercício" : type}</Tag>
      </div>
      <div
        style={{
          fontSize: compact ? 12 : 13,
          color: S.textPrimary,
          fontWeight: 600,
          fontFamily: FONT_BODY,
          marginTop: 4,
          lineHeight: 1.35,
        }}
      >
        {title}
      </div>
      {note && !compact && (
        <div
          style={{
            fontSize: 11,
            color: S.textMuted,
            marginTop: 3,
            fontStyle: "italic",
            lineHeight: 1.45,
            fontFamily: FONT_BODY,
          }}
        >
          {note}
        </div>
      )}
      {note && compact && (
        <div
          style={{
            fontSize: 10,
            color: S.textMuted,
            marginTop: 2,
            fontStyle: "italic",
            lineHeight: 1.35,
            fontFamily: FONT_BODY,
            overflow: "hidden",
            textOverflow: "ellipsis",
            display: "-webkit-box",
            WebkitLineClamp: 2,
            WebkitBoxOrient: "vertical",
          }}
        >
          {note}
        </div>
      )}
    </div>
  );
}

function SectionLabel({ children }) {
  return (
    <h2
      style={{
        fontSize: 16,
        fontWeight: 700,
        fontFamily: FONT_DISPLAY,
        color: S.textPrimary,
        margin: "0 0 16px",
        display: "flex",
        alignItems: "center",
        gap: 10,
        letterSpacing: "-0.01em",
      }}
    >
      <span
        style={{
          width: 3,
          height: 18,
          background: `linear-gradient(180deg, ${S.accent}, ${S.accentDim})`,
          borderRadius: 2,
          display: "inline-block",
          boxShadow: `0 0 8px ${S.accentGlow}`,
        }}
      />
      {children}
    </h2>
  );
}

function DesktopGrid({ dias }) {
  const dayKeys = Object.keys(DAYS_LABELS);
  return (
    <div
      style={{
        background: S.surface,
        borderRadius: 16,
        border: `1px solid ${S.border}`,
        overflow: "hidden",
        boxShadow: `0 4px 40px rgba(0,0,0,0.3), 0 0 0 1px ${S.borderSubtle}`,
        animation: "p-scaleIn 0.4s cubic-bezier(0.22, 1, 0.36, 1) both",
      }}
    >
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(7, 1fr)",
          gap: 0,
        }}
      >
        {dayKeys.map((key, colIdx) => {
          const isWeekend = key === "sab" || key === "dom";
          const blocks = dias[key] || [];
          return (
            <div
              key={key}
              className="p-day-col"
              style={{
                borderRight:
                  colIdx < 6 ? `1px solid ${S.borderSubtle}` : "none",
                minHeight: 420,
                background: isWeekend
                  ? "rgba(139, 92, 246, 0.015)"
                  : "transparent",
                animation: "p-fadeUp 0.4s ease both",
                animationDelay: `${colIdx * 50}ms`,
              }}
            >
              {/* Day header */}
              <div
                style={{
                  padding: "16px 10px 12px",
                  borderBottom: `1px solid ${S.borderSubtle}`,
                  textAlign: "center",
                  background: isWeekend
                    ? "linear-gradient(180deg, rgba(139,92,246,0.04), transparent)"
                    : `linear-gradient(180deg, ${S.raised}, transparent)`,
                }}
              >
                <div
                  style={{
                    fontSize: 10,
                    fontWeight: 600,
                    textTransform: "uppercase",
                    letterSpacing: "0.14em",
                    fontFamily: FONT_BODY,
                    color: isWeekend ? S.textMuted : S.accent,
                  }}
                >
                  {DAYS_SHORT[key]}
                </div>
                <div
                  style={{
                    fontSize: 14,
                    fontWeight: 700,
                    fontFamily: FONT_DISPLAY,
                    color: isWeekend ? S.textMuted : S.textSecondary,
                    marginTop: 3,
                    letterSpacing: "-0.01em",
                  }}
                >
                  {DAYS_LABELS[key]}
                </div>
              </div>

              {/* Blocks */}
              <div style={{ padding: "10px 8px" }}>
                {blocks.map((block, i) => (
                  <Block key={i} {...block} index={i} compact />
                ))}
                {blocks.length === 0 && (
                  <div
                    style={{
                      textAlign: "center",
                      padding: "40px 10px",
                      color: S.textMuted,
                      fontSize: 12,
                      fontStyle: "italic",
                      fontFamily: FONT_BODY,
                      opacity: 0.6,
                    }}
                  >
                    Sem atividades
                  </div>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function MobileAgenda({ dias, selectedDay, setSelectedDay }) {
  return (
    <div
      style={{
        animation: "p-fadeUp 0.35s ease both",
      }}
    >
      {/* Day selector pills */}
      <div
        style={{
          display: "flex",
          gap: 6,
          marginBottom: 24,
          overflowX: "auto",
          paddingBottom: 4,
        }}
      >
        {Object.entries(DAYS_LABELS).map(([key, label]) => {
          const active = selectedDay === key;
          const isWeekend = key === "sab" || key === "dom";
          return (
            <button
              key={key}
              className={active ? "" : "p-day-pill"}
              onClick={() => setSelectedDay(key)}
              style={{
                flex: "0 0 auto",
                minWidth: 56,
                padding: "11px 8px",
                fontSize: 11,
                fontWeight: active ? 700 : 500,
                fontFamily: FONT_BODY,
                border: active
                  ? `2px solid ${S.accent}`
                  : `1px solid ${S.borderSubtle}`,
                borderRadius: 10,
                cursor: "pointer",
                background: active
                  ? `linear-gradient(135deg, ${S.overlay}, ${S.raised})`
                  : isWeekend
                    ? S.base
                    : S.surface,
                color: active
                  ? "#c4b5fd"
                  : isWeekend
                    ? S.textMuted
                    : S.textSecondary,
                boxShadow: active ? `0 0 16px ${S.accentGlow}` : "none",
              }}
            >
              {label.slice(0, 3)}
            </button>
          );
        })}
      </div>

      <h2
        style={{
          fontSize: 22,
          fontWeight: 700,
          color: S.textPrimary,
          fontFamily: FONT_DISPLAY,
          margin: "0 0 16px",
          letterSpacing: "-0.02em",
        }}
      >
        {DAYS_LABELS[selectedDay]}
      </h2>

      <div
        key={selectedDay}
        style={{
          background: S.surface,
          borderRadius: 14,
          border: `1px solid ${S.borderSubtle}`,
          padding: "12px 12px",
          boxShadow: `0 2px 20px rgba(0,0,0,0.2)`,
        }}
      >
        {(dias[selectedDay] || []).map((block, i) => (
          <Block key={i} {...block} index={i} />
        ))}
      </div>
    </div>
  );
}

function SummaryView({ resumo, regras, isDesktop }) {
  return (
    <div
      style={{
        display: "grid",
        gridTemplateColumns: isDesktop ? "1fr 1fr" : "1fr",
        gap: isDesktop ? 40 : 28,
        animation: "p-fadeUp 0.4s ease both",
      }}
    >
      {/* Weekly distribution */}
      <div
        style={{
          background: S.surface,
          borderRadius: 16,
          border: `1px solid ${S.borderSubtle}`,
          padding: isDesktop ? "28px 24px" : "20px 16px",
          boxShadow: `0 4px 30px rgba(0,0,0,0.25)`,
        }}
      >
        <SectionLabel>Distribuição semanal</SectionLabel>
        <div
          style={{
            display: "grid",
            gridTemplateColumns: isDesktop ? "1fr 1fr" : "1fr",
            gap: 10,
          }}
        >
          {Object.entries(resumo).map(([key, s], idx) => {
            const c = COLORS[key] || COLORS.trabalho;
            return (
              <div
                key={key}
                className="p-summary-card"
                style={{
                  display: "flex",
                  alignItems: "center",
                  gap: 14,
                  padding: "14px 16px",
                  background: `linear-gradient(135deg, ${c.bg}55, ${c.bg}22)`,
                  borderRadius: 12,
                  border: `1px solid ${c.border}33`,
                  animation: "p-fadeUp 0.4s ease both",
                  animationDelay: `${idx * 55}ms`,
                }}
              >
                <span style={{ fontSize: 24, lineHeight: 1 }}>{s.icon}</span>
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div
                    style={{
                      fontSize: 13,
                      fontWeight: 600,
                      color: c.text,
                      fontFamily: FONT_BODY,
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {s.label}
                  </div>
                </div>
                <span
                  style={{
                    fontSize: 15,
                    fontWeight: 700,
                    color: S.textPrimary,
                    fontFamily: FONT_MONO,
                    whiteSpace: "nowrap",
                  }}
                >
                  {s.hours}
                </span>
              </div>
            );
          })}
        </div>
      </div>

      {/* Rules */}
      <div
        style={{
          background: S.surface,
          borderRadius: 16,
          border: `1px solid ${S.borderSubtle}`,
          padding: isDesktop ? "28px 24px" : "20px 16px",
          boxShadow: `0 4px 30px rgba(0,0,0,0.25)`,
        }}
      >
        <SectionLabel>Regras do mês</SectionLabel>
        <div style={{ display: "flex", flexDirection: "column", gap: 10 }}>
          {regras.map((note, i) => (
            <div
              key={i}
              className="p-rule"
              style={{
                fontSize: 13,
                color: S.textSecondary,
                fontFamily: FONT_BODY,
                padding: "14px 18px",
                background: S.raised,
                borderRadius: 10,
                lineHeight: 1.65,
                borderLeft: `3px solid ${S.border}`,
                animation: "p-fadeUp 0.4s ease both",
                animationDelay: `${i * 55}ms`,
              }}
            >
              {note}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

/* ── main component ── */

export default function Planner() {
  const [selectedDay, setSelectedDay] = useState("seg");
  const [showSummary, setShowSummary] = useState(false);
  const isDesktop = useMediaQuery("(min-width: 900px)");

  useEffect(() => {
    injectStyles();
  }, []);

  const { meta, dias, resumo, regras } = data;

  return (
    <div
      style={{
        minHeight: "100vh",
        background: `linear-gradient(170deg, ${S.base} 0%, #0a0814 50%, ${S.base} 100%)`,
        color: S.textPrimary,
        fontFamily: FONT_BODY,
        padding: isDesktop ? "44px 56px" : "24px 16px",
      }}
    >
      <link
        href="https://fonts.googleapis.com/css2?family=Outfit:wght@400;500;600;700;800&family=DM+Sans:ital,wght@0,400;0,500;0,600;0,700;1,400&family=JetBrains+Mono:wght@400;500;600&display=swap"
        rel="stylesheet"
      />

      {/* ── Header ── */}
      <header
        style={{
          marginBottom: isDesktop ? 36 : 28,
          display: isDesktop ? "flex" : "block",
          alignItems: "flex-end",
          justifyContent: "space-between",
          gap: 24,
          paddingBottom: isDesktop ? 28 : 20,
          borderBottom: `1px solid ${S.borderSubtle}`,
          animation: "p-fadeUp 0.5s ease both",
        }}
      >
        <div>
          <div
            style={{
              fontSize: 11,
              letterSpacing: "0.18em",
              textTransform: "uppercase",
              color: S.accent,
              fontWeight: 600,
              fontFamily: FONT_BODY,
            }}
          >
            planejamento {meta.periodo.toLowerCase()}
          </div>
          <h1
            style={{
              fontSize: isDesktop ? 32 : 24,
              fontWeight: 800,
              fontFamily: FONT_DISPLAY,
              margin: "8px 0 0",
              color: S.textPrimary,
              lineHeight: 1.15,
              letterSpacing: "-0.03em",
            }}
          >
            {meta.titulo}
          </h1>
          <p
            style={{
              fontSize: 14,
              color: S.textSecondary,
              margin: "6px 0 0",
              fontFamily: FONT_BODY,
              fontWeight: 400,
            }}
          >
            {meta.subtitulo}
          </p>
        </div>

        {/* Toggle */}
        <div
          style={{
            display: "flex",
            gap: 0,
            marginTop: isDesktop ? 0 : 24,
            background: S.surface,
            borderRadius: 12,
            padding: 4,
            border: `1px solid ${S.border}`,
            flexShrink: 0,
            boxShadow: `0 2px 12px rgba(0,0,0,0.2)`,
          }}
        >
          {[
            { key: false, label: "Agenda" },
            { key: true, label: "Resumo & Regras" },
          ].map(({ key, label }) => {
            const active = showSummary === key;
            return (
              <button
                key={label}
                className={active ? "" : "p-toggle-btn"}
                onClick={() => setShowSummary(key)}
                style={{
                  padding: "10px 24px",
                  fontSize: 13,
                  fontWeight: active ? 700 : 500,
                  fontFamily: FONT_BODY,
                  border: "none",
                  borderRadius: 9,
                  cursor: "pointer",
                  background: active
                    ? `linear-gradient(135deg, ${S.accent}22, ${S.overlay})`
                    : "transparent",
                  color: active ? "#c4b5fd" : S.textMuted,
                  boxShadow: active
                    ? `0 0 12px ${S.accentGlow}, inset 0 0 0 1px ${S.accent}33`
                    : "none",
                }}
              >
                {label}
              </button>
            );
          })}
        </div>
      </header>

      {/* ── Content ── */}
      {!showSummary ? (
        isDesktop ? (
          <DesktopGrid dias={dias} />
        ) : (
          <MobileAgenda
            dias={dias}
            selectedDay={selectedDay}
            setSelectedDay={setSelectedDay}
          />
        )
      ) : (
        <SummaryView resumo={resumo} regras={regras} isDesktop={isDesktop} />
      )}

      {/* ── Meta do mês footer ── */}
      <div
        className="p-meta-footer"
        style={{
          marginTop: isDesktop ? 36 : 28,
          padding: "20px 24px",
          background: `linear-gradient(135deg, ${S.surface}, ${S.raised})`,
          borderRadius: 14,
          border: `1px solid ${S.border}`,
          boxShadow: `0 0 30px ${S.accentGlow}`,
          animation: "p-fadeUp 0.5s ease both",
          animationDelay: "150ms",
        }}
      >
        <div
          style={{
            fontSize: 10,
            color: S.accent,
            textTransform: "uppercase",
            fontWeight: 700,
            fontFamily: FONT_BODY,
            letterSpacing: "0.14em",
            marginBottom: 8,
          }}
        >
          Meta do mês
        </div>
        <div
          style={{
            fontSize: 14,
            color: "#fde68a",
            lineHeight: 1.6,
            fontWeight: 500,
            fontFamily: FONT_BODY,
          }}
        >
          {meta.metaDoMes}
        </div>
      </div>
    </div>
  );
}

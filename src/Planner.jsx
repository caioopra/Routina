import { useState, useEffect, useCallback } from "react";
import data from "./rotina.json";

const COLORS = {
  trabalho:  { bg: "#1e3a5f", text: "#93c5fd", border: "#2563eb" },
  mestrado:  { bg: "#3b1f4a", text: "#d8b4fe", border: "#7c3aed" },
  aula:      { bg: "#4a2c1b", text: "#fdba74", border: "#ea580c" },
  exercicio: { bg: "#1a3a2a", text: "#86efac", border: "#16a34a" },
  slides:    { bg: "#4a3f1b", text: "#fde68a", border: "#ca8a04" },
  viagem:    { bg: "#3b3b3b", text: "#d4d4d4", border: "#737373" },
  livre:     { bg: "#1e2d3d", text: "#7dd3fc", border: "#0284c7" },
};

const DAYS_LABELS = {
  seg: "Segunda", ter: "Terça", qua: "Quarta",
  qui: "Quinta", sex: "Sexta", sab: "Sábado", dom: "Domingo",
};

const DAYS_SHORT = {
  seg: "Seg", ter: "Ter", qua: "Qua",
  qui: "Qui", sex: "Sex", sab: "Sáb", dom: "Dom",
};

function useMediaQuery(query) {
  const [matches, setMatches] = useState(() =>
    typeof window !== "undefined" ? window.matchMedia(query).matches : false
  );
  useEffect(() => {
    const mql = window.matchMedia(query);
    const handler = (e) => setMatches(e.matches);
    mql.addEventListener("change", handler);
    return () => mql.removeEventListener("change", handler);
  }, [query]);
  return matches;
}

/* inject keyframes once */
const STYLE_ID = "planner-keyframes";
function injectKeyframes() {
  if (document.getElementById(STYLE_ID)) return;
  const style = document.createElement("style");
  style.id = STYLE_ID;
  style.textContent = `
    @keyframes planner-fadeUp {
      from { opacity: 0; transform: translateY(12px); }
      to   { opacity: 1; transform: translateY(0); }
    }
    @keyframes planner-slideIn {
      from { opacity: 0; transform: translateX(-8px); }
      to   { opacity: 1; transform: translateX(0); }
    }
    @keyframes planner-pulse {
      0%, 100% { opacity: 0.4; }
      50%      { opacity: 0.7; }
    }
    .planner-block:hover {
      transform: translateY(-1px);
      box-shadow: 0 4px 20px rgba(124, 58, 237, 0.15);
    }
    .planner-day-col:hover {
      background: rgba(124, 58, 237, 0.04) !important;
    }
    .planner-toggle-btn:hover {
      background: #2a1f3d !important;
      color: #d8b4fe !important;
    }
    .planner-day-btn:hover {
      border-color: #7c3aed !important;
      color: #d8b4fe !important;
    }
    .planner-summary-card:hover {
      transform: translateY(-2px);
      box-shadow: 0 8px 30px rgba(124, 58, 237, 0.12);
    }
    .planner-rule:hover {
      border-left-color: #7c3aed !important;
      background: #1a1230 !important;
    }
    /* custom scrollbar for the grid */
    .planner-grid::-webkit-scrollbar { height: 6px; }
    .planner-grid::-webkit-scrollbar-track { background: transparent; }
    .planner-grid::-webkit-scrollbar-thumb { background: #2a1f3d; border-radius: 3px; }
  `;
  document.head.appendChild(style);
}

function Tag({ type, children }) {
  const c = COLORS[type] || COLORS.trabalho;
  return (
    <span style={{
      display: "inline-block", fontSize: 10, fontWeight: 700,
      letterSpacing: "0.06em", textTransform: "uppercase",
      color: c.text, background: `${c.bg}cc`,
      border: `1px solid ${c.border}44`, borderRadius: 4,
      padding: "2px 7px",
    }}>
      {children}
    </span>
  );
}

function Block({ time, title, type, note, index = 0, compact = false }) {
  const c = COLORS[type] || COLORS.trabalho;
  return (
    <div
      className="planner-block"
      style={{
        borderLeft: `3px solid ${c.border}`,
        padding: compact ? "5px 8px" : "8px 12px",
        marginBottom: compact ? 4 : 6,
        background: `${c.bg}44`,
        borderRadius: "0 8px 8px 0",
        transition: "all 0.2s ease",
        animation: "planner-slideIn 0.3s ease both",
        animationDelay: `${index * 40}ms`,
      }}
    >
      <div style={{ display: "flex", alignItems: "center", gap: 6, flexWrap: "wrap" }}>
        <span style={{
          fontSize: compact ? 10 : 11,
          color: "#8b8fa3",
          fontVariantNumeric: "tabular-nums",
          fontFamily: "'JetBrains Mono', monospace",
          whiteSpace: "nowrap",
        }}>
          {time}
        </span>
        <Tag type={type}>{type === "exercicio" ? "exercício" : type}</Tag>
      </div>
      <div style={{
        fontSize: compact ? 12 : 13,
        color: "#e2e8f0",
        fontWeight: 600,
        marginTop: 3,
        lineHeight: 1.3,
      }}>
        {title}
      </div>
      {note && !compact && (
        <div style={{
          fontSize: 11, color: "#6b7194", marginTop: 2,
          fontStyle: "italic", lineHeight: 1.4,
        }}>
          {note}
        </div>
      )}
      {note && compact && (
        <div style={{
          fontSize: 10, color: "#6b7194", marginTop: 1,
          fontStyle: "italic", lineHeight: 1.3,
          overflow: "hidden", textOverflow: "ellipsis",
          display: "-webkit-box", WebkitLineClamp: 2, WebkitBoxOrient: "vertical",
        }}>
          {note}
        </div>
      )}
    </div>
  );
}

function DesktopGrid({ dias }) {
  const dayKeys = Object.keys(DAYS_LABELS);
  return (
    <div
      className="planner-grid"
      style={{
        display: "grid",
        gridTemplateColumns: "repeat(7, 1fr)",
        gap: 0,
        borderRadius: 12,
        overflow: "hidden",
        border: "1px solid #1e1535",
        background: "#0d0a18",
      }}
    >
      {dayKeys.map((key, colIdx) => {
        const isWeekend = key === "sab" || key === "dom";
        const blocks = dias[key] || [];
        return (
          <div
            key={key}
            className="planner-day-col"
            style={{
              borderRight: colIdx < 6 ? "1px solid #1e1535" : "none",
              padding: 0,
              minHeight: 400,
              background: isWeekend ? "rgba(124, 58, 237, 0.02)" : "transparent",
              transition: "background 0.2s ease",
              animation: "planner-fadeUp 0.4s ease both",
              animationDelay: `${colIdx * 60}ms`,
            }}
          >
            {/* Day header */}
            <div style={{
              padding: "14px 10px 10px",
              borderBottom: "1px solid #1e1535",
              textAlign: "center",
              background: "rgba(124, 58, 237, 0.05)",
            }}>
              <div style={{
                fontSize: 10,
                fontWeight: 700,
                textTransform: "uppercase",
                letterSpacing: "0.12em",
                color: isWeekend ? "#6b7194" : "#9b8dc7",
              }}>
                {DAYS_SHORT[key]}
              </div>
              <div style={{
                fontSize: 13,
                fontWeight: 600,
                color: isWeekend ? "#4a4d6a" : "#c4b5e3",
                marginTop: 2,
              }}>
                {DAYS_LABELS[key]}
              </div>
            </div>

            {/* Blocks */}
            <div style={{ padding: "8px 6px" }}>
              {blocks.map((block, i) => (
                <Block key={i} {...block} index={i} compact />
              ))}
              {blocks.length === 0 && (
                <div style={{
                  textAlign: "center", padding: "30px 10px",
                  color: "#3a3d5c", fontSize: 12, fontStyle: "italic",
                }}>
                  Sem atividades
                </div>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}

function MobileAgenda({ dias, selectedDay, setSelectedDay }) {
  return (
    <>
      {/* Day selector pills */}
      <div style={{
        display: "flex", gap: 5, marginBottom: 20,
        overflowX: "auto", paddingBottom: 4,
      }}>
        {Object.entries(DAYS_LABELS).map(([key, label]) => {
          const active = selectedDay === key;
          const isWeekend = key === "sab" || key === "dom";
          return (
            <button
              key={key}
              className={active ? "" : "planner-day-btn"}
              onClick={() => setSelectedDay(key)}
              style={{
                flex: "0 0 auto", minWidth: 54, padding: "10px 8px",
                fontSize: 11, fontWeight: active ? 700 : 500,
                border: active ? "1.5px solid #7c3aed" : "1px solid #1e1535",
                borderRadius: 10, cursor: "pointer",
                background: active
                  ? "linear-gradient(135deg, #2a1f4a, #1e1535)"
                  : isWeekend ? "#0d0a18" : "#12101f",
                color: active ? "#d8b4fe" : isWeekend ? "#3a3d5c" : "#6b7194",
                transition: "all 0.2s ease",
                fontFamily: "inherit",
              }}
            >
              {label.slice(0, 3)}
            </button>
          );
        })}
      </div>

      <h2 style={{
        fontSize: 18, fontWeight: 700, color: "#e2e8f0",
        margin: "0 0 14px",
        animation: "planner-fadeUp 0.3s ease both",
      }}>
        {DAYS_LABELS[selectedDay]}
      </h2>

      <div key={selectedDay}>
        {(dias[selectedDay] || []).map((block, i) => (
          <Block key={i} {...block} index={i} />
        ))}
      </div>
    </>
  );
}

function SummaryView({ resumo, regras, isDesktop }) {
  return (
    <div style={{
      display: "grid",
      gridTemplateColumns: isDesktop ? "1fr 1fr" : "1fr",
      gap: isDesktop ? 32 : 24,
      animation: "planner-fadeUp 0.4s ease both",
    }}>
      {/* Weekly distribution */}
      <div>
        <h2 style={{
          fontSize: 18, fontWeight: 700, color: "#e2e8f0",
          margin: "0 0 16px",
          display: "flex", alignItems: "center", gap: 10,
        }}>
          <span style={{
            width: 3, height: 20, background: "#7c3aed",
            borderRadius: 2, display: "inline-block",
          }} />
          Distribuição semanal
        </h2>
        <div style={{
          display: "grid",
          gridTemplateColumns: isDesktop ? "1fr 1fr" : "1fr",
          gap: 8,
        }}>
          {Object.entries(resumo).map(([key, s], idx) => {
            const c = COLORS[key] || COLORS.trabalho;
            return (
              <div
                key={key}
                className="planner-summary-card"
                style={{
                  display: "flex", alignItems: "center", gap: 12,
                  padding: "12px 14px",
                  background: `linear-gradient(135deg, ${c.bg}66, ${c.bg}33)`,
                  borderRadius: 10,
                  border: `1px solid ${c.border}22`,
                  transition: "all 0.2s ease",
                  animation: "planner-fadeUp 0.4s ease both",
                  animationDelay: `${idx * 60}ms`,
                }}
              >
                <span style={{ fontSize: 22 }}>{s.icon}</span>
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{
                    fontSize: 13, fontWeight: 600, color: c.text,
                    overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap",
                  }}>
                    {s.label}
                  </div>
                </div>
                <span style={{
                  fontSize: 15, fontWeight: 700, color: "#e2e8f0",
                  fontFamily: "'JetBrains Mono', monospace",
                  whiteSpace: "nowrap",
                }}>
                  {s.hours}
                </span>
              </div>
            );
          })}
        </div>
      </div>

      {/* Rules */}
      <div>
        <h2 style={{
          fontSize: 18, fontWeight: 700, color: "#e2e8f0",
          margin: "0 0 16px",
          display: "flex", alignItems: "center", gap: 10,
        }}>
          <span style={{
            width: 3, height: 20, background: "#7c3aed",
            borderRadius: 2, display: "inline-block",
          }} />
          Regras do mês
        </h2>
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          {regras.map((note, i) => (
            <div
              key={i}
              className="planner-rule"
              style={{
                fontSize: 13, color: "#b0b4d0", padding: "12px 16px",
                background: "#12101f", borderRadius: 10, lineHeight: 1.6,
                borderLeft: "3px solid #2a1f4a",
                transition: "all 0.2s ease",
                animation: "planner-fadeUp 0.4s ease both",
                animationDelay: `${i * 60}ms`,
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

export default function Planner() {
  const [selectedDay, setSelectedDay] = useState("seg");
  const [showSummary, setShowSummary] = useState(false);
  const isDesktop = useMediaQuery("(min-width: 900px)");

  useEffect(() => { injectKeyframes(); }, []);

  const { meta, dias, resumo, regras } = data;

  return (
    <div style={{
      minHeight: "100vh",
      background: "linear-gradient(170deg, #0d0a18 0%, #0c1220 40%, #0f0b1e 100%)",
      color: "#e2e8f0",
      fontFamily: "'Instrument Sans', 'Segoe UI', system-ui, sans-serif",
      padding: isDesktop ? "40px 48px" : "20px 16px",
      maxWidth: "none",
      margin: 0,
    }}>
      <link
        href="https://fonts.googleapis.com/css2?family=Instrument+Sans:wght@400;600;700&family=JetBrains+Mono:wght@400;500&display=swap"
        rel="stylesheet"
      />

      {/* Header */}
      <div style={{
        marginBottom: isDesktop ? 32 : 24,
        display: isDesktop ? "flex" : "block",
        alignItems: "flex-end",
        justifyContent: "space-between",
        gap: 20,
        animation: "planner-fadeUp 0.5s ease both",
      }}>
        <div>
          <div style={{
            fontSize: 11, letterSpacing: "0.15em",
            textTransform: "uppercase", color: "#6b5b95", fontWeight: 700,
          }}>
            planejamento {meta.periodo.toLowerCase()}
          </div>
          <h1 style={{
            fontSize: isDesktop ? 28 : 22,
            fontWeight: 700,
            margin: "6px 0 0",
            color: "#f1f5f9",
            lineHeight: 1.2,
          }}>
            {meta.titulo}
          </h1>
          <p style={{ fontSize: 13, color: "#6b7194", margin: "4px 0 0" }}>
            {meta.subtitulo}
          </p>
        </div>

        {/* Toggle */}
        <div style={{
          display: "flex", gap: 4,
          marginTop: isDesktop ? 0 : 20,
          background: "#0d0a18",
          borderRadius: 10,
          padding: 3,
          border: "1px solid #1e1535",
          flexShrink: 0,
        }}>
          {[
            { key: false, label: "Agenda" },
            { key: true, label: "Resumo & Regras" },
          ].map(({ key, label }) => (
            <button
              key={label}
              className={showSummary === key ? "" : "planner-toggle-btn"}
              onClick={() => setShowSummary(key)}
              style={{
                padding: "9px 20px", fontSize: 12, fontWeight: 700,
                border: "none", borderRadius: 8, cursor: "pointer",
                background: showSummary === key
                  ? "linear-gradient(135deg, #2a1f4a, #1e1535)"
                  : "transparent",
                color: showSummary === key ? "#d8b4fe" : "#4a4d6a",
                transition: "all 0.2s ease",
                fontFamily: "inherit",
              }}
            >
              {label}
            </button>
          ))}
        </div>
      </div>

      {/* Content */}
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

      {/* Meta do mês footer */}
      <div style={{
        marginTop: isDesktop ? 32 : 24,
        padding: "16px 20px",
        background: "linear-gradient(135deg, #12101f, #1a1230)",
        borderRadius: 12,
        border: "1px solid #1e1535",
        animation: "planner-fadeUp 0.5s ease both",
        animationDelay: "200ms",
      }}>
        <div style={{
          fontSize: 11, color: "#6b5b95", textTransform: "uppercase",
          fontWeight: 700, letterSpacing: "0.12em", marginBottom: 6,
        }}>
          Meta do mês
        </div>
        <div style={{
          fontSize: 13, color: "#fde68a", lineHeight: 1.6,
          fontWeight: 500,
        }}>
          {meta.metaDoMes}
        </div>
      </div>
    </div>
  );
}

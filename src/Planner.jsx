import { useState } from "react";
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

function Tag({ type, children }) {
  const c = COLORS[type] || COLORS.trabalho;
  return (
    <span style={{
      display: "inline-block", fontSize: 10, fontWeight: 700,
      letterSpacing: "0.05em", textTransform: "uppercase",
      color: c.text, background: c.bg,
      border: `1px solid ${c.border}44`, borderRadius: 4,
      padding: "2px 7px", marginBottom: 2,
    }}>
      {children}
    </span>
  );
}

function Block({ time, title, type, note }) {
  const c = COLORS[type] || COLORS.trabalho;
  return (
    <div style={{
      borderLeft: `3px solid ${c.border}`, padding: "6px 10px",
      marginBottom: 6, background: `${c.bg}55`, borderRadius: "0 6px 6px 0",
    }}>
      <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <span style={{
          fontSize: 11, color: "#94a3b8", fontVariantNumeric: "tabular-nums",
          minWidth: 80, fontFamily: "'JetBrains Mono', monospace",
        }}>
          {time}
        </span>
        <Tag type={type}>{type === "exercicio" ? "exercício" : type}</Tag>
      </div>
      <div style={{ fontSize: 13, color: "#e2e8f0", fontWeight: 600, marginTop: 2 }}>
        {title}
      </div>
      {note && (
        <div style={{ fontSize: 11, color: "#94a3b8", marginTop: 1, fontStyle: "italic" }}>
          {note}
        </div>
      )}
    </div>
  );
}

export default function Planner() {
  const [selectedDay, setSelectedDay] = useState("seg");
  const [showSummary, setShowSummary] = useState(false);

  const { meta, dias, resumo, regras } = data;

  return (
    <div style={{
      minHeight: "100vh", background: "#0c1220", color: "#e2e8f0",
      fontFamily: "'Instrument Sans', 'Segoe UI', system-ui, sans-serif",
      padding: "20px 16px", maxWidth: 520, margin: "0 auto",
    }}>
      <link
        href="https://fonts.googleapis.com/css2?family=Instrument+Sans:wght@400;600;700&family=JetBrains+Mono:wght@400;500&display=swap"
        rel="stylesheet"
      />

      {/* Header */}
      <div style={{ marginBottom: 24 }}>
        <div style={{
          fontSize: 11, letterSpacing: "0.15em",
          textTransform: "uppercase", color: "#64748b", fontWeight: 700,
        }}>
          planejamento {meta.periodo.toLowerCase()}
        </div>
        <h1 style={{ fontSize: 22, fontWeight: 700, margin: "4px 0 0", color: "#f1f5f9", lineHeight: 1.2 }}>
          {meta.titulo}
        </h1>
        <p style={{ fontSize: 12, color: "#64748b", margin: "4px 0 0" }}>
          {meta.subtitulo}
        </p>
      </div>

      {/* Toggle */}
      <div style={{ display: "flex", gap: 6, marginBottom: 16 }}>
        {[
          { key: false, label: "Agenda" },
          { key: true, label: "Resumo & Regras" },
        ].map(({ key, label }) => (
          <button
            key={label}
            onClick={() => setShowSummary(key)}
            style={{
              flex: 1, padding: "8px 0", fontSize: 12, fontWeight: 700,
              border: "none", borderRadius: 6, cursor: "pointer",
              background: showSummary === key ? "#1e3a5f" : "#1a1f2e",
              color: showSummary === key ? "#93c5fd" : "#64748b",
              transition: "all .2s",
            }}
          >
            {label}
          </button>
        ))}
      </div>

      {!showSummary ? (
        <>
          {/* Day selector */}
          <div style={{ display: "flex", gap: 4, marginBottom: 16, overflowX: "auto" }}>
            {Object.entries(DAYS_LABELS).map(([key, label]) => {
              const active = selectedDay === key;
              const isWeekend = key === "sab" || key === "dom";
              return (
                <button
                  key={key}
                  onClick={() => setSelectedDay(key)}
                  style={{
                    flex: "0 0 auto", minWidth: 52, padding: "10px 6px",
                    fontSize: 11, fontWeight: active ? 700 : 500,
                    border: active ? "1.5px solid #2563eb" : "1px solid #1e293b",
                    borderRadius: 8, cursor: "pointer",
                    background: active ? "#1e3a5f" : isWeekend ? "#111827" : "#151b2b",
                    color: active ? "#93c5fd" : isWeekend ? "#475569" : "#94a3b8",
                    transition: "all .15s",
                  }}
                >
                  {label.slice(0, 3)}
                </button>
              );
            })}
          </div>

          <h2 style={{ fontSize: 16, fontWeight: 700, color: "#f1f5f9", margin: "0 0 12px" }}>
            {DAYS_LABELS[selectedDay]}
          </h2>

          <div>
            {(dias[selectedDay] || []).map((block, i) => (
              <Block key={i} {...block} />
            ))}
          </div>
        </>
      ) : (
        <>
          <h2 style={{ fontSize: 16, fontWeight: 700, color: "#f1f5f9", margin: "0 0 12px" }}>
            Distribuição semanal
          </h2>
          <div style={{ display: "flex", flexDirection: "column", gap: 6, marginBottom: 20 }}>
            {Object.entries(resumo).map(([key, s]) => {
              const c = COLORS[key] || COLORS.trabalho;
              return (
                <div key={key} style={{
                  display: "flex", alignItems: "center", gap: 10,
                  padding: "8px 12px", background: `${c.bg}88`,
                  borderRadius: 8, border: `1px solid ${c.border}33`,
                }}>
                  <span style={{ fontSize: 18 }}>{s.icon}</span>
                  <div style={{ flex: 1 }}>
                    <div style={{ fontSize: 13, fontWeight: 600, color: c.text }}>{s.label}</div>
                  </div>
                  <span style={{
                    fontSize: 14, fontWeight: 700, color: "#e2e8f0",
                    fontFamily: "'JetBrains Mono', monospace",
                  }}>
                    {s.hours}
                  </span>
                </div>
              );
            })}
          </div>

          <h2 style={{ fontSize: 16, fontWeight: 700, color: "#f1f5f9", margin: "0 0 12px" }}>
            Regras do mês
          </h2>
          <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
            {regras.map((note, i) => (
              <div key={i} style={{
                fontSize: 12, color: "#cbd5e1", padding: "10px 12px",
                background: "#151b2b", borderRadius: 8, lineHeight: 1.5,
                borderLeft: "3px solid #334155",
              }}>
                {note}
              </div>
            ))}
          </div>
        </>
      )}

      <div style={{
        marginTop: 24, padding: "12px", background: "#111827",
        borderRadius: 8, border: "1px solid #1e293b",
      }}>
        <div style={{
          fontSize: 11, color: "#64748b", textTransform: "uppercase",
          fontWeight: 700, letterSpacing: "0.1em", marginBottom: 6,
        }}>
          Meta do mês
        </div>
        <div style={{ fontSize: 12, color: "#fde68a", lineHeight: 1.6 }}>
          {meta.metaDoMes}
        </div>
      </div>
    </div>
  );
}

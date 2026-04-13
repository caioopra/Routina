# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

Projeto Vite + React para visualização de rotina semanal. Single-page app com um único componente (`Planner`) que renderiza dados de um JSON. Sem roteamento, sem state management externo, sem CSS externo — todo estilo é inline.

## Comandos

```bash
npm install
npm run dev      # dev server em localhost:5173 (hot reload)
npm run build    # build estática em dist/
npm run preview  # serve a build local
```

## Estrutura

- `src/rotina.json` — **Todos os dados da rotina.** Para alterar horários, blocos, regras ou resumo, edite apenas este arquivo.
- `src/Planner.jsx` — Componente de visualização. Só mexer aqui se quiser mudar o layout/design.
- `src/main.jsx` — Entry point (não precisa mexer).

## Como editar a rotina

O arquivo `src/rotina.json` tem esta estrutura:

- `meta` — título, subtítulo, período, meta do mês
- `dias` — objeto com chaves `seg`, `ter`, `qua`, `qui`, `sex`, `sab`, `dom`, cada uma com array de blocos
- `resumo` — horas totais por categoria (cada entrada tem `label`, `hours`, `icon`)
- `regras` — array de strings com as regras do mês

Cada bloco de um dia tem:
```json
{
  "time": "09:00–12:00",
  "title": "Trabalho (bloco focado)",
  "type": "trabalho",
  "note": "Opcional — dica ou contexto"
}
```

Types válidos: `trabalho`, `mestrado`, `aula`, `exercicio`, `slides`, `viagem`, `livre`.

## Arquitetura do Planner.jsx

O componente tem duas views alternadas por toggle:
- **Agenda** — seletor de dia da semana + lista de blocos do dia selecionado
- **Resumo & Regras** — distribuição semanal de horas + regras do mês

Cores por tipo de bloco são definidas no objeto `COLORS` no topo do arquivo. Para adicionar um novo type, inclua uma entrada em `COLORS` com `bg`, `text` e `border`.

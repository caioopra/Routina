//! System prompt builders.
//!
//! All prompts are written in PT-BR because the assistant speaks Portuguese to
//! the end user.  Code, logs, and comments remain in English.
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Context structs
// ---------------------------------------------------------------------------

/// Information about the authenticated user, injected into every system prompt.
///
/// `planner_context` is a free-text "about me" authored by the user (job,
/// weekly intent, long-term goals).  When `None` or empty the corresponding
/// prompt section is omitted entirely.
#[derive(Debug, Clone)]
pub struct UserContext {
    pub name: String,
    pub planner_context: Option<String>,
}

/// Minimal description of the routine this conversation is locked to.
///
/// Slice C will extend this with full block/rule state.  For now only the
/// identifying fields are needed so the prompt can declare the conversation scope.
#[derive(Debug, Clone)]
pub struct RoutineContext {
    pub id: Uuid,
    pub name: String,
    pub period: Option<String>,
}

// ---------------------------------------------------------------------------
// Planner system prompt
// ---------------------------------------------------------------------------

/// Render the main system prompt for the planning assistant.
///
/// The prompt declares the conversation is locked to a single routine and
/// injects the user's `planner_context` verbatim when present.
pub fn planner_system_prompt(user: &UserContext, routine: &RoutineContext) -> String {
    // Build the optional "about the user" section.
    let user_context_section = match user
        .planner_context
        .as_deref()
        .filter(|s| !s.trim().is_empty())
    {
        Some(ctx) => format!("\n\n## Sobre o usuário\n{ctx}"),
        None => String::new(),
    };

    let period = routine.period.as_deref().unwrap_or("sem período definido");

    format!(
        r#"Você é o Assistente de Planejamento Semanal do usuário {user_name}.
Seu objetivo é ajudar o usuário a criar, visualizar e ajustar sua rotina semanal
de forma conversacional, usando as ferramentas disponíveis para modificar os dados.

## Idioma
Sempre responda em Português do Brasil (PT-BR), independente do idioma da mensagem recebida.

## Escopo desta conversa
Esta conversa está bloqueada para a rotina **{routine_name}** (período: {period}, ID: `{routine_id}`).
Você deve referenciar exclusivamente esta rotina em todas as chamadas de ferramenta.
Não mencione nem modifique outras rotinas do usuário.

## Domínio — entidades que você gerencia

**Rotina** (`Routine`): Uma agenda semanal com nome e período (ex.: "Semestre 2026.1").
Cada usuário pode ter várias rotinas; esta conversa cobre apenas a listada acima.

**Bloco** (`Block`): Uma atividade dentro de uma rotina, associada a um dia da semana
(0=Dom … 6=Sáb), horário de início, horário de fim opcional, título, tipo e notas.
Tipos válidos: `trabalho`, `mestrado`, `aula`, `exercicio`, `slides`, `viagem`, `livre`.

**Label** (`Label`): Uma etiqueta colorida que pode ser associada a um ou mais blocos
para categorização visual. Cada usuário tem etiquetas padrão e pode criar as suas.

**Regra** (`Rule`): Uma restrição ou diretriz de planejamento em texto livre
(ex.: "Não marcar reuniões antes das 10h"). Ajuda o assistente a respeitar
preferências ao sugerir mudanças.

## Comportamento esperado

1. Use **somente as ferramentas disponíveis** para criar, alterar ou excluir dados.
   Nunca invente IDs — use apenas IDs retornados pelas ferramentas ou fornecidos
   pelo sistema.

2. Para operações **destrutivas** (deletar bloco, deletar regra, limpar rotina),
   peça confirmação explícita ao usuário antes de chamar a ferramenta.

3. Após aplicar mudanças, **resuma o que foi feito** em linguagem natural amigável.

4. Se o usuário pedir algo que viole uma regra existente, **avise-o** antes de
   prosseguir e peça confirmação.

5. Mantenha respostas objetivas. Use listas quando listar múltiplos blocos ou
   mudanças. Evite textos longos sem ação.

## Regras de uso de ferramentas

Você tem acesso a um conjunto de ferramentas que modificam a rotina diretamente.
As regras abaixo são obrigatórias:

1. **Mutações são imediatas.** Toda chamada de ferramenta bem-sucedida altera o
   banco de dados na hora. Após executar uma ferramenta, sempre resuma em PT-BR
   o que foi feito (ex.: "Criei o bloco 'Academia' na segunda-feira às 07:00.").

2. **Para desfazer, use `undo_last_action`.** Se o usuário pedir para desfazer,
   cancelar ou "desfazer" a última ação, chame `undo_last_action` imediatamente.
   Nunca tente reverter manualmente construindo chamadas opostas (ex.: não delete
   um bloco recém-criado "na mão" — use o undo).

3. **Nunca invente IDs.** Antes de atualizar ou deletar um bloco ou regra,
   chame `list_blocks` ou `list_rules` para obter os IDs reais do banco.
   Jamais referencie a rotina de outro usuário.

4. **Prefira sequências em lote.** Quando uma solicitação do usuário exigir
   múltiplas mutações, faça todas as chamadas de ferramenta necessárias em
   sequência dentro da mesma resposta; evite perguntar ao usuário entre cada passo.

5. **A lista de ferramentas disponíveis** (nomes e parâmetros) já é enviada pela
   API; não há necessidade de listá-las neste prompt.{user_context_section}"#,
        user_name = user.name,
        routine_name = routine.name,
        period = period,
        routine_id = routine.id,
        user_context_section = user_context_section,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn make_user(name: &str, planner_context: Option<&str>) -> UserContext {
        UserContext {
            name: name.to_string(),
            planner_context: planner_context.map(str::to_string),
        }
    }

    fn make_routine(name: &str, period: Option<&str>) -> RoutineContext {
        RoutineContext {
            id: Uuid::now_v7(),
            name: name.to_string(),
            period: period.map(str::to_string),
        }
    }

    // Helper that builds a prompt with sensible defaults so tests don't all
    // need to specify every field.
    fn default_prompt() -> String {
        planner_system_prompt(
            &make_user("Caio", None),
            &make_routine("Rotina Teste", Some("2026.1")),
        )
    }

    #[test]
    fn prompt_contains_user_name() {
        let prompt = default_prompt();
        assert!(prompt.contains("Caio"));
    }

    #[test]
    fn prompt_is_ptbr() {
        let prompt = default_prompt();
        assert!(prompt.contains("Português do Brasil"));
        assert!(prompt.contains("rotina"));
    }

    #[test]
    fn prompt_with_routine_includes_name_and_id() {
        let routine = make_routine("Semestre 2026", Some("2026.1"));
        let prompt = planner_system_prompt(&make_user("Caio", None), &routine);
        assert!(prompt.contains("Semestre 2026"), "routine name missing");
        assert!(prompt.contains("2026.1"), "period missing");
        assert!(
            prompt.contains(&routine.id.to_string()),
            "routine ID missing"
        );
    }

    #[test]
    fn prompt_routine_name_appears_in_scope_declaration() {
        let routine = make_routine("Rotina de Verão", Some("2026.2"));
        let prompt = planner_system_prompt(&make_user("Caio", None), &routine);
        // The scope section uses the routine name in bold markdown.
        assert!(
            prompt.contains("**Rotina de Verão**"),
            "routine name not in scope declaration"
        );
    }

    #[test]
    fn prompt_declares_conversation_locked_to_routine() {
        let prompt = default_prompt();
        // Key sentence that locks the conversation.
        assert!(
            prompt.contains("bloqueada para a rotina"),
            "prompt does not declare conversation is locked to a routine"
        );
        assert!(
            prompt.contains("Não mencione nem modifique outras rotinas"),
            "prompt does not forbid referencing other routines"
        );
    }

    #[test]
    fn planner_context_present_appears_verbatim() {
        let ctx = "Sou engenheiro de software, trabalho das 9h às 18h. Objetivo: correr 5km 3x por semana.";
        let user = make_user("Caio", Some(ctx));
        let routine = make_routine("Rotina Teste", Some("2026.1"));
        let prompt = planner_system_prompt(&user, &routine);

        assert!(
            prompt.contains(ctx),
            "planner_context not found verbatim in prompt"
        );
        assert!(
            prompt.contains("## Sobre o usuário"),
            "section header '## Sobre o usuário' missing"
        );
    }

    #[test]
    fn planner_context_none_section_absent() {
        let user = make_user("Caio", None);
        let routine = make_routine("Rotina Teste", Some("2026.1"));
        let prompt = planner_system_prompt(&user, &routine);

        assert!(
            !prompt.contains("## Sobre o usuário"),
            "section '## Sobre o usuário' should be absent when planner_context is None"
        );
    }

    #[test]
    fn planner_context_empty_string_section_absent() {
        let user = make_user("Caio", Some("   "));
        let routine = make_routine("Rotina Teste", Some("2026.1"));
        let prompt = planner_system_prompt(&user, &routine);

        assert!(
            !prompt.contains("## Sobre o usuário"),
            "section '## Sobre o usuário' should be absent when planner_context is empty/whitespace"
        );
    }

    #[test]
    fn prompt_instructs_no_fabricated_ids() {
        let prompt = default_prompt();
        assert!(prompt.contains("invente IDs"));
    }

    #[test]
    fn prompt_instructs_confirm_destructive() {
        let prompt = default_prompt();
        assert!(prompt.contains("confirmação"));
    }

    #[test]
    fn prompt_length_under_900_tokens_approx() {
        // Rough token estimate: ~4 chars per token.
        // Limit bumped to 900 to accommodate the new scope section.
        let prompt = default_prompt();
        let estimated_tokens = prompt.len() / 4;
        assert!(
            estimated_tokens < 900,
            "Prompt is too long: ~{estimated_tokens} tokens (limit 900). len={}",
            prompt.len()
        );
    }

    #[test]
    fn prompt_mentions_block_types() {
        let prompt = default_prompt();
        for block_type in &["trabalho", "exercicio", "aula", "livre"] {
            assert!(
                prompt.contains(block_type),
                "Expected block type '{block_type}' in prompt"
            );
        }
    }

    #[test]
    fn prompt_with_no_period_shows_fallback() {
        let routine = make_routine("Rotina Simples", None);
        let prompt = planner_system_prompt(&make_user("Caio", None), &routine);
        assert!(prompt.contains("sem período definido"));
    }

    // ---- Tool-use section tests (Slice C) -----------------------------------

    #[test]
    fn prompt_contains_tool_use_section() {
        let prompt = default_prompt();
        assert!(
            prompt.contains("## Regras de uso de ferramentas"),
            "prompt must contain the tool-use rules section header"
        );
    }

    #[test]
    fn prompt_tool_use_section_contains_ferramentas_keyword() {
        let prompt = default_prompt();
        assert!(
            prompt.contains("ferramentas"),
            "prompt tool-use section must contain 'ferramentas'"
        );
    }

    #[test]
    fn prompt_tool_use_section_contains_desfazer_keyword() {
        let prompt = default_prompt();
        assert!(
            prompt.contains("desfazer"),
            "prompt tool-use section must contain 'desfazer'"
        );
    }

    #[test]
    fn prompt_tool_use_section_mentions_undo_last_action() {
        let prompt = default_prompt();
        assert!(
            prompt.contains("undo_last_action"),
            "prompt must mention 'undo_last_action' tool in PT-BR instructions"
        );
    }

    #[test]
    fn prompt_tool_use_mutations_are_immediate() {
        let prompt = default_prompt();
        // The prompt must tell the LLM that mutations are immediate.
        assert!(
            prompt.contains("imediatas") || prompt.contains("imediata"),
            "prompt must declare that mutations are immediate"
        );
    }

    #[test]
    fn prompt_tool_use_no_manual_reverse() {
        let prompt = default_prompt();
        // The prompt must forbid manually reversing mutations.
        assert!(
            prompt.contains("manualmente"),
            "prompt must warn against manually reversing mutations"
        );
    }

    #[test]
    fn prompt_tool_use_batch_preference() {
        let prompt = default_prompt();
        // The prompt must advise batching multiple tool calls.
        assert!(
            prompt.contains("lote") || prompt.contains("sequência"),
            "prompt must mention batching/sequence preference"
        );
    }

    #[test]
    fn prompt_routine_name_still_appears_after_tool_section_added() {
        let routine = make_routine("Rotina de Inverno", Some("2026.2"));
        let prompt = planner_system_prompt(&make_user("Caio", None), &routine);
        assert!(
            prompt.contains("Rotina de Inverno"),
            "routine name must still appear in prompt after adding tool-use section"
        );
    }

    #[test]
    fn prompt_length_under_1500_tokens_approx_with_tool_section() {
        // The tool-use section adds ~300 tokens; 1500-token ceiling is generous.
        // We keep the original 900-token test for the base prompt (no context),
        // but add this separate one to confirm the ceiling is still reasonable.
        let prompt = default_prompt();
        let estimated_tokens = prompt.len() / 4;
        assert!(
            estimated_tokens < 1500,
            "Prompt with tool section is too long: ~{estimated_tokens} tokens (limit 1500). len={}",
            prompt.len()
        );
    }
}

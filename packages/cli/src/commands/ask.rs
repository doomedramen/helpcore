//! `hc ask` — one-shot question with streaming response.

use anyhow::{Context, bail};
use helpcore_api::{
    ChatRequest, InteractionPayload, InteractionQuestionType, InteractionResponse,
    PendingInteraction, QuestionAnswer,
};
use std::io::{IsTerminal, Write};

use crate::{
    client::{ChatOutcome, Client},
    config::Credentials,
};

/// Sends a question to the AI and streams the response to stdout. If no
/// `conversation_id` is given, a new conversation is created.
pub async fn run(
    query: &[String],
    conversation_id: Option<&str>,
    provider: Option<&str>,
    model: Option<&str>,
    server_flag: Option<&str>,
) -> anyhow::Result<()> {
    let mut creds = Credentials::load()?;

    let server = creds
        .resolve_server(server_flag)
        .ok_or_else(|| anyhow::anyhow!("no server configured — run 'helpcore login' first"))?;

    let mut access_token = creds
        .access_token
        .clone()
        .ok_or_else(|| anyhow::anyhow!("not logged in — run 'helpcore login' first"))?;

    let message = query.join(" ");
    if message.trim().is_empty() {
        bail!("message cannot be empty");
    }

    let client = Client::new(&server);
    let request = ChatRequest {
        conversation_id: conversation_id.map(|s| s.to_string()),
        message,
        provider_id: provider.map(|s| s.to_string()),
        model: model.map(|s| s.to_string()),
    };

    let is_new_conversation = conversation_id.is_none();

    // Attempt chat; if the access token has expired, try refreshing once.
    let mut outcome = match client.chat(&access_token, &request, emit_chunk).await {
        Ok(outcome) => outcome,
        Err(e) if Client::is_token_expired(&e) => {
            let refresh_token = creds
                .refresh_token
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("session expired — run 'hc login'"))?;
            let tokens = client
                .refresh(refresh_token)
                .await
                .map_err(|_| anyhow::anyhow!("session expired — run 'hc login'"))?;
            let new_access = tokens.access_token.clone();
            access_token = new_access.clone();
            creds.access_token = Some(new_access.clone());
            creds.refresh_token = Some(tokens.refresh_token);
            let _ = creds.save(); // best-effort — don't fail the whole request on disk error
            client.chat(&new_access, &request, emit_chunk).await?
        }
        Err(e) => return Err(e),
    };

    loop {
        match outcome {
            ChatOutcome::Done(done) => {
                println!();
                if is_new_conversation {
                    eprintln!(
                        "\n→ conversation {} (use -c {} to continue)",
                        done.conversation_id, done.conversation_id
                    );
                }
                return Ok(());
            }
            ChatOutcome::InputRequired(required) => {
                println!();
                if !std::io::stdin().is_terminal() {
                    eprintln!(
                        "Conversation {} needs interactive input (interaction {}). \
                         Open it in the web UI to continue.",
                        required.interaction.conversation_id, required.interaction.id
                    );
                    return Ok(());
                }
                let response = prompt_for_interaction(&required.interaction)?;
                outcome = client
                    .respond_interaction(
                        &access_token,
                        &required.interaction.conversation_id,
                        &required.interaction.id,
                        &response,
                        emit_chunk,
                    )
                    .await?;
            }
        }
    }
}

fn prompt_for_interaction(interaction: &PendingInteraction) -> anyhow::Result<InteractionResponse> {
    match &interaction.payload {
        InteractionPayload::Questions { questions } => {
            let mut answers = Vec::with_capacity(questions.len());
            for (index, question) in questions.iter().enumerate() {
                eprintln!(
                    "\n{} ({}/{})",
                    question.question,
                    index + 1,
                    questions.len()
                );
                match question.question_type {
                    InteractionQuestionType::Text => {
                        let value = prompt_line("Answer (or 'esc' to dismiss): ")?;
                        if value.eq_ignore_ascii_case("esc") {
                            return Ok(InteractionResponse::Dismiss);
                        }
                        if value.is_empty() {
                            bail!("answer must not be empty");
                        }
                        answers.push(QuestionAnswer {
                            question_id: question.id.clone(),
                            option_ids: Vec::new(),
                            custom_response: Some(value),
                        });
                    }
                    InteractionQuestionType::SingleSelect
                    | InteractionQuestionType::MultiSelect => {
                        for (option_index, option) in question.options.iter().enumerate() {
                            eprintln!(
                                "  {}. {} — {}",
                                option_index + 1,
                                option.label,
                                option.description
                            );
                        }
                        let prompt = if question.question_type
                            == InteractionQuestionType::MultiSelect
                        {
                            "Choose numbers separated by commas, type another answer, or 'esc': "
                        } else {
                            "Choose one number, type another answer, or 'esc': "
                        };
                        let value = prompt_line(prompt)?;
                        if value.eq_ignore_ascii_case("esc") {
                            return Ok(InteractionResponse::Dismiss);
                        }
                        let parsed_indexes = value
                            .split(',')
                            .map(|part| {
                                part.trim()
                                    .parse::<usize>()
                                    .context("choices must be option numbers")
                            })
                            .collect::<anyhow::Result<Vec<_>>>();
                        let (option_ids, custom_response) = match parsed_indexes {
                            Ok(indexes)
                                if !indexes.is_empty()
                                    && (question.question_type
                                        == InteractionQuestionType::MultiSelect
                                        || indexes.len() == 1) =>
                            {
                                let option_ids = indexes
                                    .into_iter()
                                    .map(|choice| {
                                        question
                                            .options
                                            .get(choice.saturating_sub(1))
                                            .map(|option| option.id.clone())
                                            .context("option number is out of range")
                                    })
                                    .collect::<anyhow::Result<Vec<_>>>()?;
                                (option_ids, None)
                            }
                            _ if !value.is_empty() => (Vec::new(), Some(value)),
                            _ => bail!("answer must not be empty"),
                        };
                        answers.push(QuestionAnswer {
                            question_id: question.id.clone(),
                            option_ids,
                            custom_response,
                        });
                    }
                }
            }
            Ok(InteractionResponse::Questions { answers })
        }
        InteractionPayload::PluginApproval { plugin } => {
            eprintln!("\n{}: {}", plugin.name, plugin.rationale);
            eprintln!("Action: {}", plugin.action.replace('_', " "));
            if !plugin.permissions.is_empty() {
                eprintln!("Permissions: {}", plugin.permissions.join(", "));
            }
            if !plugin.allowed_hosts.is_empty() {
                eprintln!("Allowed hosts: {}", plugin.allowed_hosts.join(", "));
            }
            if let Some(guide) = &plugin.setup_guide {
                eprintln!("Setup guide: {guide}");
            }
            let value = prompt_line("Approve? [Y/n/esc]: ")?;
            if value.eq_ignore_ascii_case("esc") {
                Ok(InteractionResponse::Dismiss)
            } else {
                Ok(InteractionResponse::PluginApproval {
                    approved: value.is_empty()
                        || value.eq_ignore_ascii_case("y")
                        || value.eq_ignore_ascii_case("yes"),
                })
            }
        }
        InteractionPayload::PluginConfig {
            plugin,
            fields,
            current_values,
            error,
        } => {
            eprintln!("\nConfigure {}", plugin.name);
            if let Some(error) = error {
                eprintln!("Previous error: {error}");
            }
            let current = current_values.as_object();
            let mut values = serde_json::Map::new();
            for field in fields {
                let configured_secret = current
                    .and_then(|map| map.get(&field.key))
                    .and_then(|value| value.get("configured"))
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                let prompt = format!(
                    "{}{}{}: ",
                    field.label,
                    if field.required { " *" } else { "" },
                    field
                        .default
                        .as_deref()
                        .map(|value| format!(" [{value}]"))
                        .unwrap_or_default()
                );
                let raw = if field.field_type == "secret" {
                    rpassword::prompt_password(if configured_secret {
                        format!("{} (blank keeps existing): ", field.label)
                    } else {
                        prompt.clone()
                    })?
                } else {
                    prompt_line(&prompt)?
                };
                if raw.eq_ignore_ascii_case("esc") && field.field_type != "secret" {
                    return Ok(InteractionResponse::Dismiss);
                }
                if raw.is_empty() {
                    if field.field_type == "secret" && configured_secret {
                        continue;
                    }
                    if let Some(default) = &field.default {
                        values.insert(field.key.clone(), typed_config_value(field, default)?);
                        continue;
                    }
                    if !field.required {
                        continue;
                    }
                }
                values.insert(field.key.clone(), typed_config_value(field, &raw)?);
            }
            Ok(InteractionResponse::PluginConfig {
                values: serde_json::Value::Object(values),
            })
        }
    }
}

/// Prints a streaming token to stdout immediately (no newline).
fn emit_chunk(delta: &str) {
    print!("{delta}");
    let _ = std::io::stdout().flush();
}

fn prompt_line(prompt: &str) -> anyhow::Result<String> {
    eprint!("{prompt}");
    std::io::stderr().flush()?;
    let mut value = String::new();
    std::io::stdin().read_line(&mut value)?;
    Ok(value.trim().to_string())
}

fn typed_config_value(
    field: &helpcore_api::ConfigField,
    value: &str,
) -> anyhow::Result<serde_json::Value> {
    match field.field_type.as_str() {
        "number" => {
            Ok(serde_json::json!(value.parse::<f64>().with_context(
                || format!("{} must be a number", field.label)
            )?))
        }
        "boolean" => Ok(serde_json::json!(matches!(
            value.to_ascii_lowercase().as_str(),
            "true" | "yes" | "y" | "1" | "on"
        ))),
        "select" if !field.options.iter().any(|option| option == value) => {
            bail!(
                "{} must be one of: {}",
                field.label,
                field.options.join(", ")
            )
        }
        _ => Ok(serde_json::Value::String(value.to_string())),
    }
}

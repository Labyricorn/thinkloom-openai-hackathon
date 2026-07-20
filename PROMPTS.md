# Thinkloom prompt configuration

Thinkloom 0.5.11 exposes every instruction sent to a language model as editable JSON. The desktop app creates a prompts folder in its operating-system configuration directory and shows its exact path under Settings → Prompt configuration. Use Open prompt folder to open it.

Prompt files are loaded immediately before every model request. Save a valid edit, then make the next request; no restart or rebuild is required. Thinkloom preserves existing prompt instructions. When a release introduces a required prompt field, it adds only the missing field and wraps legacy conversation instructions with the new modular context variables.

## Files and effects

### conversation.json

Affects replies in Ideation. systemPrompt defines the overall role, userPromptTemplate defines the turn task, and challengeGuidance supplies the Gentle, Balanced, or Rigorous instruction. The {{persona_instruction}}, {{genre_instruction}}, {{lore_context}}, {{web_search_instruction}}, {{challenge_guidance}}, and {{context}} placeholders are required. Together they form the modular system instruction for each session.

### drafting.json

Affects passage previews in Drafting and editorial previews in Finalization. systemPrompt defines the overall role, draftPromptTemplate is used by Draft a passage, editorialPromptTemplate is used by the editorial actions, and distillationPromptTemplate turns the Phase 1 drafting paper into a token-efficient handoff. Available placeholders are {{relation}}, {{action}}, and {{context}}.

The description, effect, and variables objects document the configuration and are not sent to the model.

## Editing safely

1. You do not need to close Thinkloom; prompt files reload automatically.
2. Back up a JSON file before a substantial change.
3. Edit string values, preserving double quotes, commas, escaped line breaks as \n, and double-brace placeholders.
4. Save the file and make a new request in the affected screen.

Thinkloom validates the schema, required fields, challenge level, and unresolved placeholders before contacting the provider. An invalid file leaves the writing untouched and displays the file path and correction needed.

## Resetting a prompt

Close Thinkloom, rename or delete only the affected JSON file in the user prompt folder, then reopen Thinkloom. The missing file is recreated from the bundled default. Source defaults for developers are in src-tauri/prompts/; rebuilding them does not overwrite an existing user's customized files.

## Privacy and security

Prompt content and substituted context are sent only to the provider selected in Settings. Local Ollama requests remain local. Cloud providers still require project approval. Do not put passwords or API keys in prompt files.
---
name: cs-ask
description: Ask a natural language question about the codebase. Translates to graph queries automatically. Use when user asks questions in Turkish or English about code structure.
user-invocable: true
argument-hint: "<question in any language>"
---

# Ask Codescope

Ask a question about the codebase in natural language. Works in Turkish and English.

Use the `ask` MCP tool with the question: **$ARGUMENTS**

If the `ask` tool doesn't return good results, try breaking it down:
1. For "who calls X?" → use `find_callers`
2. For "find X" → use `search_functions` or `find_function`
3. For "what's in file X?" → use `file_entities`
4. For complex queries → use `raw_query` with SurrealQL

## Example Questions

Turkish:
- "auth ile ilgili fonksiyonlar neler?"
- "bu fonksiyonu kim cagiriyor?"
- "en buyuk 5 fonksiyon hangisi?"

English:
- "what functions handle authentication?"
- "show me the dependency chain for UserService"
- "which files have the most functions?"

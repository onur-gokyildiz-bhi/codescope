---
name: cs-impact
description: Analyze the impact of changing a function. Shows callers, dependencies, and affected files. Use when user asks what happens if they change or refactor something.
user-invocable: true
argument-hint: "<function-name>"
---

# Impact Analysis

Analyze what would be affected if a function is changed.

Use the `impact_analysis` MCP tool with function name: **$ARGUMENTS**

If no arguments given, ask which function to analyze.

## Display Format

Show results grouped:

### Direct Callers
- List functions that directly call this function

### Affected Files
- List files that would need changes

### Risk Assessment
- High: many callers across many files
- Medium: several callers in few files
- Low: few or no callers

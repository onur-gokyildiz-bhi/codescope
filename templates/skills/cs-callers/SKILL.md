---
name: cs-callers
description: Find all callers of a function. Use when user asks who calls a function, wants to trace call chains, or asks about function usage.
user-invocable: true
argument-hint: "<function-name>"
---

# Find Callers

Find all functions that call the given function using the `find_callers` MCP tool.

Function name: **$ARGUMENTS**

If no arguments given, ask which function to trace.

## Display Format

Show as a call tree:

```
handleRequest is called by:
  <- routeHandler      (src/routes.ts:45)
  <- middleware         (src/middleware.ts:12)
  <- testHandleRequest (tests/routes.test.ts:8)
```

If no callers found, mention it might be a top-level entry point or unused.
